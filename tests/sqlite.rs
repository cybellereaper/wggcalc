use std::{collections::HashMap, fs};

use serde_json::{json, Map, Value};
use tempfile::tempdir;
use weirdgungamecalc::{
    parser::load_data,
    sheet_parser::{save_sqlite, DataSections, ExportData},
};

fn obj(value: Value) -> Map<String, Value> {
    value.as_object().expect("test object").clone()
}

fn fixture_export() -> ExportData {
    let mut data = DataSections::new();
    data.insert(
        "Cores".into(),
        vec![obj(json!({
            "Name": "Core-A",
            "Category": "AR",
            "Damage": [30.0, 20.0],
            "Fire_Rate": 120.0
        }))],
    );
    data.insert(
        "Magazines".into(),
        vec![obj(json!({
            "Name": "Mag-A",
            "Category": "AR",
            "Magazine_Size": 20.0,
            "Reload_Time": 1.0,
            "Damage": 5.0,
            "Fire_Rate": 10.0
        }))],
    );

    for key in ["Barrels", "Grips", "Stocks"] {
        data.insert(
            key.into(),
            vec![obj(json!({
                "Name": "Part-A",
                "Category": "AR",
                "Damage": 0.0,
                "Fire_Rate": 0.0
            }))],
        );
    }

    ExportData {
        data,
        penalties: vec![vec![1.0]],
        categories: HashMap::from([
            ("Primary".into(), HashMap::from([("AR".into(), 0)])),
            ("Secondary".into(), HashMap::new()),
        ]),
    }
}

#[test]
fn sqlite_round_trip() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("full_data.sqlite3");
    let export = fixture_export();

    save_sqlite(&export, &db_path).unwrap();
    let loaded = load_data(db_path.to_str().unwrap()).unwrap();

    assert_eq!(loaded.cores.len(), 1);
    assert_eq!(loaded.magazines.len(), 1);
    assert_eq!(loaded.barrels.len(), 1);
    assert_eq!(loaded.categories["AR"], 0);
    assert_eq!(loaded.penalties[0][0], 1.0);
    assert_eq!(loaded.cores[0].damage_end, 20.0);
    assert_eq!(loaded.magazines[0].damage_mod, 5.0);
}

#[test]
fn open_ended_damage_range_keeps_original_sqlite_semantics() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("full_data.sqlite3");
    let mut export = fixture_export();
    export.data.get_mut("Cores").unwrap()[0].insert(
        "Damage".into(),
        Value::Array(vec![json!(30.0), Value::Null]),
    );

    save_sqlite(&export, &db_path).unwrap();
    let loaded = load_data(db_path.to_str().unwrap()).unwrap();

    assert_eq!(loaded.cores[0].damage, 30.0);
    assert_eq!(loaded.cores[0].damage_end, 0.0);
}

#[test]
fn failed_generation_does_not_destroy_existing_database() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("full_data.sqlite3");
    fs::write(&db_path, b"known-good").unwrap();

    let mut invalid_export = fixture_export();
    invalid_export.data.remove("Cores");

    assert!(save_sqlite(&invalid_export, &db_path).is_err());
    assert_eq!(fs::read(&db_path).unwrap(), b"known-good");
}
