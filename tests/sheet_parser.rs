use weirdgungamecalc::sheet_parser::{detect_price_type, normalize_numeric_value, CoresParser, PartsParser};

#[test]
fn normalizes_values() {
    assert_eq!(normalize_numeric_value("10", false).unwrap().as_f64(), Some(10.0));
    assert_eq!(normalize_numeric_value("2x3", false).unwrap().as_f64(), Some(6.0));
    assert_eq!(normalize_numeric_value("  12.5% ", false).unwrap().as_f64(), Some(12.5));
    assert!(normalize_numeric_value("🎲", false).unwrap().is_null());
    assert_eq!(normalize_numeric_value("5 - 10", true).unwrap(), serde_json::json!([5.0, 10.0]));
}

#[test]
fn detects_price_aliases() {
    assert_eq!(detect_price_type(""), "Coin");
    assert_eq!(detect_price_type("Weird Boxes"), "WC");
    assert_eq!(detect_price_type("Exclusive Weird Boxes"), "Robux");
    assert_eq!(detect_price_type("12345"), "Coin");
    assert_eq!(detect_price_type("mystery"), "Unknown");
}

#[test]
fn parses_parts_rows() {
    let mut header = vec![String::new(); 17];
    header[0] = "Coin".into();
    header[1] = "AR Barrels".into();
    let mut part = vec![String::new(); 17];
    part[0] = "WC".into();
    part[1] = "Test Barrel".into();
    part[2] = "10 stat".into();
    let output = PartsParser::new().parse_rows(&[header, part]).unwrap();
    assert_eq!(output["Barrels"].len(), 1);
    assert_eq!(output["Barrels"][0]["Name"], "Test Barrel");
    assert_eq!(output["Barrels"][0]["Magazine_Size"].as_f64(), Some(10.0));
}

#[test]
fn rejects_duplicate_parts() {
    let mut rows = Vec::new();
    let mut header = vec![String::new(); 17];
    header[0] = "Coin".into();
    header[1] = "AR Barrels".into();
    rows.push(header);
    for _ in 0..2 {
        let mut part = vec![String::new(); 17];
        part[0] = "Coin".into();
        part[1] = "Dup".into();
        part[2] = "10 stat".into();
        rows.push(part);
    }
    assert!(PartsParser::new().parse_rows(&rows).is_err());
}

#[test]
fn cores_accept_extra_columns() {
    let row = vec!["Coin","Test Core","10 - 8","100 - 80","","","","","","","","","","","1 - 1","1 - 1","1 - 1","1 - 1","ignored extra"]
        .into_iter().map(str::to_string).collect::<Vec<_>>();
    let output = CoresParser.parse_rows(&[row]).unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["Name"], "Test Core");
    assert_eq!(output[0]["Category"], "AR");
    assert_eq!(output[0]["Damage"], serde_json::json!([10.0, 8.0]));
}
