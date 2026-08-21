use std::{collections::HashMap, fs};

use rusqlite::Connection;
use serde_json::{json, Map, Value};
use tempfile::tempdir;
use weirdgungamecalc::{
    parser::load_data,
    sheet_parser::{save_sqlite, DataSections, ExportData},
};

fn object(value: Value) -> Map<String, Value> {
    value.as_object().expect("fixture object").clone()
}

fn export_fixture() -> ExportData {
    let mut data = DataSections::new();
    data.insert(
        "Cores".into(),
        vec![object(json!({
            "Name": "Core-A",
            "Category": "AR",
            "Damage": [30.0, 20.0],
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
            "Damage": 5.0,
            "Fire_Rate": 10.0
        }))],
    );

    for section in ["Barrels", "Grips", "Stocks"] {
        data.insert(
            section.into(),
            vec![object(json!({
                "Name": format!("{section}-A"),
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
fn sqlite_round_trip_preserves_calculator_fields() {
    let directory = tempdir().expect("create temporary directory");
    let database = directory.path().join("full_data.sqlite3");
    save_sqlite(&export_fixture(), &database).expect("save SQLite fixture");

    let loaded = load_data(database.to_str().expect("UTF-8 temporary path"))
        .expect("load SQLite fixture");
    assert_eq!(loaded.cores.len(), 1);
    assert_eq!(loaded.magazines.len(), 1);
    assert_eq!(loaded.barrels.len(), 1);
    assert_eq!(loaded.grips.len(), 1);
    assert_eq!(loaded.stocks.len(), 1);
    assert_eq!(loaded.categories["AR"], 0);
    assert_eq!(loaded.penalties[0][0], 1.0);
    assert_eq!(loaded.cores[0].damage, 30.0);
    assert_eq!(loaded.cores[0].damage_end, 20.0);
    assert_eq!(loaded.magazines[0].damage_mod, 5.0);
    assert_eq!(loaded.magazines[0].fire_rate_mod, 10.0);
}

#[test]
fn sqlite_extension_detection_is_case_insensitive() {
    let directory = tempdir().expect("create temporary directory");
    let database = directory.path().join("FULLDATA.SQLITE3");
    save_sqlite(&export_fixture(), &database).expect("save SQLite fixture");

    let loaded = load_data(database.to_str().expect("UTF-8 temporary path"))
        .expect("load upper-case SQLite extension");
    assert_eq!(loaded.cores.len(), 1);
}

#[test]
fn open_ended_damage_range_keeps_original_sqlite_semantics() {
    let directory = tempdir().expect("create temporary directory");
    let database = directory.path().join("full_data.sqlite3");
    let mut export = export_fixture();
    export.data.get_mut("Cores").expect("Cores fixture")[0].insert(
        "Damage".into(),
        Value::Array(vec![json!(30.0), Value::Null]),
    );

    save_sqlite(&export, &database).expect("save SQLite fixture");
    let loaded = load_data(database.to_str().expect("UTF-8 temporary path"))
        .expect("load SQLite fixture");

    assert_eq!(loaded.cores[0].damage, 30.0);
    assert_eq!(loaded.cores[0].damage_end, 0.0);
}

#[test]
fn sparse_penalty_cells_default_to_neutral_multiplier() {
    let directory = tempdir().expect("create temporary directory");
    let database = directory.path().join("full_data.sqlite3");
    let mut export = export_fixture();
    export
        .categories
        .get_mut("Primary")
        .expect("Primary categories")
        .insert("SMG".into(), 1);
    export.penalties = vec![vec![1.0, 0.75], vec![0.8, 1.0]];
    save_sqlite(&export, &database).expect("save SQLite fixture");

    let connection = Connection::open(&database).expect("open SQLite fixture");
    connection
        .execute(
            "DELETE FROM penalties WHERE core_idx = 0 AND part_idx = 1",
            [],
        )
        .expect("delete penalty fixture cell");
    drop(connection);

    let loaded = load_data(database.to_str().expect("UTF-8 temporary path"))
        .expect("load sparse penalty matrix");
    assert_eq!(loaded.penalties[0][0], 1.0);
    assert_eq!(loaded.penalties[0][1], 1.0);
    assert_eq!(loaded.penalties[1][0], 0.8);
}

#[test]
fn negative_sqlite_indices_are_rejected_instead_of_wrapping() {
    let directory = tempdir().expect("create temporary directory");
    let database = directory.path().join("bad.sqlite3");
    let connection = Connection::open(&database).expect("open SQLite fixture");
    connection
        .execute_batch(
            "CREATE TABLE categories (name TEXT PRIMARY KEY, idx INTEGER NOT NULL);\n\
             INSERT INTO categories (name, idx) VALUES ('AR', -1);",
        )
        .expect("create malformed category fixture");
    drop(connection);

    let error = load_data(database.to_str().expect("UTF-8 temporary path"))
        .expect_err("negative index must be rejected")
        .to_string();
    assert!(error.contains("negative index"), "unexpected error: {error}");
}

#[test]
fn out_of_bounds_penalty_indices_are_rejected() {
    let directory = tempdir().expect("create temporary directory");
    let database = directory.path().join("bad.sqlite3");
    let connection = Connection::open(&database).expect("open SQLite fixture");
    connection
        .execute_batch(
            "CREATE TABLE categories (name TEXT PRIMARY KEY, idx INTEGER NOT NULL);\n\
             CREATE TABLE penalties (core_idx INTEGER NOT NULL, part_idx INTEGER NOT NULL, value REAL NOT NULL);\n\
             INSERT INTO categories (name, idx) VALUES ('AR', 0);\n\
             INSERT INTO penalties (core_idx, part_idx, value) VALUES (0, 2, 1.0);",
        )
        .expect("create malformed penalty fixture");
    drop(connection);

    let error = load_data(database.to_str().expect("UTF-8 temporary path"))
        .expect_err("out-of-bounds penalty must be rejected")
        .to_string();
    assert!(
        error.contains("outside category matrix"),
        "unexpected error: {error}"
    );
}

#[test]
fn failed_generation_does_not_destroy_existing_database() {
    let directory = tempdir().expect("create temporary directory");
    let database = directory.path().join("full_data.sqlite3");
    fs::write(&database, b"known-good").expect("write previous database marker");

    let mut invalid_export = export_fixture();
    invalid_export.data.remove("Cores");

    assert!(save_sqlite(&invalid_export, &database).is_err());
    assert_eq!(
        fs::read(&database).expect("read previous database marker"),
        b"known-good"
    );
}

#[test]
fn legacy_json_supports_scalar_damage_and_missing_optional_numbers() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("legacy.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "Categories": {
                "Primary": {"AR": 0},
                "Secondary": {}
            },
            "Penalties": [[1.0]],
            "Data": {
                "Cores": [{
                    "Name": "Core",
                    "Category": "AR",
                    "Damage": 25,
                    "Fire_Rate": 100
                }],
                "Magazines": [{
                    "Name": "Mag",
                    "Category": "AR",
                    "Magazine_Size": 20
                }],
                "Barrels": [{"Name": "Barrel", "Category": "AR"}],
                "Grips": [{"Name": "Grip", "Category": "AR"}],
                "Stocks": [{"Name": "Stock", "Category": "AR"}]
            }
        }))
        .expect("serialize JSON fixture"),
    )
    .expect("write JSON fixture");

    let loaded = load_data(path.to_str().expect("UTF-8 temporary path"))
        .expect("load legacy JSON fixture");
    assert_eq!(loaded.cores[0].damage, 25.0);
    assert_eq!(loaded.cores[0].damage_end, 25.0);
    assert_eq!(loaded.magazines[0].damage_mod, 0.0);
    assert_eq!(loaded.magazines[0].fire_rate_mod, 0.0);
}

#[test]
fn invalid_json_shape_numeric_types_and_category_indices_return_errors() {
    let directory = tempdir().expect("create temporary directory");

    let missing_data = directory.path().join("missing.json");
    fs::write(&missing_data, b"{}").expect("write invalid JSON fixture");
    assert!(load_data(missing_data.to_str().expect("UTF-8 temporary path")).is_err());

    let bad_number = directory.path().join("bad-number.json");
    fs::write(
        &bad_number,
        serde_json::to_vec(&json!({
            "Categories": {"Primary": {"AR": 0}, "Secondary": {}},
            "Penalties": [[1.0]],
            "Data": {
                "Cores": [{"Name": "Core", "Category": "AR", "Damage": "high", "Fire_Rate": 100}],
                "Magazines": [], "Barrels": [], "Grips": [], "Stocks": []
            }
        }))
        .expect("serialize invalid numeric fixture"),
    )
    .expect("write invalid numeric fixture");
    assert!(load_data(bad_number.to_str().expect("UTF-8 temporary path")).is_err());

    let fractional_index = directory.path().join("fractional-index.json");
    fs::write(
        &fractional_index,
        serde_json::to_vec(&json!({
            "Categories": {"Primary": {"AR": 0.5}, "Secondary": {}},
            "Penalties": [[1.0]],
            "Data": {"Cores": [], "Magazines": [], "Barrels": [], "Grips": [], "Stocks": []}
        }))
        .expect("serialize invalid index fixture"),
    )
    .expect("write invalid index fixture");
    assert!(load_data(fractional_index.to_str().expect("UTF-8 temporary path")).is_err());
}
