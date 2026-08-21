use std::collections::HashMap;
use weirdgungamecalc::{Config, Core, DataSet, Engine, Magazine, NumericRange, Part};

fn fixture_data() -> DataSet {
    DataSet {
        cores: vec![Core { name: "Core-1".into(), category: "AR".into(), damage: 50.0, damage_end: 40.0, fire_rate: 120.0 }],
        magazines: vec![Magazine { name: "Mag-1".into(), category: "AR".into(), magazine_size: 20.0, reload_time: 1.0, damage_mod: 0.0, fire_rate_mod: 0.0 }],
        barrels: vec![Part { name: "Barrel-1".into(), category: "AR".into(), damage_mod: 0.0, fire_rate_mod: 0.0 }],
        grips: vec![Part { name: "Grip-1".into(), category: "AR".into(), damage_mod: 0.0, fire_rate_mod: 0.0 }],
        stocks: vec![Part { name: "Stock-1".into(), category: "AR".into(), damage_mod: 0.0, fire_rate_mod: 0.0 }],
        penalties: vec![vec![1.0]], categories: HashMap::from([("AR".into(), 0)]),
    }
}

#[test]
fn open_ended_ranges() {
    assert!(NumericRange::new(Some(2.0), Some(4.0)).contains(3.0));
    assert!(!NumericRange::new(Some(2.0), Some(4.0)).contains(5.0));
    assert!(NumericRange::new(Some(2.0), None).contains(500.0));
    assert!(NumericRange::new(None, Some(10.0)).contains(-1.0));
}

#[test]
fn tracks_evaluated_combinations() {
    let data = fixture_data();
    let (results, stats) = Engine::new(&data).calculate_top(&Config::default());
    assert_eq!(stats.cores_considered, 1);
    assert_eq!(stats.combinations_evaluated, 1);
    assert_eq!(stats.combinations_filtered, 0);
    assert_eq!(stats.results_kept, 1);
    assert_eq!(results.len(), 1);
}

#[test]
fn tracks_filtered_combinations() {
    let data = fixture_data();
    let mut config = Config::default();
    config.damage_range = NumericRange::new(Some(9999.0), None);
    let (results, stats) = Engine::new(&data).calculate_top(&config);
    assert_eq!(stats.combinations_evaluated, 1);
    assert_eq!(stats.combinations_filtered, 1);
    assert_eq!(stats.results_kept, 0);
    assert!(results.is_empty());
}
