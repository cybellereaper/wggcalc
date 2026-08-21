use std::{cmp::Ordering, collections::HashMap};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NumericRange {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl NumericRange {
    pub const fn new(min: Option<f64>, max: Option<f64>) -> Self {
        Self { min, max }
    }

    pub fn contains(self, value: f64) -> bool {
        if self.min.is_some_and(|min| value < min) {
            return false;
        }
        if self.max.is_some_and(|max| value > max) {
            return false;
        }
        true
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
        let mut results = Vec::new();
        let mut stats = CalculationStats::default();

        for core in &self.data.cores {
            stats.cores_considered += 1;
            if !include_category(&config.include_categories, &core.category) {
                stats.cores_skipped_by_category += 1;
                continue;
            }

            let Some(&core_idx) = self.data.categories.get(&core.category) else {
                continue;
            };

            let magazines = self.top_magazines(core, core_idx, config.part_pool_per_type);
            let barrels = self.top_parts(&self.data.barrels, core, core_idx, config.part_pool_per_type);
            let stocks = self.top_parts(&self.data.stocks, core, core_idx, config.part_pool_per_type);
            let grips = self.top_parts(&self.data.grips, core, core_idx, config.part_pool_per_type);

            for mag in magazines {
                for barrel in &barrels {
                    for stock in &stocks {
                        for grip in &grips {
                            stats.combinations_evaluated += 1;
                            let Some(candidate) = self.build_result(config, core, core_idx, mag, barrel, stock, grip) else {
                                continue;
                            };
                            if !passes_filters(config, &candidate) {
                                stats.combinations_filtered += 1;
                                continue;
                            }
                            self.push_top(&mut results, candidate, config);
                        }
                    }
                }
            }
        }

        stats.results_kept = results.len();
        (results, stats)
    }

    fn push_top(&self, results: &mut Vec<ResultRow>, candidate: ResultRow, config: &Config) {
        if config.top_n == 0 {
            return;
        }

        if results.len() < config.top_n {
            results.push(candidate);
        } else if better(&candidate, results.last().expect("non-empty top list"), config) {
            *results.last_mut().expect("non-empty top list") = candidate;
        } else {
            return;
        }

        results.sort_by(|a, b| {
            if better(a, b, config) {
                Ordering::Less
            } else if better(b, a, config) {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        });
    }

    fn build_result(
        &self,
        config: &Config,
        core: &Core,
        core_idx: usize,
        mag: &Magazine,
        barrel: &Part,
        stock: &Part,
        grip: &Part,
    ) -> Option<ResultRow> {
        let damage_parts = [
            (&mag.name, &mag.category, mag.damage_mod),
            (&barrel.name, &barrel.category, barrel.damage_mod),
            (&stock.name, &stock.category, stock.damage_mod),
            (&grip.name, &grip.category, grip.damage_mod),
        ];
        let fire_rate_parts = [
            (&mag.name, &mag.category, mag.fire_rate_mod),
            (&barrel.name, &barrel.category, barrel.fire_rate_mod),
            (&stock.name, &stock.category, stock.fire_rate_mod),
            (&grip.name, &grip.category, grip.fire_rate_mod),
        ];

        let damage_mult = self.percent_multiplier(core, core_idx, &damage_parts);
        let fire_rate_mult = self.percent_multiplier(core, core_idx, &fire_rate_parts);
        let damage = core.damage * damage_mult;
        let fire_rate = core.fire_rate * fire_rate_mult;
        if damage <= 0.0 || fire_rate <= 0.0 {
            return None;
        }

        let shots = (config.player_max_health / damage).ceil();
        let ttk_seconds = ((shots - 1.0) / fire_rate) * 60.0;

        Some(ResultRow {
            core: core.name.clone(),
            magazine: mag.name.clone(),
            barrel: barrel.name.clone(),
            stock: stock.name.clone(),
            grip: grip.name.clone(),
            damage,
            damage_end: core.damage_end * damage_mult,
            fire_rate,
            magazine_size: mag.magazine_size,
            ttk_seconds,
            dps: damage * fire_rate / 60.0,
        })
    }

    fn percent_multiplier(&self, core: &Core, core_idx: usize, parts: &[(&String, &String, f64)]) -> f64 {
        parts.iter().fold(1.0, |mult, (name, category, raw_mod)| {
            let penalty = self.penalty_for(core_idx, category);
            mult * (1.0 + adjusted_mod(*raw_mod, &core.name, name, penalty) / 100.0)
        })
    }

    fn top_parts<'b>(&self, pool: &'b [Part], core: &Core, core_idx: usize, max_count: usize) -> Vec<&'b Part> {
        let mut parts: Vec<_> = pool.iter().collect();
        parts.sort_by(|a, b| {
            self.part_score(core, core_idx, &b.name, &b.category, b.damage_mod, b.fire_rate_mod)
                .partial_cmp(&self.part_score(core, core_idx, &a.name, &a.category, a.damage_mod, a.fire_rate_mod))
                .unwrap_or(Ordering::Equal)
        });
        parts.truncate(max_count);
        parts
    }

    fn top_magazines<'b>(&'b self, core: &Core, core_idx: usize, max_count: usize) -> Vec<&'b Magazine> {
        let mut magazines: Vec<_> = self.data.magazines.iter().collect();
        magazines.sort_by(|a, b| {
            let score_a = self.part_score(core, core_idx, &a.name, &a.category, a.damage_mod, a.fire_rate_mod)
                + a.magazine_size * 0.05;
            let score_b = self.part_score(core, core_idx, &b.name, &b.category, b.damage_mod, b.fire_rate_mod)
                + b.magazine_size * 0.05;
            score_b.partial_cmp(&score_a).unwrap_or(Ordering::Equal)
        });
        magazines.truncate(max_count);
        magazines
    }

    fn part_score(&self, core: &Core, core_idx: usize, part_name: &str, category: &str, damage_mod: f64, fire_rate_mod: f64) -> f64 {
        let penalty = self.penalty_for(core_idx, category);
        adjusted_mod(damage_mod, &core.name, part_name, penalty)
            + adjusted_mod(fire_rate_mod, &core.name, part_name, penalty) * 0.6
    }

    fn penalty_for(&self, core_idx: usize, category: &str) -> f64 {
        let Some(&part_idx) = self.data.categories.get(category) else {
            return 1.0;
        };
        self.data.penalties
            .get(core_idx)
            .and_then(|row| row.get(part_idx))
            .copied()
            .unwrap_or(1.0)
    }
}

fn adjusted_mod(raw: f64, core_name: &str, part_name: &str, penalty: f64) -> f64 {
    if core_name == part_name { 0.0 } else { raw * penalty }
}

fn passes_filters(config: &Config, result: &ResultRow) -> bool {
    config.damage_range.contains(result.damage)
        && config.damage_end_range.contains(result.damage_end)
        && config.ttk_seconds_range.contains(result.ttk_seconds)
        && config.dps_range.contains(result.dps)
}

fn include_category(allowed: &[String], category: &str) -> bool {
    allowed.is_empty() || allowed.iter().any(|candidate| candidate.eq_ignore_ascii_case(category))
}

fn better(a: &ResultRow, b: &ResultRow, config: &Config) -> bool {
    let left = metric(a, config.sort_key);
    let right = metric(b, config.sort_key);
    let priority = match config.priority {
        SortPriority::Auto if config.sort_key == SortKey::Ttk => SortPriority::Lowest,
        SortPriority::Auto => SortPriority::Highest,
        explicit => explicit,
    };
    match priority {
        SortPriority::Highest => left > right,
        SortPriority::Lowest => left < right,
        SortPriority::Auto => unreachable!(),
    }
}

fn metric(result: &ResultRow, key: SortKey) -> f64 {
    match key {
        SortKey::Ttk => result.ttk_seconds,
        SortKey::Dps => result.dps,
        SortKey::Damage => result.damage,
        SortKey::DamageEnd => result.damage_end,
        SortKey::FireRate => result.fire_rate,
        SortKey::Magazine => result.magazine_size,
    }
}

pub fn write_results(results: &[ResultRow]) -> String {
    let mut out = String::new();
    for (idx, r) in results.iter().enumerate() {
        use std::fmt::Write;
        let _ = writeln!(out, "#{}", idx + 1);
        let _ = writeln!(out, " Core: {}", r.core);
        let _ = writeln!(out, " Magazine: {}", r.magazine);
        let _ = writeln!(out, " Barrel: {}", r.barrel);
        let _ = writeln!(out, " Stock: {}", r.stock);
        let _ = writeln!(out, " Grip: {}", r.grip);
        let _ = writeln!(out, " Damage: {:.3}", r.damage);
        let _ = writeln!(out, " Damage End: {:.3}", r.damage_end);
        let _ = writeln!(out, " Fire Rate: {:.3}", r.fire_rate);
        let _ = writeln!(out, " TTK: {:.3}s", r.ttk_seconds);
        let _ = writeln!(out, " DPS: {:.3}\n", r.dps);
    }
    out
}
