use std::time::Instant;

use clap::Parser;
use weirdgungamecalc::{
    engine::write_results, parser::load_data, Config, Engine, NumericRange, SortKey, SortPriority,
};

#[derive(Debug, Parser)]
#[command(
    name = "wggcalc",
    about = "Find the best Weird Gun Game build",
    version
)]
struct Cli {
    #[arg(long, default_value = "Data/FullData.sqlite3")]
    data: String,
    #[arg(long, default_value_t = 10)]
    top: usize,
    #[arg(long = "mh", default_value_t = 100.0)]
    max_health: f64,
    #[arg(long, default_value = "ttk", value_name = "KEY")]
    sort: String,
    #[arg(long, default_value = "auto", value_name = "MODE")]
    priority: String,
    #[arg(long, value_delimiter = ',', value_name = "LIST")]
    include: Vec<String>,
    #[arg(long = "part-pool", default_value_t = 20)]
    part_pool: usize,
    #[arg(long = "damage-min")]
    damage_min: Option<f64>,
    #[arg(long = "damage-max")]
    damage_max: Option<f64>,
    #[arg(long = "damage-end-min")]
    damage_end_min: Option<f64>,
    #[arg(long = "damage-end-max")]
    damage_end_max: Option<f64>,
    #[arg(long = "ttk-min")]
    ttk_min: Option<f64>,
    #[arg(long = "ttk-max")]
    ttk_max: Option<f64>,
    #[arg(long = "dps-min")]
    dps_min: Option<f64>,
    #[arg(long = "dps-max")]
    dps_max: Option<f64>,
    #[arg(long)]
    metrics: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config {
        data_path: cli.data,
        top_n: cli.top,
        player_max_health: cli.max_health,
        sort_key: SortKey::parse(&cli.sort)?,
        priority: SortPriority::parse(&cli.priority)?,
        include_categories: cli.include.into_iter().filter(|s| !s.is_empty()).collect(),
        damage_range: NumericRange::new(cli.damage_min, cli.damage_max),
        damage_end_range: NumericRange::new(cli.damage_end_min, cli.damage_end_max),
        ttk_seconds_range: NumericRange::new(cli.ttk_min, cli.ttk_max),
        dps_range: NumericRange::new(cli.dps_min, cli.dps_max),
        part_pool_per_type: cli.part_pool,
    };

    let total_start = Instant::now();
    let load_start = Instant::now();
    let data = load_data(&config.data_path)?;
    let load_elapsed = load_start.elapsed();
    let calc_start = Instant::now();
    let (results, stats) = Engine::new(&data).calculate_top(&config);
    let calc_elapsed = calc_start.elapsed();
    let total_elapsed = total_start.elapsed();

    println!(
        "Loaded {} cores, {} magazines, {} barrels, {} stocks, {} grips\n",
        data.cores.len(),
        data.magazines.len(),
        data.barrels.len(),
        data.stocks.len(),
        data.grips.len()
    );
    print!("{}", write_results(&results));

    if cli.metrics {
        println!("Performance metrics:");
        println!("  Data load: {:.3} ms", load_elapsed.as_secs_f64() * 1000.0);
        println!(
            "  Calculation: {:.3} ms",
            calc_elapsed.as_secs_f64() * 1000.0
        );
        println!(
            "  Total runtime: {:.3} ms",
            total_elapsed.as_secs_f64() * 1000.0
        );
        println!("  Cores considered: {}", stats.cores_considered);
        println!(
            "  Cores skipped by category: {}",
            stats.cores_skipped_by_category
        );
        println!("  Combinations evaluated: {}", stats.combinations_evaluated);
        println!("  Combinations filtered: {}", stats.combinations_filtered);
        println!("  Results kept: {}", stats.results_kept);
    }
    Ok(())
}
