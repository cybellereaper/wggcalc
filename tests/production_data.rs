use std::path::PathBuf;

use weirdgungamecalc::{parser::load_data, Config, Engine};

fn production_database() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Data/FullData.sqlite3")
}

#[test]
fn checked_in_database_loads_and_produces_viable_builds() {
    let path = production_database();
    assert!(
        path.exists(),
        "production database is missing: {}",
        path.display()
    );

    let data = load_data(path.to_str().expect("UTF-8 repository path"))
        .expect("checked-in production database should load");

    assert!(!data.cores.is_empty(), "production database has no cores");
    assert!(
        !data.magazines.is_empty(),
        "production database has no magazines"
    );
    assert!(!data.barrels.is_empty(), "production database has no barrels");
    assert!(!data.stocks.is_empty(), "production database has no stocks");
    assert!(!data.grips.is_empty(), "production database has no grips");
    assert!(!data.categories.is_empty(), "category map is empty");
    assert!(!data.penalties.is_empty(), "penalty matrix is empty");

    let config = Config {
        top_n: 3,
        part_pool_per_type: 3,
        ..Config::default()
    };
    let (results, stats) = Engine::new(&data).calculate_top(&config);

    assert!(!results.is_empty(), "production data produced no viable builds");
    assert!(results.len() <= 3);
    assert_eq!(stats.results_kept, results.len());
    assert!(stats.cores_considered > 0);
    assert!(stats.combinations_evaluated > 0);
}

#[test]
fn production_search_is_deterministic_across_repeated_runs() {
    let path = production_database();
    let data = load_data(path.to_str().expect("UTF-8 repository path"))
        .expect("checked-in production database should load");
    let config = Config {
        top_n: 10,
        part_pool_per_type: 5,
        ..Config::default()
    };

    let first = Engine::new(&data).calculate_top(&config);
    let second = Engine::new(&data).calculate_top(&config);

    assert_eq!(first, second, "parallel search must remain deterministic");
}
