use std::{collections::HashMap, fs};

use anyhow::{Context, Result};
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
    let conn = Connection::open(path).with_context(|| format!("opening SQLite database {path}"))?;
    Ok(DataSet {
        categories: load_categories_sqlite(&conn)?,
        penalties: load_penalties_sqlite(&conn)?,
        cores: load_cores_sqlite(&conn)?,
        magazines: load_magazines_sqlite(&conn)?,
        barrels: load_parts_sqlite(&conn, "Barrels")?,
        grips: load_parts_sqlite(&conn, "Grips")?,
        stocks: load_parts_sqlite(&conn, "Stocks")?,
    })
}

fn load_cores_sqlite(conn: &Connection) -> Result<Vec<Core>> {
    let mut stmt = conn.prepare("SELECT name, category, damage, damage_end, fire_rate FROM cores ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok(Core {
            name: row.get(0)?, category: row.get(1)?, damage: row.get(2)?, damage_end: row.get(3)?, fire_rate: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_magazines_sqlite(conn: &Connection) -> Result<Vec<Magazine>> {
    let mut stmt = conn.prepare("SELECT name, category, magazine_size, reload_time, damage_mod, fire_rate_mod FROM magazines ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok(Magazine {
            name: row.get(0)?, category: row.get(1)?, magazine_size: row.get(2)?, reload_time: row.get(3)?, damage_mod: row.get(4)?, fire_rate_mod: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_parts_sqlite(conn: &Connection, part_type: &str) -> Result<Vec<Part>> {
    let mut stmt = conn.prepare("SELECT name, category, damage_mod, fire_rate_mod FROM parts WHERE part_type = ?1 ORDER BY id")?;
    let rows = stmt.query_map([part_type], |row| {
        Ok(Part { name: row.get(0)?, category: row.get(1)?, damage_mod: row.get(2)?, fire_rate_mod: row.get(3)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_categories_sqlite(conn: &Connection) -> Result<HashMap<String, usize>> {
    let mut stmt = conn.prepare("SELECT name, idx FROM categories")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize)))?;
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

fn load_penalties_sqlite(conn: &Connection) -> Result<Vec<Vec<f64>>> {
    let mut stmt = conn.prepare("SELECT core_idx, part_idx, value FROM penalties")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)? as usize, row.get::<_, i64>(1)? as usize, row.get::<_, f64>(2)?)))?;
    let entries = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let Some(max_row) = entries.iter().map(|(r, _, _)| *r).max() else { return Ok(Vec::new()); };
    let max_col = entries.iter().map(|(_, c, _)| *c).max().unwrap_or(0);
    let mut matrix = vec![vec![1.0; max_col + 1]; max_row + 1];
    for (r, c, value) in entries { matrix[r][c] = value; }
    Ok(matrix)
}

fn load_json_data(path: &str) -> Result<DataSet> {
    let root: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let data = root.get("Data").and_then(Value::as_object).context("missing Data object")?;
    Ok(DataSet {
        categories: parse_category_map(&root)?,
        penalties: root.get("Penalties").and_then(Value::as_array).context("missing Penalties")?
            .iter().map(|row| row.as_array().context("penalty row is not an array")?.iter().map(number_to_f).collect()).collect::<Result<_>>()?,
        cores: parse_cores(data.get("Cores").and_then(Value::as_array).context("missing Cores")?)?,
        magazines: parse_magazines(data.get("Magazines").and_then(Value::as_array).context("missing Magazines")?)?,
        barrels: parse_parts(data.get("Barrels").and_then(Value::as_array).context("missing Barrels")?)?,
        grips: parse_parts(data.get("Grips").and_then(Value::as_array).context("missing Grips")?)?,
        stocks: parse_parts(data.get("Stocks").and_then(Value::as_array).context("missing Stocks")?)?,
    })
}

fn parse_cores(nodes: &[Value]) -> Result<Vec<Core>> {
    nodes.iter().map(|node| {
        let obj = node.as_object().context("core must be an object")?;
        let (damage, damage_end) = parse_damage_pair(obj.get("Damage"))?;
        Ok(Core {
            name: obj.get("Name").and_then(Value::as_str).context("core Name")?.into(),
            category: obj.get("Category").and_then(Value::as_str).context("core Category")?.into(),
            damage, damage_end, fire_rate: optional_number(obj.get("Fire_Rate"))?,
        })
    }).collect()
}

fn parse_magazines(nodes: &[Value]) -> Result<Vec<Magazine>> {
    nodes.iter().map(|node| {
        let obj = node.as_object().context("magazine must be an object")?;
        Ok(Magazine {
            name: obj.get("Name").and_then(Value::as_str).context("magazine Name")?.into(),
            category: obj.get("Category").and_then(Value::as_str).context("magazine Category")?.into(),
            magazine_size: optional_number(obj.get("Magazine_Size"))?, reload_time: optional_number(obj.get("Reload_Time"))?,
            damage_mod: optional_number(obj.get("Damage"))?, fire_rate_mod: optional_number(obj.get("Fire_Rate"))?,
        })
    }).collect()
}

fn parse_parts(nodes: &[Value]) -> Result<Vec<Part>> {
    nodes.iter().map(|node| {
        let obj = node.as_object().context("part must be an object")?;
        Ok(Part {
            name: obj.get("Name").and_then(Value::as_str).context("part Name")?.into(),
            category: obj.get("Category").and_then(Value::as_str).context("part Category")?.into(),
            damage_mod: optional_number(obj.get("Damage"))?, fire_rate_mod: optional_number(obj.get("Fire_Rate"))?,
        })
    }).collect()
}

fn parse_damage_pair(value: Option<&Value>) -> Result<(f64, f64)> {
    let Some(value) = value else { return Ok((0.0, 0.0)); };
    if let Some(arr) = value.as_array() {
        let first = arr.first().map(number_to_f).transpose()?.unwrap_or(0.0);
        let second = arr.get(1).map(number_to_f).transpose()?.unwrap_or(first);
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
    let groups = root.get("Categories").and_then(Value::as_object).context("missing Categories")?;
    let mut map = HashMap::new();
    for group_name in ["Primary", "Secondary"] {
        if let Some(group) = groups.get(group_name).and_then(Value::as_object) {
            for (name, idx) in group { map.insert(name.clone(), number_to_f(idx)? as usize); }
        }
    }
    Ok(map)
}

fn number_to_f(value: &Value) -> Result<f64> {
    value.as_f64().context("expected numeric value")
}
