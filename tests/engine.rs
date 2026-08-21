use std::collections::HashMap;

use weirdgungamecalc::{
    Config, Core, DataSet, Engine, Magazine, NumericRange, Part, SortKey,
};

fn neutral_magazine(name: &str, category: &str) -> Magazine {
    Magazine {
        name: name.into(),
        category: category.into(),
        magazine_size: 20.0,
        reload_time: 1.0,
        damage_mod: 0.0,
        fire_rate_mod: 0.0,
    }
}

fn neutral_part(name: &str, category: &str) -> Part {
    Part {
        name: name.into(),
        category: category.into(),
        damage_mod: 0.0,
        fire_rate_mod: 0.0,
    }
}

fn fixture_data() -> DataSet {
    DataSet {
        cores: vec![Core {
            name: "Core-1".into(),
            category: "AR".into(),
            damage: 50.0,
            damage_end: 40.0,
            fire_rate: 120.0,
        }],
        magazines: vec![neutral_magazine("Mag-1", "AR")],
        barrels: vec![neutral_part("Barrel-1", "AR")],
        grips: vec![neutral_part("Grip-1", "AR")],
        stocks: vec![neutral_part("Stock-1", "AR")],
        penalties: vec![vec![1.0]],
        categories: HashMap::from([("AR".into(), 0)]),
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
    let config = Config {
        damage_range: NumericRange::new(Some(9999.0), None),
        ..Config::default()
    };

    let (results, stats) = Engine::new(&data).calculate_top(&config);

    assert_eq!(stats.combinations_evaluated, 1);
    assert_eq!(stats.combinations_filtered, 1);
    assert_eq!(stats.results_kept, 0);
    assert!(results.is_empty());
}

#[test]
fn applies_cross_category_penalties_to_modifiers() {
    let mut data = fixture_data();
    data.categories.insert("SMG".into(), 1);
    data.penalties = vec![vec![1.0, 0.5], vec![0.5, 1.0]];
    data.magazines = vec![Magazine {
        name: "SMG Mag".into(),
        category: "SMG".into(),
        magazine_size: 20.0,
        reload_time: 1.0,
        damage_mod: 100.0,
        fire_rate_mod: 0.0,
    }];
    data.cores[0].damage = 100.0;
    data.cores[0].damage_end = 80.0;

    let (results, _) = Engine::new(&data).calculate_top(&Config::default());

    assert_eq!(results.len(), 1);
    assert!((results[0].damage - 150.0).abs() < f64::EPSILON);
    assert!((results[0].damage_end - 120.0).abs() < f64::EPSILON);
}

#[test]
fn ignores_modifiers_when_part_name_matches_core_name() {
    let mut data = fixture_data();
    data.cores[0].damage = 100.0;
    data.barrels = vec![Part {
        name: data.cores[0].name.clone(),
        category: "AR".into(),
        damage_mod: 100.0,
        fire_rate_mod: 100.0,
    }];

    let (results, _) = Engine::new(&data).calculate_top(&Config::default());

    assert_eq!(results.len(), 1);
    assert!((results[0].damage - 100.0).abs() < f64::EPSILON);
    assert!((results[0].fire_rate - 120.0).abs() < f64::EPSILON);
}

#[test]
fn auto_priority_uses_highest_for_dps() {
    let mut data = fixture_data();
    data.cores = vec![
        Core {
            name: "Low DPS".into(),
            category: "AR".into(),
            damage: 10.0,
            damage_end: 10.0,
            fire_rate: 60.0,
        },
        Core {
            name: "High DPS".into(),
            category: "AR".into(),
            damage: 5.0,
            damage_end: 5.0,
            fire_rate: 240.0,
        },
    ];

    let config = Config {
        top_n: 1,
        sort_key: SortKey::Dps,
        ..Config::default()
    };

    let (results, _) = Engine::new(&data).calculate_top(&config);

    assert_eq!(results[0].core, "High DPS");
}

#[test]
fn auto_priority_uses_lowest_for_ttk() {
    let mut data = fixture_data();
    data.cores = vec![
        Core {
            name: "Slow".into(),
            category: "AR".into(),
            damage: 50.0,
            damage_end: 50.0,
            fire_rate: 60.0,
        },
        Core {
            name: "Fast".into(),
            category: "AR".into(),
            damage: 34.0,
            damage_end: 34.0,
            fire_rate: 180.0,
        },
    ];

    let config = Config {
        top_n: 1,
        ..Config::default()
    };

    let (results, _) = Engine::new(&data).calculate_top(&config);

    assert_eq!(results[0].core, "Fast");
}

#[test]
fn parallel_core_search_preserves_source_order_for_ties() {
    let mut data = fixture_data();
    data.cores = vec![
        Core {
            name: "First".into(),
            category: "AR".into(),
            damage: 50.0,
            damage_end: 40.0,
            fire_rate: 120.0,
        },
        Core {
            name: "Second".into(),
            category: "AR".into(),
            damage: 50.0,
            damage_end: 40.0,
            fire_rate: 120.0,
        },
    ];

    let config = Config {
        top_n: 1,
        ..Config::default()
    };

    let (results, stats) = Engine::new(&data).calculate_top(&config);

    assert_eq!(stats.cores_considered, 2);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].core, "First");
}

#[test]
fn category_filter_is_case_insensitive() {
    let data = fixture_data();
    let config = Config {
        include_categories: vec!["ar".into()],
        ..Config::default()
    };

    let (results, stats) = Engine::new(&data).calculate_top(&config);

    assert_eq!(stats.cores_skipped_by_category, 0);
    assert_eq!(results.len(), 1);
}

#[test]
fn zero_part_pool_evaluates_no_combinations() {
    let data = fixture_data();
    let config = Config {
        part_pool_per_type: 0,
        ..Config::default()
    };

    let (results, stats) = Engine::new(&data).calculate_top(&config);

    assert_eq!(stats.combinations_evaluated, 0);
    assert!(results.is_empty());
}
