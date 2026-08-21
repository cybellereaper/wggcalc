use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use serde_json::json;
use tempfile::tempdir;
use weirdgungamecalc::sheet_parser::{
    detect_price_type, extract_leading_token, normalize_numeric_value, CoresParser, PartsParser,
    SheetDownloader, SheetExport,
};

fn blank_row(width: usize) -> Vec<String> {
    vec![String::new(); width]
}

#[test]
fn normalizes_numeric_units_multipliers_and_ranges() {
    assert_eq!(
        normalize_numeric_value("10", false).unwrap().as_f64(),
        Some(10.0)
    );
    assert_eq!(
        normalize_numeric_value("2x3", false).unwrap().as_f64(),
        Some(6.0)
    );
    assert_eq!(
        normalize_numeric_value("  12.5% ", false).unwrap().as_f64(),
        Some(12.5)
    );
    assert_eq!(
        normalize_numeric_value("100 rpm", false).unwrap().as_f64(),
        Some(100.0)
    );
    assert!(normalize_numeric_value("🎲", false).unwrap().is_null());
    assert_eq!(
        normalize_numeric_value("5 - 10", true).unwrap(),
        json!([5.0, 10.0])
    );
    assert_eq!(
        normalize_numeric_value("23 >", true).unwrap(),
        json!([23.0, null])
    );
}

#[test]
fn rejects_malformed_numbers_and_ranges() {
    assert!(normalize_numeric_value("not-a-number", false).is_err());
    assert!(normalize_numeric_value("10", true).is_err());
    assert!(normalize_numeric_value("10 - nope", true).is_err());
}

#[test]
fn detects_price_types_and_aliases() {
    assert_eq!(detect_price_type(""), "Coin");
    assert_eq!(detect_price_type("Weird Boxes"), "WC");
    assert_eq!(detect_price_type("Exclusive Weird Boxes"), "Robux");
    assert_eq!(detect_price_type("12345"), "Coin");
    assert_eq!(detect_price_type("verify discord"), "Verify discord");
    assert_eq!(detect_price_type("mystery"), "Unknown");
}

#[test]
fn extracts_leading_property_token_and_rejects_invalid_cells() {
    assert_eq!(extract_leading_token("10 stat").unwrap(), "10");
    assert!(extract_leading_token("10").is_err());
}

#[test]
fn parses_rows_into_categorized_parts() {
    let mut header = blank_row(17);
    header[0] = "Coin".into();
    header[1] = "AR Barrels".into();

    let mut part = blank_row(17);
    part[0] = "WC".into();
    part[1] = "Test Barrel".into();
    part[2] = "10 stat".into();
    part[4] = "5% damage".into();

    let output = PartsParser::new().parse_rows(&[header, part]).unwrap();
    assert_eq!(output["Barrels"].len(), 1);
    assert_eq!(output["Barrels"][0]["Name"], "Test Barrel");
    assert_eq!(output["Barrels"][0]["Magazine_Size"].as_f64(), Some(10.0));
    assert_eq!(output["Barrels"][0]["Damage"].as_f64(), Some(5.0));
}

#[test]
fn parts_parser_rejects_bad_rows_parts_before_headers_and_duplicates() {
    assert!(PartsParser::new()
        .parse_rows(&[vec![String::new(); 16]])
        .is_err());

    let mut orphan = blank_row(17);
    orphan[1] = "Orphan".into();
    assert!(PartsParser::new().parse_rows(&[orphan]).is_err());

    let mut header = blank_row(17);
    header[1] = "AR Barrels".into();
    let mut first = blank_row(17);
    first[1] = "Dup".into();
    first[2] = "10 stat".into();
    let second = first.clone();
    assert!(PartsParser::new()
        .parse_rows(&[header, first, second])
        .is_err());
}

#[test]
fn duplicate_part_names_are_allowed_in_different_category_sections() {
    let mut ar_header = blank_row(17);
    ar_header[1] = "AR Barrels".into();
    let mut ar_part = blank_row(17);
    ar_part[1] = "Shared Name".into();
    ar_part[2] = "1 stat".into();

    let mut smg_header = blank_row(17);
    smg_header[1] = "SMG Barrels".into();
    let smg_part = ar_part.clone();

    let output = PartsParser::new()
        .parse_rows(&[ar_header, ar_part, smg_header, smg_part])
        .unwrap();
    assert_eq!(output["Barrels"].len(), 2);
}

#[test]
fn cores_parser_handles_category_dividers_pellets_and_trailing_columns() {
    let mut divider = blank_row(18);
    divider[1] = "SMG Cores".into();

    let row = vec![
        "Coin",
        "Test Core",
        "10x3 > 8x3",
        "100 - 80",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "1 - 1",
        "1 - 1",
        "1 - 1",
        "1 - 1",
        "ignored extra",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();

    let output = CoresParser.parse_rows(&[divider, row]).unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["Name"], "Test Core");
    assert_eq!(output[0]["Category"], "SMG");
    assert_eq!(output[0]["Damage"], json!([30.0, 24.0]));
    assert_eq!(output[0]["Pellets"], 3);
}

#[test]
fn cores_parser_rejects_short_rows() {
    assert!(CoresParser.parse_rows(&[blank_row(17)]).is_err());
}

#[test]
fn sheet_downloader_follows_http_redirects() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept test request");
            let mut request = [0_u8; 2048];
            let bytes = stream.read(&mut request).expect("read test request");
            let request = String::from_utf8_lossy(&request[..bytes]);

            if request.starts_with("GET /redirect ") {
                write!(
                    stream,
                    "HTTP/1.1 307 Temporary Redirect\r\nLocation: /final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .expect("write redirect response");
            } else {
                let body = "name,stat\nx,y\n";
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .expect("write success response");
            }
        }
    });

    let directory = tempdir().expect("create temporary directory");
    let output = directory.path().join("cores.csv");
    let export = SheetExport {
        gid: "unused".into(),
        output_path: output.clone(),
        url_override: Some(format!("http://{address}/redirect")),
    };

    SheetDownloader::new("unused", directory.path())
        .expect("construct downloader")
        .download(&[export])
        .expect("download redirect fixture");
    server.join().expect("join test server");

    assert!(fs::read_to_string(output)
        .expect("read downloaded fixture")
        .contains("name,stat"));
}

#[test]
fn failed_download_preserves_previous_sheet_cache() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept test request");
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).expect("read test request");
        write!(
            stream,
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .expect("write error response");
    });

    let directory = tempdir().expect("create temporary directory");
    let previous = directory.path().join("previous.csv");
    fs::write(&previous, b"last-known-good").expect("write previous cache");

    let export = SheetExport {
        gid: "unused".into(),
        output_path: directory.path().join("new.csv"),
        url_override: Some(format!("http://{address}/fail")),
    };
    let result = SheetDownloader::new("unused", directory.path())
        .expect("construct downloader")
        .download(&[export]);
    server.join().expect("join test server");

    assert!(result.is_err());
    assert_eq!(
        fs::read(previous).expect("read previous cache"),
        b"last-known-good"
    );
}
