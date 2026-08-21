use std::{collections::HashMap, process::Command};

use serde_json::{json, Map, Value};
use tempfile::tempdir;
use weirdgungamecalc::sheet_parser::{save_sqlite, DataSections, ExportData};

fn object(value: Value) -> Map<String, Value> {
    value.as_object().expect("fixture object").clone()
}

fn create_database() -> tempfile::TempDir {
    let directory = tempdir().expect("create temporary directory");
    let mut data = DataSections::new();

    data.insert(
        "Cores".into(),
        vec![object(json!({
            "Name": "Core-A",
            "Category": "AR",
            "Damage": [50.0, 40.0],
            "Fire_Rate": 120.0
        }))],
    );
    data.insert(
        "Magazines".into(),
        vec![object(json!({
            "Name": "Mag-A",
            "Category": "AR",
            "Magazine_Size": 20.0,
            "Reload_Time": 1.0,
            "Damage": 0.0,
            "Fire_Rate": 0.0
        }))],
    );

    for section in ["Barrels", "Grips", "Stocks"] {
        data.insert(
            section.into(),
            vec![object(json!({
                "Name": section,
                "Category": "AR",
                "Damage": 0.0,
                "Fire_Rate": 0.0
            }))],
        );
    }

    let export = ExportData {
        data,
        penalties: vec![vec![1.0]],
        categories: HashMap::from([
            ("Primary".into(), HashMap::from([("AR".into(), 0)])),
            ("Secondary".into(), HashMap::new()),
        ]),
    };

    save_sqlite(&export, &directory.path().join("FullData.sqlite3"))
        .expect("write CLI fixture database");
    directory
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_wggcalc")
}

#[test]
fn help_lists_compatibility_flags() {
    let output = Command::new(binary())
        .arg("--help")
        .output()
        .expect("run --help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 help output");

    for flag in [
        "--data",
        "--top",
        "--mh",
        "--sort",
        "--priority",
        "--include",
        "--part-pool",
        "--damage-min",
        "--damage-max",
        "--ttk-min",
        "--ttk-max",
        "--dps-min",
        "--dps-max",
        "--metrics",
    ] {
        assert!(stdout.contains(flag), "missing flag {flag} in help output");
    }
}

#[test]
fn end_to_end_cli_reads_sqlite_calculates_and_prints_metrics() {
    let directory = create_database();
    let database = directory.path().join("FullData.sqlite3");
    let output = Command::new(binary())
        .args([
            "--data",
            database.to_str().expect("UTF-8 temporary path"),
            "--top",
            "1",
            "--sort",
            "ttk",
            "--include",
            "AR",
            "--metrics",
        ])
        .output()
        .expect("run calculator");

    assert!(
        output.status.success(),
        "calculator failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("UTF-8 calculator output");
    assert!(stdout.contains("Loaded 1 cores, 1 magazines, 1 barrels, 1 stocks, 1 grips"));
    assert!(stdout.contains("#1"));
    assert!(stdout.contains(" Core: Core-A"));
    assert!(stdout.contains(" Magazine: Mag-A"));
    assert!(stdout.contains(" Damage: 50.000"));
    assert!(stdout.contains(" Damage End: 40.000"));
    assert!(stdout.contains(" Fire Rate: 120.000"));
    assert!(stdout.contains(" TTK: 0.500s"));
    assert!(stdout.contains(" DPS: 100.000"));
    assert!(stdout.contains("Cores considered: 1"));
    assert!(stdout.contains("Combinations evaluated: 1"));
    assert!(stdout.contains("Combinations filtered: 0"));
    assert!(stdout.contains("Results kept: 1"));
}

#[test]
fn invalid_sort_and_priority_values_fail_cleanly() {
    for arguments in [["--sort", "speed"], ["--priority", "middle"]] {
        let output = Command::new(binary())
            .args(arguments)
            .output()
            .expect("run invalid CLI invocation");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        assert!(
            stderr.contains("invalid"),
            "expected useful error, got: {stderr}"
        );
    }
}

#[test]
fn missing_data_file_fails_without_partial_results() {
    let directory = tempdir().expect("create temporary directory");
    let missing = directory.path().join("missing.sqlite3");
    let output = Command::new(binary())
        .args(["--data", missing.to_str().expect("UTF-8 temporary path")])
        .output()
        .expect("run missing-data invocation");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("opening SQLite database"));
}
