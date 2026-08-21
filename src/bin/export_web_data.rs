use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::{json, Value};
use weirdgungamecalc::parser::load_data;

#[derive(Debug, Parser)]
#[command(
    name = "export_web_data",
    about = "Export calculator data for the static web app"
)]
struct Cli {
    #[arg(long, default_value = "Data/FullData.sqlite3")]
    data: String,
    #[arg(long, default_value = "docs/data.json")]
    output: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let data = load_data(&cli.data)?;

    let categories = data
        .categories
        .iter()
        .map(|(name, index)| (name.clone(), *index))
        .collect::<BTreeMap<_, _>>();

    let web_data = json!({
        "version": 1,
        "cores": data.cores.iter().map(|core| json!({
            "name": core.name,
            "category": core.category,
            "damage": core.damage,
            "damage_end": core.damage_end,
            "fire_rate": core.fire_rate,
        })).collect::<Vec<Value>>(),
        "magazines": data.magazines.iter().map(|magazine| json!({
            "name": magazine.name,
            "category": magazine.category,
            "magazine_size": magazine.magazine_size,
            "damage_mod": magazine.damage_mod,
            "fire_rate_mod": magazine.fire_rate_mod,
        })).collect::<Vec<Value>>(),
        "barrels": export_parts(&data.barrels),
        "stocks": export_parts(&data.stocks),
        "grips": export_parts(&data.grips),
        "penalties": data.penalties,
        "categories": categories,
    });

    if let Some(parent) = cli.output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating output directory {}", parent.display()))?;
    }

    let encoded = serde_json::to_vec(&web_data).context("serializing web dataset")?;
    fs::write(&cli.output, encoded)
        .with_context(|| format!("writing web dataset to {}", cli.output.display()))?;

    println!("Wrote {}", cli.output.display());
    Ok(())
}

fn export_parts(parts: &[weirdgungamecalc::Part]) -> Vec<Value> {
    parts
        .iter()
        .map(|part| {
            json!({
                "name": part.name,
                "category": part.category,
                "damage_mod": part.damage_mod,
                "fire_rate_mod": part.fire_rate_mod,
            })
        })
        .collect()
}
