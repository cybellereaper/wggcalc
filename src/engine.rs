use std::{collections::HashMap, fmt::Write as _};

use rayon::prelude::*;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NumericRange {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl NumericRange {
    pub const fn new(min: Option<f64>, max: Option<f64>) -> Self {
        Self { min, max }
    }

    #[inline]
    pub fn contains(self, value: f64) -> bool {
        !self.min.is_some_and(|min| value < min)
            && !self.max.is_some_and(|max| value > max)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Ttk,
    Dps,
    Damage,
    DamageEnd,
    FireRate,
    Magazine,
}

impl SortKey {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "ttk" => Ok(Self::Ttk),
            "dps" => Ok(Self::Dps),
            "damage" => Ok(Self::Damage),
            "damageend" => Ok(Self::DamageEnd),
            "firerate" => Ok(Self::FireRate),
            "magazine" => Ok(Self::Magazine),
            _ => anyhow::bail!("Invalid sort key: {value}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortPriority {
    Highest,
    Lowest,
    Auto,
}

impl SortPriority {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "highest" => Ok(Self::Highest),
            "lowest" => Ok(Self::Lowest),
            "auto" => Ok(Self::Auto),
            _ => anyhow::bail!("Invalid priority: {value}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub data_path: String,
    pub top_n: usize,
    pub player_max_health: f64,
    pub sort_key: SortKey,
    pub priority: SortPriority,
    pub include_categories: Vec<String>,
    pub damage_range: NumericRange,
    pub damage_end_range: NumericRange,
    pub ttk_seconds_range: NumericRange,
    pub dps_range: NumericRange,
    pub part_pool_per_type: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_path: "Data/FullData.sqlite3".into(),
            top_n: 10,
            player_max_health: 100.0,
            sort_key: SortKey::Ttk,
            priority: SortPriority::Auto,
            include_categories: Vec::new(),
            damage_range: NumericRange::default(),
            damage_end_range: NumericRange::default(),
            ttk_seconds_range: NumericRange::default(),
            dps_range: NumericRange::default(),
            part_pool_per_type: 20,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalculationStats {
    pub cores_considered: usize,
    pub cores_skipped_by_category: usize,
    pub combinations_evaluated: u64,
    pub combinations_filtered: u64,
    pub results_kept: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Core {
    pub name: String,
    pub category: String,
    pub damage: f64,
    pub damage_end: f64,
    pub fire_rate: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Magazine {
    pub name: String,
    pub category: String,
    pub magazine_size: f64,
    pub reload_time: f64,
    pub damage_mod: f64,
    pub fire_rate_mod: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub name: String,
    pub category: String,
    pub damage_mod: f64,
    pub fire_rate_mod: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultRow {
    pub core: String,
    pub magazine: String,
    pub barrel: String,
    pub stock: String,
    pub grip: String,
    pub damage: f64,
    pub damage_end: f64,
    pub fire_rate: f64,
    pub magazine_size: f64,
    pub ttk_seconds: f64,
    pub dps: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataSet {
    pub cores: Vec<Core>,
    pub magazines: Vec<Magazine>,
    pub barrels: Vec<Part>,
    pub grips: Vec<Part>,
    pub stocks: Vec<Part>,
    pub penalties: Vec<Vec<f64>>,
    pub categories: HashMap<String, usize>,
}

pub struct Engine<'a> {
    data: &'a DataSet,
}

impl<'a> Engine<'a> {
    pub const fn new(data: &'a DataSet) -> Self {
        Self { data }
    }

    pub fn calculate_top(&self, config: &Config) -> (Vec<ResultRow>, CalculationStats) {
        let ranking = Ranking::from_config(config);

        // Cores are independent search spaces, so evaluate them concurrently.
        // Indexed parallel collection preserves core order, which keeps tie
        // behavior deterministic when the per-core top lists are merged.
        let outcomes: Vec<_> = self
            .data
            .cores
            .par_iter()
            .map(|core| self.calculate_core(core, config, ranking))
            .collect();

        let mut results = Vec::with_capacity(config.top_n.min(1024));
        let mut stats = CalculationStats::default();

        for outcome in outcomes {
            stats.cores_considered += outcome.stats.cores_considered;
            stats.cores_skipped_by_category += outcome.stats.cores_skipped_by_category;
            stats.combinations_evaluated += outcome.stats.combinations_evaluated;
            stats.combinations_filtered += outcome.stats.combinations_filtered;

            for result in outcome.results {
                insert_top_result(&mut results, result, config.top_n, ranking);
            }
        }

        stats.results_kept = results.len();
        (results, stats)
    }

    fn calculate_core(
        &self,
        core: &Core,
        config: &Config,
        ranking: Ranking,
    ) -> CoreOutcome {
        let mut stats = CalculationStats {
            cores_considered: 1,
            ..CalculationStats::default()
        };

        if !include_category(&config.include_categories, &core.category) {
            stats.cores_skipped_by_category = 1;
            return CoreOutcome {
                results: Vec::new(),
                stats,
            };
        }

        let Some(&core_idx) = self.data.categories.get(&core.category) else {
            return CoreOutcome {
                results: Vec::new(),
                stats,
            };
        };

        let magazines = self.top_magazines(core, core_idx, config.part_pool_per_type);
        let barrels = self.top_parts(
            &self.data.barrels,
            core,
            core_idx,
            config.part_pool_per_type,
        );
        let stocks = self.top_parts(
            &self.data.stocks,
            core,
            core_idx,
            config.part_pool_per_type,
        );
        let grips =
            self.top_parts(&self.data.grips, core, core_idx, config.part_pool_per_type);

        let mut results = Vec::with_capacity(config.top_n.min(256));

        for mag in magazines {
            for barrel in &barrels {
                let mag_barrel_damage = mag.damage_factor * barrel.damage_factor;
                let mag_barrel_fire_rate = mag.fire_rate_factor * barrel.fire_rate_factor;

                for stock in &stocks {
                    let base_damage_factor = mag_barrel_damage * stock.damage_factor;
                    let base_fire_rate_factor =
                        mag_barrel_fire_rate * stock.fire_rate_factor;

                    for grip in &grips {
                        stats.combinations_evaluated += 1;

                        let damage_factor = base_damage_factor * grip.damage_factor;
                        let fire_rate_factor = base_fire_rate_factor * grip.fire_rate_factor;

                        let Some(metrics) = evaluate_metrics(
                            config,
                            core,
                            damage_factor,
                            fire_rate_factor,
                            mag.magazine.magazine_size,
                        )
                        else {
                            continue;
                        };

                        if !passes_filters(config, metrics) {
                            stats.combinations_filtered += 1;
                            continue;
                        }

                        if !can_enter_top(&results, metrics, config.top_n, ranking) {
                            continue;
                        }

                        let result = ResultRow {
                            core: core.name.clone(),
                            magazine: mag.magazine.name.clone(),
                            barrel: barrel.part.name.clone(),
                            stock: stock.part.name.clone(),
                            grip: grip.part.name.clone(),
                            damage: metrics.damage,
                            damage_end: metrics.damage_end,
                            fire_rate: metrics.fire_rate,
                            magazine_size: metrics.magazine_size,
                            ttk_seconds: metrics.ttk_seconds,
                            dps: metrics.dps,
                        };

                        insert_top_result(&mut results, result, config.top_n, ranking);
                    }
                }
            }
        }

        CoreOutcome { results, stats }
    }

    fn top_parts<'b>(
        &self,
        pool: &'b [Part],
        core: &Core,
        core_idx: usize,
        max_count: usize,
    ) -> Vec<EvaluatedPart<'b>> {
        let mut parts = pool
            .iter()
            .map(|part| {
                let modifiers = self.evaluate_modifiers(
                    core,
                    core_idx,
                    &part.name,
                    &part.category,
                    part.damage_mod,
                    part.fire_rate_mod,
                );
                EvaluatedPart {
                    part,
                    damage_factor: modifiers.damage_factor,
                    fire_rate_factor: modifiers.fire_rate_factor,
                    score: modifiers.score,
                }
            })
            .collect::<Vec<_>>();

        // sort_by is stable, so equal-scoring parts retain source order.
        parts.sort_by(|a, b| b.score.total_cmp(&a.score));
        parts.truncate(max_count);
        parts
    }

    fn top_magazines<'b>(
        &self,
        core: &Core,
        core_idx: usize,
        max_count: usize,
    ) -> Vec<EvaluatedMagazine<'b>> {
        let mut magazines = self
            .data
            .magazines
            .iter()
            .map(|magazine| {
                let modifiers = self.evaluate_modifiers(
                    core,
                    core_idx,
                    &magazine.name,
                    &magazine.category,
                    magazine.damage_mod,
                    magazine.fire_rate_mod,
                );
                EvaluatedMagazine {
                    magazine,
                    damage_factor: modifiers.damage_factor,
                    fire_rate_factor: modifiers.fire_rate_factor,
                    score: modifiers.score + magazine.magazine_size * 0.05,
                }
            })
            .collect::<Vec<_>>();

        magazines.sort_by(|a, b| b.score.total_cmp(&a.score));
        magazines.truncate(max_count);
        magazines
    }

    #[inline]
    fn evaluate_modifiers(
        &self,
        core: &Core,
        core_idx: usize,
        part_name: &str,
        category: &str,
        damage_mod: f64,
        fire_rate_mod: f64,
    ) -> EvaluatedModifiers {
        // A part with the same name as its core contributes no modifier.
        let penalty = if core.name == part_name {
            0.0
        } else {
            self.penalty_for(core_idx, category)
        };

        let adjusted_damage = damage_mod * penalty;
        let adjusted_fire_rate = fire_rate_mod * penalty;

        EvaluatedModifiers {
            damage_factor: 1.0 + adjusted_damage / 100.0,
            fire_rate_factor: 1.0 + adjusted_fire_rate / 100.0,
            score: adjusted_damage + adjusted_fire_rate * 0.6,
        }
    }

    #[inline]
    fn penalty_for(&self, core_idx: usize, category: &str) -> f64 {
        let Some(&part_idx) = self.data.categories.get(category) else {
            return 1.0;
        };

        self.data
            .penalties
            .get(core_idx)
            .and_then(|row| row.get(part_idx))
            .copied()
            .unwrap_or(1.0)
    }
}

#[derive(Clone, Copy)]
struct Ranking {
    key: SortKey,
    priority: SortPriority,
}

impl Ranking {
    fn from_config(config: &Config) -> Self {
        let priority = match config.priority {
            SortPriority::Auto if config.sort_key == SortKey::Ttk => SortPriority::Lowest,
            SortPriority::Auto => SortPriority::Highest,
            explicit => explicit,
        };

        Self {
            key: config.sort_key,
            priority,
        }
    }

    #[inline]
    fn better(self, left: f64, right: f64) -> bool {
        match self.priority {
            SortPriority::Highest => left > right,
            SortPriority::Lowest => left < right,
            SortPriority::Auto => unreachable!("auto priority is resolved when Ranking is built"),
        }
    }

    #[inline]
    fn result_metric(self, result: &ResultRow) -> f64 {
        match self.key {
            SortKey::Ttk => result.ttk_seconds,
            SortKey::Dps => result.dps,
            SortKey::Damage => result.damage,
            SortKey::DamageEnd => result.damage_end,
            SortKey::FireRate => result.fire_rate,
            SortKey::Magazine => result.magazine_size,
        }
    }

    #[inline]
    fn metrics_metric(self, metrics: ResultMetrics) -> f64 {
        match self.key {
            SortKey::Ttk => metrics.ttk_seconds,
            SortKey::Dps => metrics.dps,
            SortKey::Damage => metrics.damage,
            SortKey::DamageEnd => metrics.damage_end,
            SortKey::FireRate => metrics.fire_rate,
            SortKey::Magazine => metrics.magazine_size,
        }
    }
}

struct CoreOutcome {
    results: Vec<ResultRow>,
    stats: CalculationStats,
}

#[derive(Clone, Copy)]
struct EvaluatedModifiers {
    damage_factor: f64,
    fire_rate_factor: f64,
    score: f64,
}

#[derive(Clone, Copy)]
struct EvaluatedPart<'a> {
    part: &'a Part,
    damage_factor: f64,
    fire_rate_factor: f64,
    score: f64,
}

#[derive(Clone, Copy)]
struct EvaluatedMagazine<'a> {
    magazine: &'a Magazine,
    damage_factor: f64,
    fire_rate_factor: f64,
    score: f64,
}

#[derive(Clone, Copy)]
struct ResultMetrics {
    damage: f64,
    damage_end: f64,
    fire_rate: f64,
    magazine_size: f64,
    ttk_seconds: f64,
    dps: f64,
}

#[inline]
fn evaluate_metrics(
    config: &Config,
    core: &Core,
    damage_factor: f64,
    fire_rate_factor: f64,
    magazine_size: f64,
) -> Option<ResultMetrics> {
    let damage = core.damage * damage_factor;
    let fire_rate = core.fire_rate * fire_rate_factor;

    if damage <= 0.0 || fire_rate <= 0.0 {
        return None;
    }

    let shots = (config.player_max_health / damage).ceil();
    let ttk_seconds = ((shots - 1.0) / fire_rate) * 60.0;

    Some(ResultMetrics {
        damage,
        damage_end: core.damage_end * damage_factor,
        fire_rate,
        magazine_size,
        ttk_seconds,
        dps: damage * fire_rate / 60.0,
    })
}

#[inline]
fn passes_filters(config: &Config, metrics: ResultMetrics) -> bool {
    config.damage_range.contains(metrics.damage)
        && config.damage_end_range.contains(metrics.damage_end)
        && config.ttk_seconds_range.contains(metrics.ttk_seconds)
        && config.dps_range.contains(metrics.dps)
}

#[inline]
fn include_category(allowed: &[String], category: &str) -> bool {
    allowed.is_empty()
        || allowed
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(category))
}

#[inline]
fn can_enter_top(
    results: &[ResultRow],
    metrics: ResultMetrics,
    top_n: usize,
    ranking: Ranking,
) -> bool {
    if top_n == 0 {
        return false;
    }
    if results.len() < top_n {
        return true;
    }

    let worst_metric = ranking.result_metric(
        results
            .last()
            .expect("a full top-N list must contain at least one result"),
    );
    ranking.better(ranking.metrics_metric(metrics), worst_metric)
}

fn insert_top_result(
    results: &mut Vec<ResultRow>,
    candidate: ResultRow,
    top_n: usize,
    ranking: Ranking,
) {
    if top_n == 0 {
        return;
    }

    if results.len() >= top_n {
        let worst = results
            .last()
            .expect("a full top-N list must contain at least one result");
        if !ranking.better(ranking.result_metric(&candidate), ranking.result_metric(worst)) {
            return;
        }
    }

    // Keep equal-ranked entries in their original encounter order.
    let insertion_index = results.partition_point(|existing| {
        !ranking.better(
            ranking.result_metric(&candidate),
            ranking.result_metric(existing),
        )
    });

    results.insert(insertion_index, candidate);
    if results.len() > top_n {
        results.pop();
    }
}

pub fn write_results(results: &[ResultRow]) -> String {
    let mut out = String::new();

    for (idx, result) in results.iter().enumerate() {
        let _ = writeln!(out, "#{}", idx + 1);
        let _ = writeln!(out, " Core: {}", result.core);
        let _ = writeln!(out, " Magazine: {}", result.magazine);
        let _ = writeln!(out, " Barrel: {}", result.barrel);
        let _ = writeln!(out, " Stock: {}", result.stock);
        let _ = writeln!(out, " Grip: {}", result.grip);
        let _ = writeln!(out, " Damage: {:.3}", result.damage);
        let _ = writeln!(out, " Damage End: {:.3}", result.damage_end);
        let _ = writeln!(out, " Fire Rate: {:.3}", result.fire_rate);
        let _ = writeln!(out, " TTK: {:.3}s", result.ttk_seconds);
        let _ = writeln!(out, " DPS: {:.3}\n", result.dps);
    }

    out
}
