use std::collections::HashMap;

use weirdgungamecalc::{
    CalculationStats, Config, Core, DataSet, Engine, Magazine, NumericRange, Part, ResultRow,
    SortKey, SortPriority,
};

fn part(name: &str, category: &str, damage_mod: f64, fire_rate_mod: f64) -> Part {
    Part {
        name: name.into(),
        category: category.into(),
        damage_mod,
        fire_rate_mod,
    }
}

fn magazine(
    name: &str,
    category: &str,
    magazine_size: f64,
    damage_mod: f64,
    fire_rate_mod: f64,
) -> Magazine {
    Magazine {
        name: name.into(),
        category: category.into(),
        magazine_size,
        reload_time: 1.0,
        damage_mod,
        fire_rate_mod,
    }
}

fn regression_data() -> DataSet {
    DataSet {
        cores: vec![
            Core {
                name: "AR Core".into(),
                category: "AR".into(),
                damage: 42.0,
                damage_end: 31.0,
                fire_rate: 150.0,
            },
            Core {
                name: "SMG Core".into(),
                category: "SMG".into(),
                damage: 29.0,
                damage_end: 22.0,
                fire_rate: 210.0,
            },
        ],
        magazines: vec![
            magazine("AR Mag", "AR", 30.0, 5.0, 0.0),
            magazine("SMG Mag", "SMG", 24.0, 0.0, 12.0),
            magazine("Weird Mag", "Weird", 40.0, -3.0, 8.0),
            magazine("Neutral Mag", "AR", 18.0, 0.0, 0.0),
        ],
        barrels: vec![
            part("Damage Barrel", "AR", 12.0, -2.0),
            part("Fast Barrel", "SMG", -1.0, 15.0),
            part("Odd Barrel", "Weird", 4.0, 4.0),
            part("Plain Barrel", "AR", 0.0, 0.0),
        ],
        stocks: vec![
            part("Damage Stock", "AR", 6.0, 0.0),
            part("Fast Stock", "SMG", 0.0, 9.0),
            part("Odd Stock", "Weird", 3.0, 3.0),
            part("Plain Stock", "AR", 0.0, 0.0),
        ],
        grips: vec![
            part("Damage Grip", "AR", 4.0, 0.0),
            part("Fast Grip", "SMG", 0.0, 7.0),
            part("Odd Grip", "Weird", 2.0, 2.0),
            part("Plain Grip", "AR", 0.0, 0.0),
        ],
        penalties: vec![
            vec![1.0, 0.75, 1.0],
            vec![0.8, 1.0, 1.0],
            vec![1.0, 1.0, 1.0],
        ],
        categories: HashMap::from([
            ("AR".into(), 0),
            ("SMG".into(), 1),
            ("Weird".into(), 2),
        ]),
    }
}

#[test]
fn optimized_engine_matches_naive_reference_across_rankings() {
    let data = regression_data();

    for (sort_key, priority) in [
        (SortKey::Ttk, SortPriority::Auto),
        (SortKey::Dps, SortPriority::Auto),
        (SortKey::Damage, SortPriority::Highest),
        (SortKey::DamageEnd, SortPriority::Highest),
        (SortKey::FireRate, SortPriority::Lowest),
        (SortKey::Magazine, SortPriority::Highest),
    ] {
        let config = Config {
            top_n: 7,
            player_max_health: 125.0,
            sort_key,
            priority,
            part_pool_per_type: 3,
            damage_range: NumericRange::new(Some(20.0), None),
            ..Config::default()
        };

        let (optimized_results, optimized_stats) = Engine::new(&data).calculate_top(&config);
        let (reference_results, reference_stats) = naive_calculate_top(&data, &config);

        assert_eq!(
            optimized_results, reference_results,
            "optimized results diverged for {sort_key:?}/{priority:?}"
        );
        assert_eq!(
            optimized_stats, reference_stats,
            "optimized stats diverged for {sort_key:?}/{priority:?}"
        );
    }
}

#[test]
fn optimized_engine_matches_reference_with_category_filter_and_ties() {
    let mut data = regression_data();
    data.magazines.insert(0, magazine("Tie A", "AR", 30.0, 5.0, 0.0));
    data.magazines.insert(1, magazine("Tie B", "AR", 30.0, 5.0, 0.0));

    let config = Config {
        top_n: 5,
        include_categories: vec!["ar".into()],
        part_pool_per_type: 4,
        ..Config::default()
    };

    let (optimized_results, optimized_stats) = Engine::new(&data).calculate_top(&config);
    let (reference_results, reference_stats) = naive_calculate_top(&data, &config);

    assert_eq!(optimized_results, reference_results);
    assert_eq!(optimized_stats, reference_stats);
    assert_eq!(optimized_stats.cores_skipped_by_category, 1);
}

fn naive_calculate_top(data: &DataSet, config: &Config) -> (Vec<ResultRow>, CalculationStats) {
    let mut rows = Vec::<(u64, ResultRow)>::new();
    let mut stats = CalculationStats::default();
    let mut sequence = 0_u64;

    for core in &data.cores {
        stats.cores_considered += 1;

        if !config.include_categories.is_empty()
            && !config
                .include_categories
                .iter()
                .any(|category| category.eq_ignore_ascii_case(&core.category))
        {
            stats.cores_skipped_by_category += 1;
            continue;
        }

        let Some(&core_index) = data.categories.get(&core.category) else {
            continue;
        };

        let magazines = top_magazines_reference(data, core, core_index, config.part_pool_per_type);
        let barrels =
            top_parts_reference(data, &data.barrels, core, core_index, config.part_pool_per_type);
        let stocks =
            top_parts_reference(data, &data.stocks, core, core_index, config.part_pool_per_type);
        let grips =
            top_parts_reference(data, &data.grips, core, core_index, config.part_pool_per_type);

        for magazine in magazines {
            for barrel in &barrels {
                for stock in &stocks {
                    for grip in &grips {
                        stats.combinations_evaluated += 1;
                        sequence += 1;

                        let damage_multiplier = modifier_multiplier_reference(
                            data,
                            core,
                            core_index,
                            [
                                (magazine.name.as_str(), magazine.category.as_str(), magazine.damage_mod),
                                (barrel.name.as_str(), barrel.category.as_str(), barrel.damage_mod),
                                (stock.name.as_str(), stock.category.as_str(), stock.damage_mod),
                                (grip.name.as_str(), grip.category.as_str(), grip.damage_mod),
                            ],
                        );
                        let fire_rate_multiplier = modifier_multiplier_reference(
                            data,
                            core,
                            core_index,
                            [
                                (magazine.name.as_str(), magazine.category.as_str(), magazine.fire_rate_mod),
                                (barrel.name.as_str(), barrel.category.as_str(), barrel.fire_rate_mod),
                                (stock.name.as_str(), stock.category.as_str(), stock.fire_rate_mod),
                                (grip.name.as_str(), grip.category.as_str(), grip.fire_rate_mod),
                            ],
                        );

                        let damage = core.damage * damage_multiplier;
                        let fire_rate = core.fire_rate * fire_rate_multiplier;
                        if damage <= 0.0 || fire_rate <= 0.0 {
                            continue;
                        }

                        let shots = (config.player_max_health / damage).ceil();
                        let result = ResultRow {
                            core: core.name.clone(),
                            magazine: magazine.name.clone(),
                            barrel: barrel.name.clone(),
                            stock: stock.name.clone(),
                            grip: grip.name.clone(),
                            damage,
                            damage_end: core.damage_end * damage_multiplier,
                            fire_rate,
                            magazine_size: magazine.magazine_size,
                            ttk_seconds: ((shots - 1.0) / fire_rate) * 60.0,
                            dps: damage * fire_rate / 60.0,
                        };

                        if !config.damage_range.contains(result.damage)
                            || !config.damage_end_range.contains(result.damage_end)
                            || !config.ttk_seconds_range.contains(result.ttk_seconds)
                            || !config.dps_range.contains(result.dps)
                        {
                            stats.combinations_filtered += 1;
                            continue;
                        }

                        rows.push((sequence, result));
                    }
                }
            }
        }
    }

    let priority = match config.priority {
        SortPriority::Auto if config.sort_key == SortKey::Ttk => SortPriority::Lowest,
        SortPriority::Auto => SortPriority::Highest,
        explicit => explicit,
    };

    rows.sort_by(|(sequence_a, a), (sequence_b, b)| {
        let order = result_metric(a, config.sort_key).total_cmp(&result_metric(b, config.sort_key));
        let order = match priority {
            SortPriority::Highest => order.reverse(),
            SortPriority::Lowest => order,
            SortPriority::Auto => unreachable!(),
        };
        order.then_with(|| sequence_a.cmp(sequence_b))
    });
    rows.truncate(config.top_n);

    let results = rows.into_iter().map(|(_, result)| result).collect::<Vec<_>>();
    stats.results_kept = results.len();
    (results, stats)
}

fn top_parts_reference<'a>(
    data: &DataSet,
    pool: &'a [Part],
    core: &Core,
    core_index: usize,
    max_count: usize,
) -> Vec<&'a Part> {
    let mut parts = pool.iter().collect::<Vec<_>>();
    parts.sort_by(|a, b| {
        part_score_reference(data, core, core_index, b)
            .total_cmp(&part_score_reference(data, core, core_index, a))
    });
    parts.truncate(max_count);
    parts
}

fn top_magazines_reference<'a>(
    data: &'a DataSet,
    core: &Core,
    core_index: usize,
    max_count: usize,
) -> Vec<&'a Magazine> {
    let mut magazines = data.magazines.iter().collect::<Vec<_>>();
    magazines.sort_by(|a, b| {
        magazine_score_reference(data, core, core_index, b)
            .total_cmp(&magazine_score_reference(data, core, core_index, a))
    });
    magazines.truncate(max_count);
    magazines
}

fn part_score_reference(data: &DataSet, core: &Core, core_index: usize, part: &Part) -> f64 {
    let penalty = penalty_reference(data, core_index, &part.category);
    adjusted_reference(part.damage_mod, &core.name, &part.name, penalty)
        + adjusted_reference(part.fire_rate_mod, &core.name, &part.name, penalty) * 0.6
}

fn magazine_score_reference(
    data: &DataSet,
    core: &Core,
    core_index: usize,
    magazine: &Magazine,
) -> f64 {
    let penalty = penalty_reference(data, core_index, &magazine.category);
    adjusted_reference(magazine.damage_mod, &core.name, &magazine.name, penalty)
        + adjusted_reference(magazine.fire_rate_mod, &core.name, &magazine.name, penalty) * 0.6
        + magazine.magazine_size * 0.05
}

fn modifier_multiplier_reference<const N: usize>(
    data: &DataSet,
    core: &Core,
    core_index: usize,
    parts: [(&str, &str, f64); N],
) -> f64 {
    parts.into_iter().fold(1.0, |multiplier, (name, category, raw)| {
        let penalty = penalty_reference(data, core_index, category);
        multiplier * (1.0 + adjusted_reference(raw, &core.name, name, penalty) / 100.0)
    })
}

fn penalty_reference(data: &DataSet, core_index: usize, category: &str) -> f64 {
    data.categories
        .get(category)
        .and_then(|part_index| data.penalties.get(core_index)?.get(*part_index))
        .copied()
        .unwrap_or(1.0)
}

fn adjusted_reference(raw: f64, core_name: &str, part_name: &str, penalty: f64) -> f64 {
    if core_name == part_name {
        0.0
    } else {
        raw * penalty
    }
}

fn result_metric(result: &ResultRow, key: SortKey) -> f64 {
    match key {
        SortKey::Ttk => result.ttk_seconds,
        SortKey::Dps => result.dps,
        SortKey::Damage => result.damage,
        SortKey::DamageEnd => result.damage_end,
        SortKey::FireRate => result.fire_rate,
        SortKey::Magazine => result.magazine_size,
    }
}
