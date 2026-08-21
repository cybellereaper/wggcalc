use std::{collections::HashMap, fs};

use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use serde_json::Value;

use crate::{Core, DataSet, Magazine, Part};

pub fn load_data(path: &str) -> Result<DataSet> {
    if sqlite_path(path) {
        load_sqlite_data(path)
    } else {
        load_json_data(path)
    }
}

fn sqlite_path(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    lowered.ends_with(".sqlite") || lowered.ends_with(".sqlite3") || lowered.ends_with(".db")
}

fn load_sqlite_data(path: &str) -> Result<DataSet> {
    let conn =
        Connection::open(path).with_context(|| format!("opening SQLite database {path}"))?;

    let categories = load_categories_sqlite(&conn)?;
    let category_count = categories
        .values()
        .copied()
        .max()
        .map_or(0, |max_index| max_index + 1);
    let penalties = load_penalties_sqlite(&conn, category_count)?;

    Ok(DataSet {
        cores: load_cores_sqlite(&conn)?,
        magazines: load_magazines_sqlite(&conn)?,
        barrels: load_parts_sqlite(&conn, "Barrels")?,
        grips: load_parts_sqlite(&conn, "Grips")?,
        stocks: load_parts_sqlite(&conn, "Stocks")?,
        penalties,
        categories,
    })
}

fn load_cores_sqlite(conn: &Connection) -> Result<Vec<Core>> {
    let mut stmt = conn.prepare(
        "SELECT name, category, damage, damage_end, fire_rate FROM cores ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Core {
            name: row.get(0)?,
            category: row.get(1)?,
            damage: row.get(2)?,
            damage_end: row.get(3)?,
            fire_rate: row.get(4)?,
        })
    })?;

    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_magazines_sqlite(conn: &Connection) -> Result<Vec<Magazine>> {
    let mut stmt = conn.prepare(
        "SELECT name, category, magazine_size, reload_time, damage_mod, fire_rate_mod \
         FROM magazines ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Magazine {
            name: row.get(0)?,
            category: row.get(1)?,
            magazine_size: row.get(2)?,
            reload_time: row.get(3)?,
            damage_mod: row.get(4)?,
            fire_rate_mod: row.get(5)?,
        })
    })?;

    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_parts_sqlite(conn: &Connection, part_type: &str) -> Result<Vec<Part>> {
    let mut stmt = conn.prepare(
        "SELECT name, category, damage_mod, fire_rate_mod \
         FROM parts WHERE part_type = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([part_type], |row| {
        Ok(Part {
            name: row.get(0)?,
            category: row.get(1)?,
            damage_mod: row.get(2)?,
            fire_rate_mod: row.get(3)?,
        })
    })?;

    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_categories_sqlite(conn: &Connection) -> Result<HashMap<String, usize>> {
    let mut stmt = conn.prepare("SELECT name, idx FROM categories")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    let mut categories = HashMap::new();
    for row in rows {
        let (name, raw_index) = row?;
        let index = usize::try_from(raw_index)
            .with_context(|| format!("category {name:?} has negative index {raw_index}"))?;
        categories.insert(name, index);
    }

    Ok(categories)
}

fn load_penalties_sqlite(conn: &Connection, category_count: usize) -> Result<Vec<Vec<f64>>> {
    let mut matrix = vec![vec![1.0; category_count]; category_count];
    let mut stmt = conn.prepare("SELECT core_idx, part_idx, value FROM penalties")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;

    for row in rows {
        let (raw_core_idx, raw_part_idx, value) = row?;
        let core_idx = usize::try_from(raw_core_idx)
            .with_context(|| format!("negative penalty core index {raw_core_idx}"))?;
        let part_idx = usize::try_from(raw_part_idx)
            .with_context(|| format!("negative penalty part index {raw_part_idx}"))?;

        ensure!(
            core_idx < category_count && part_idx < category_count,
            "penalty index ({core_idx}, {part_idx}) is outside category matrix size {category_count}"
        );

        matrix[core_idx][part_idx] = value;
    }

    Ok(matrix)
}

fn load_json_data(path: &str) -> Result<DataSet> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("reading legacy JSON data from {path}"))?;
    let root: Value =
        serde_json::from_str(&raw).with_context(|| format!("parsing legacy JSON data from {path}"))?;
    let data = root
        .get("Data")
        .and_then(Value::as_object)
        .context("missing Data object")?;

    let penalties = root
        .get("Penalties")
        .and_then(Value::as_array)
        .context("missing Penalties")?
        .iter()
        .map(|row| {
            row.as_array()
                .context("penalty row is not an array")?
                .iter()
                .map(number_to_f)
                .collect()
        })
        .collect::<Result<_>>()?;

    Ok(DataSet {
        cores: parse_cores(
            data.get("Cores")
                .and_then(Value::as_array)
                .context("missing Cores")?,
        )?,
        magazines: parse_magazines(
            data.get("Magazines")
                .and_then(Value::as_array)
                .context("missing Magazines")?,
        )?,
        barrels: parse_parts(
            data.get("Barrels")
                .and_then(Value::as_array)
                .context("missing Barrels")?,
        )?,
        grips: parse_parts(
            data.get("Grips")
                .and_then(Value::as_array)
                .context("missing Grips")?,
        )?,
        stocks: parse_parts(
            data.get("Stocks")
                .and_then(Value::as_array)
                .context("missing Stocks")?,
        )?,
        penalties,
        categories: parse_category_map(&root)?,
    })
}

fn parse_cores(nodes: &[Value]) -> Result<Vec<Core>> {
    nodes
        .iter()
        .map(|node| {
            let obj = node.as_object().context("core must be an object")?;
            let (damage, damage_end) = parse_damage_pair(obj.get("Damage"))?;

            Ok(Core {
                name: required_string(obj.get("Name"), "core Name")?.into(),
                category: required_string(obj.get("Category"), "core Category")?.into(),
                damage,
                damage_end,
                fire_rate: optional_number(obj.get("Fire_Rate"))?,
            })
        })
        .collect()
}

fn parse_magazines(nodes: &[Value]) -> Result<Vec<Magazine>> {
    nodes
        .iter()
        .map(|node| {
            let obj = node.as_object().context("magazine must be an object")?;

            Ok(Magazine {
                name: required_string(obj.get("Name"), "magazine Name")?.into(),
                category: required_string(obj.get("Category"), "magazine Category")?.into(),
                magazine_size: optional_number(obj.get("Magazine_Size"))?,
                reload_time: optional_number(obj.get("Reload_Time"))?,
                damage_mod: optional_number(obj.get("Damage"))?,
                fire_rate_mod: optional_number(obj.get("Fire_Rate"))?,
            })
        })
        .collect()
}

fn parse_parts(nodes: &[Value]) -> Result<Vec<Part>> {
    nodes
        .iter()
        .map(|node| {
            let obj = node.as_object().context("part must be an object")?;

            Ok(Part {
                name: required_string(obj.get("Name"), "part Name")?.into(),
                category: required_string(obj.get("Category"), "part Category")?.into(),
                damage_mod: optional_number(obj.get("Damage"))?,
                fire_rate_mod: optional_number(obj.get("Fire_Rate"))?,
            })
        })
        .collect()
}

fn required_string<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a str> {
    value.and_then(Value::as_str).with_context(|| field.to_string())
}

fn parse_damage_pair(value: Option<&Value>) -> Result<(f64, f64)> {
    let Some(value) = value else {
        return Ok((0.0, 0.0));
    };

    if let Some(values) = value.as_array() {
        let first = values.first().map(number_to_f).transpose()?.unwrap_or(0.0);
        let second = values.get(1).map(number_to_f).transpose()?.unwrap_or(first);
        Ok((first, second))
    } else {
        let damage = number_to_f(value)?;
        Ok((damage, damage))
    }
}

fn optional_number(value: Option<&Value>) -> Result<f64> {
    value.map(number_to_f).transpose().map(|v| v.unwrap_or(0.0))
}

fn parse_category_map(root: &Value) -> Result<HashMap<String, usize>> {
    let groups = root
        .get("Categories")
        .and_then(Value::as_object)
        .context("missing Categories")?;
    let mut map = HashMap::new();

    for group_name in ["Primary", "Secondary"] {
        if let Some(group) = groups.get(group_name).and_then(Value::as_object) {
            for (name, value) in group {
                map.insert(name.clone(), number_to_index(value)?);
            }
        }
    }

    Ok(map)
}

fn number_to_index(value: &Value) -> Result<usize> {
    let number = number_to_f(value)?;
    ensure!(
        number.is_finite() && number >= 0.0 && number.fract() == 0.0,
        "expected non-negative integer category index"
    );
    ensure!(
        number <= usize::MAX as f64,
        "category index is too large for this platform"
    );
    Ok(number as usize)
}

fn number_to_f(value: &Value) -> Result<f64> {
    value.as_f64().context("expected numeric value")
}
