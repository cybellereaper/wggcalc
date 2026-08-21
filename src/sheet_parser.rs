use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process,
};

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde_json::{Map, Number, Value};
use thiserror::Error;

pub const SHEET_ID: &str = "1Kc9aME3xlUC_vV5dFRe457OchqUOrwuiX_pQykjCF68";
pub const SHEET_FOLDER: &str = "SheetData";
pub const PARTS_V2_SHEET_GID: &str = "319672878";
pub const CORES_SHEET_GID: &str = "911413911";
pub const PARTS_V2_SHEET: &str = "SheetData/parts2.csv";
pub const CORES_SHEET: &str = "SheetData/cores.csv";
pub const OUTPUT_FILE: &str = "Data/FullData.sqlite3";

pub const VALID_PART_CATEGORIES: [&str; 8] = [
    "AR", "Sniper", "SMG", "LMG", "Shotgun", "BR", "Weird", "Sidearm",
];
pub const VALID_PART_TYPES: [&str; 4] = ["Barrels", "Magazines", "Grips", "Stocks"];
pub const VALID_PRICE_TYPES: [&str; 11] = [
    "Coin",
    "WC",
    "Follow",
    "Robux",
    "Free",
    "Spin",
    "Limited",
    "Missions",
    "Verify discord",
    "Season Pass 1",
    "Unknown",
];

const PART_PROPERTY_NAMES: [&str; 14] = [
    "Magazine_Size",
    "Reload_Time",
    "Damage",
    "Detection_Radius",
    "Equip_Time",
    "Fire_Rate",
    "Health",
    "Magazine_Cap",
    "Movement_Speed",
    "Pellets",
    "Range",
    "Recoil",
    "Reload_Speed",
    "Spread",
];

const CORE_PROPERTY_NAMES: [&str; 16] = [
    "Damage",
    "Dropoff_Studs",
    "Fire_Rate",
    "Hipfire_Spread",
    "ADS_Spread",
    "Time_To_Aim",
    "Detection_Radius",
    "Burst",
    "Movement_Speed_Modifier",
    "Suppression",
    "Health",
    "Equip_Time",
    "Recoil_Hip_Horizontal",
    "Recoil_Hip_Vertical",
    "Recoil_Aim_Horizontal",
    "Recoil_Aim_Vertical",
];

pub const CURRENT_PENALTIES: [[f64; 8]; 8] = [
    [1.00, 0.70, 0.75, 0.70, 0.75, 1.00, 0.80, 0.65],
    [0.70, 1.00, 0.60, 0.60, 0.80, 1.00, 0.85, 0.50],
    [0.80, 0.60, 1.00, 0.65, 0.65, 1.00, 0.70, 0.70],
    [0.70, 0.50, 0.65, 1.00, 0.75, 1.00, 0.60, 0.65],
    [0.75, 0.80, 0.65, 0.75, 1.00, 1.00, 0.85, 0.50],
    [1.00, 1.00, 1.00, 1.00, 1.00, 1.00, 1.00, 1.00],
    [0.80, 0.85, 0.70, 0.60, 0.85, 1.00, 1.00, 0.65],
    [0.65, 0.50, 0.75, 0.65, 0.50, 1.00, 0.65, 1.00],
];

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ParseError(pub String);

pub type Item = Map<String, Value>;
pub type ItemRows = Vec<Item>;
pub type DataSections = HashMap<String, ItemRows>;

#[derive(Debug, Clone)]
pub struct ExportData {
    pub data: DataSections,
    pub penalties: Vec<Vec<f64>>,
    pub categories: HashMap<String, HashMap<String, usize>>,
}

#[derive(Debug, Clone)]
pub struct SheetExport {
    pub gid: String,
    pub output_path: PathBuf,
    pub url_override: Option<String>,
}

impl SheetExport {
    pub fn new(gid: impl Into<String>, output_path: impl Into<PathBuf>) -> Self {
        Self {
            gid: gid.into(),
            output_path: output_path.into(),
            url_override: None,
        }
    }

    pub fn export_url(&self, sheet_id: &str) -> String {
        self.url_override.clone().unwrap_or_else(|| {
            format!(
                "https://docs.google.com/spreadsheets/d/{sheet_id}/export?format=csv&id={sheet_id}&gid={}",
                self.gid
            )
        })
    }
}

pub struct SheetDownloader {
    sheet_id: String,
    sheet_folder: PathBuf,
    client: Client,
}

impl SheetDownloader {
    pub fn new(sheet_id: impl Into<String>, sheet_folder: impl Into<PathBuf>) -> Result<Self> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;

        Ok(Self {
            sheet_id: sheet_id.into(),
            sheet_folder: sheet_folder.into(),
            client,
        })
    }

    pub fn download(&self, exports: &[SheetExport]) -> Result<()> {
        // Fetch everything before mutating SheetData. A transient network
        // failure therefore cannot destroy the last successfully downloaded set.
        let mut downloads = Vec::with_capacity(exports.len());
        for export in exports {
            let response = self
                .client
                .get(export.export_url(&self.sheet_id))
                .send()
                .with_context(|| format!("downloading {}", export.output_path.display()))?
                .error_for_status()
                .with_context(|| format!("downloading {}", export.output_path.display()))?;
            downloads.push((export.output_path.clone(), response.bytes()?.to_vec()));
        }

        clear_folder(&self.sheet_folder)?;

        for (output_path, contents) in downloads {
            create_parent_dir(&output_path)?;
            fs::write(&output_path, contents)
                .with_context(|| format!("writing {}", output_path.display()))?;
        }

        Ok(())
    }
}

fn create_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn clear_folder(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;

    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }

    Ok(())
}

pub fn normalize_numeric_value(
    raw_value: &str,
    expect_range: bool,
) -> std::result::Result<Value, ParseError> {
    let value = raw_value.trim();
    if value.is_empty() || value == "🎲" {
        return Ok(Value::Null);
    }

    let cleaned = value
        .replace('°', "")
        .replace('s', "")
        .replace("rpm", "")
        .replace('%', "")
        .replace(',', "");
    let cleaned = cleaned.trim();

    if expect_range {
        if let Some(prefix) = cleaned.strip_suffix('>') {
            return Ok(Value::Array(vec![
                number_value(parse_single_or_multiplier(prefix.trim())?),
                Value::Null,
            ]));
        }

        let normalized = cleaned.replace('>', "-");
        let pieces = normalized.split(" - ").collect::<Vec<_>>();
        if pieces.len() != 2 {
            return Err(ParseError(format!(
                "Expected numeric range but got: {raw_value:?}"
            )));
        }

        return Ok(Value::Array(vec![
            number_value(parse_single_or_multiplier(pieces[0].trim())?),
            number_value(parse_single_or_multiplier(pieces[1].trim())?),
        ]));
    }

    Ok(number_value(parse_single_or_multiplier(
        cleaned.replace('>', "-").trim(),
    )?))
}

pub fn detect_price_type(price: &str) -> String {
    let normalized = price.trim();
    if normalized.is_empty() {
        return "Coin".into();
    }
    if VALID_PRICE_TYPES.contains(&normalized) {
        return normalized.into();
    }

    let mut chars = normalized.chars();
    let capitalized = match chars.next() {
        Some(first) => {
            first.to_uppercase().collect::<String>() + &chars.as_str().to_ascii_lowercase()
        }
        None => String::new(),
    };
    if VALID_PRICE_TYPES.contains(&capitalized.as_str()) {
        return capitalized;
    }

    if normalized.contains("WC") || normalized == "Weird Boxes" {
        return "WC".into();
    }
    if normalized == "Exclusive Weird Boxes" {
        return "Robux".into();
    }
    if normalized.replace(',', "").parse::<i64>().is_ok() {
        return "Coin".into();
    }

    "Unknown".into()
}

fn parse_single_or_multiplier(value: &str) -> std::result::Result<f64, ParseError> {
    if let Some((left, right)) = value.split_once('x') {
        return Ok(parse_f64(left)? * parse_f64(right)?);
    }
    parse_f64(value)
}

fn parse_f64(value: &str) -> std::result::Result<f64, ParseError> {
    value
        .trim()
        .parse()
        .map_err(|_| ParseError(format!("Invalid numeric value: {value:?}")))
}

fn number_value(value: f64) -> Value {
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

pub struct PartsParser {
    seen: HashMap<(String, String), HashSet<String>>,
}

impl PartsParser {
    pub fn new() -> Self {
        let mut seen = HashMap::new();
        for category in VALID_PART_CATEGORIES {
            for part_type in VALID_PART_TYPES {
                seen.insert((category.into(), part_type.into()), HashSet::new());
            }
        }
        Self { seen }
    }

    pub fn parse_file(&mut self, path: &Path) -> Result<HashMap<String, ItemRows>> {
        let rows = read_csv_rows(path, 2, true)?;
        let truncated = rows
            .into_iter()
            .take_while(|row| !(row.len() >= 2 && row[1].starts_with("Notable ")))
            .collect::<Vec<_>>();
        self.parse_rows(&truncated).map_err(Into::into)
    }

    pub fn parse_rows(
        &mut self,
        rows: &[Vec<String>],
    ) -> std::result::Result<HashMap<String, ItemRows>, ParseError> {
        let mut output = VALID_PART_TYPES
            .into_iter()
            .map(|part_type| (part_type.to_string(), Vec::new()))
            .collect::<HashMap<String, ItemRows>>();
        let mut current_category = "AR".to_string();
        let mut current_type = String::new();

        for row in rows {
            if row.is_empty() {
                continue;
            }
            if row.len() != 17 {
                return Err(ParseError(format!(
                    "Invalid parts row length: expected 17, got {}",
                    row.len()
                )));
            }

            let name = row[1].trim();
            if let Some((category, part_type)) = parse_divider(name) {
                current_category = category;
                current_type = part_type;
                continue;
            }
            if current_type.is_empty() {
                return Err(ParseError("Part encountered before section header".into()));
            }

            let seen = self
                .seen
                .get_mut(&(current_category.clone(), current_type.clone()))
                .expect("known category/type");
            if !seen.insert(name.to_string()) {
                return Err(ParseError(format!("Duplicate part name {name}")));
            }

            let mut part = Item::new();
            part.insert("Price_Type".into(), detect_price_type(&row[0]).into());
            part.insert("Name".into(), name.into());
            part.insert("Category".into(), current_category.clone().into());

            for index in 2..=15 {
                let cell = row[index].trim();
                if cell.is_empty() {
                    continue;
                }
                part.insert(
                    PART_PROPERTY_NAMES[index - 2].into(),
                    normalize_numeric_value(&extract_leading_token(cell)?, false)?,
                );
            }

            output
                .get_mut(&current_type)
                .expect("known part type")
                .push(part);
        }

        Ok(output)
    }
}

impl Default for PartsParser {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_divider(name: &str) -> Option<(String, String)> {
    let mut parts = name.split_whitespace();
    let category = parts.next()?;
    let part_type = parts.next()?;

    if parts.next().is_some()
        || !VALID_PART_CATEGORIES.contains(&category)
        || !VALID_PART_TYPES.contains(&part_type)
    {
        return None;
    }

    Some((category.into(), part_type.into()))
}

pub struct CoresParser;

impl CoresParser {
    pub fn parse_file(&self, path: &Path) -> Result<ItemRows> {
        self.parse_rows(&read_csv_rows(path, 2, true)?)
            .map_err(Into::into)
    }

    pub fn parse_rows(
        &self,
        rows: &[Vec<String>],
    ) -> std::result::Result<ItemRows, ParseError> {
        const WIDTH: usize = 18;

        let mut output = Vec::new();
        let mut current_category = "AR".to_string();

        for row in rows {
            if row.is_empty() {
                continue;
            }
            if row.len() < WIDTH {
                return Err(ParseError(format!(
                    "Invalid cores row length: expected at least {WIDTH}, got {}",
                    row.len()
                )));
            }

            let row = &row[..WIDTH];
            let name = row[1].trim();

            if let Some(category) = name
                .strip_suffix(" Cores")
                .filter(|category| VALID_PART_CATEGORIES.contains(category))
            {
                current_category = category.into();
                continue;
            }

            let mut core = Item::new();
            core.insert("Price_Type".into(), detect_price_type(&row[0]).into());
            core.insert("Name".into(), name.into());
            core.insert("Category".into(), current_category.clone().into());

            for index in 2..=17 {
                let cell = &row[index];

                if index == 2 {
                    if let Some(pellets) = extract_pellets(cell) {
                        core.insert("Pellets".into(), pellets.into());
                    }
                }

                let value = normalize_numeric_value(cell, index <= 3 || index >= 14)?;
                if !value.is_null() {
                    core.insert(CORE_PROPERTY_NAMES[index - 2].into(), value);
                }
            }

            output.push(core);
        }

        Ok(output)
    }
}

fn extract_pellets(cell: &str) -> Option<i64> {
    let first = cell.split(" > ").next()?;
    let (_, right) = first.split_once('x')?;
    right.trim().parse().ok()
}

pub fn read_csv_rows(
    path: &Path,
    skip_header_rows: usize,
    trim_first_column: bool,
) -> Result<Vec<Vec<String>>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .with_context(|| format!("opening CSV {}", path.display()))?;
    let mut rows = Vec::new();

    for record in reader.records().skip(skip_header_rows) {
        let mut row = record?
            .iter()
            .map(str::to_string)
            .collect::<Vec<String>>();

        if trim_first_column && !row.is_empty() {
            row.remove(0);
        }

        rows.push(row);
    }

    Ok(rows)
}

pub fn extract_leading_token(cell: &str) -> std::result::Result<String, ParseError> {
    let (first, _) = cell
        .split_once(' ')
        .ok_or_else(|| ParseError(format!("Invalid property cell format: {cell:?}")))?;
    Ok(first.into())
}

pub fn build_full_data(parts_file: &Path, cores_file: &Path) -> Result<ExportData> {
    let parts = PartsParser::new().parse_file(parts_file)?;
    let cores = CoresParser.parse_file(cores_file)?;
    let mut data = parts;
    data.insert("Cores".into(), cores);

    let primary = [
        ("AR", 0),
        ("Sniper", 1),
        ("SMG", 2),
        ("Shotgun", 3),
        ("LMG", 4),
        ("Weird", 5),
        ("BR", 6),
    ]
    .into_iter()
    .map(|(name, index)| (name.to_string(), index))
    .collect();
    let secondary = HashMap::from([("Sidearm".to_string(), 7)]);

    Ok(ExportData {
        data,
        penalties: CURRENT_PENALTIES.iter().map(|row| row.to_vec()).collect(),
        categories: HashMap::from([
            ("Primary".into(), primary),
            ("Secondary".into(), secondary),
        ]),
    })
}

pub fn save_sqlite(export: &ExportData, output_path: &Path) -> Result<()> {
    create_parent_dir(output_path)?;

    let temp_path = temporary_database_path(output_path)?;
    if temp_path.exists() {
        fs::remove_file(&temp_path)?;
    }

    if let Err(error) = write_sqlite(export, &temp_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    // The existing database is only removed after the replacement has been
    // fully generated and committed, so parse/database failures do not destroy
    // the last known-good output.
    if output_path.exists() {
        fs::remove_file(output_path)
            .with_context(|| format!("removing old database {}", output_path.display()))?;
    }

    fs::rename(&temp_path, output_path).with_context(|| {
        format!(
            "replacing {} with generated database {}",
            output_path.display(),
            temp_path.display()
        )
    })?;

    Ok(())
}

fn temporary_database_path(output_path: &Path) -> Result<PathBuf> {
    let file_name = output_path
        .file_name()
        .context("SQLite output path must include a file name")?
        .to_string_lossy();

    Ok(output_path.with_file_name(format!(
        ".{file_name}.{}.tmp",
        process::id()
    )))
}

fn write_sqlite(export: &ExportData, path: &Path) -> Result<()> {
    let mut connection =
        Connection::open(path).with_context(|| format!("creating {}", path.display()))?;
    create_schema(&connection)?;
    let transaction = connection.transaction()?;

    {
        let mut statement =
            transaction.prepare("INSERT INTO categories (name, idx) VALUES (?1, ?2)")?;
        for group in export.categories.values() {
            for (name, index) in group {
                statement.execute(params![name, *index as i64])?;
            }
        }
    }

    {
        let mut statement = transaction.prepare(
            "INSERT INTO penalties (core_idx, part_idx, value) VALUES (?1, ?2, ?3)",
        )?;
        for (core_idx, row) in export.penalties.iter().enumerate() {
            for (part_idx, value) in row.iter().enumerate() {
                statement.execute(params![core_idx as i64, part_idx as i64, value])?;
            }
        }
    }

    {
        let cores = export.data.get("Cores").context("missing Cores")?;
        let mut statement = transaction.prepare(
            "INSERT INTO cores (name, category, damage, damage_end, fire_rate) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for core in cores {
            let (damage, damage_end) = extract_damage_pair(core.get("Damage"));
            statement.execute(params![
                string(core, "Name")?,
                string(core, "Category")?,
                damage,
                damage_end,
                value_f(core.get("Fire_Rate")),
            ])?;
        }
    }

    {
        let magazines = export
            .data
            .get("Magazines")
            .context("missing Magazines")?;
        let mut statement = transaction.prepare(
            "INSERT INTO magazines \
             (name, category, magazine_size, reload_time, damage_mod, fire_rate_mod) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        for magazine in magazines {
            statement.execute(params![
                string(magazine, "Name")?,
                string(magazine, "Category")?,
                value_f(magazine.get("Magazine_Size")),
                value_f(magazine.get("Reload_Time")),
                value_f(magazine.get("Damage")),
                value_f(magazine.get("Fire_Rate")),
            ])?;
        }
    }

    {
        let mut statement = transaction.prepare(
            "INSERT INTO parts \
             (part_type, name, category, damage_mod, fire_rate_mod) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for part_type in ["Barrels", "Grips", "Stocks"] {
            let parts = export
                .data
                .get(part_type)
                .with_context(|| format!("missing {part_type} part section"))?;

            for part in parts {
                statement.execute(params![
                    part_type,
                    string(part, "Name")?,
                    string(part, "Category")?,
                    value_f(part.get("Damage")),
                    value_f(part.get("Fire_Rate")),
                ])?;
            }
        }
    }

    transaction.commit()?;
    Ok(())
}

fn create_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r#"
CREATE TABLE categories (
    name TEXT PRIMARY KEY,
    idx INTEGER NOT NULL
);
CREATE TABLE penalties (
    core_idx INTEGER NOT NULL,
    part_idx INTEGER NOT NULL,
    value REAL NOT NULL,
    PRIMARY KEY (core_idx, part_idx)
);
CREATE TABLE cores (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    damage REAL NOT NULL,
    damage_end REAL NOT NULL,
    fire_rate REAL NOT NULL
);
CREATE TABLE magazines (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    magazine_size REAL NOT NULL,
    reload_time REAL NOT NULL,
    damage_mod REAL NOT NULL,
    fire_rate_mod REAL NOT NULL
);
CREATE TABLE parts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    part_type TEXT NOT NULL,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    damage_mod REAL NOT NULL,
    fire_rate_mod REAL NOT NULL
);
"#,
    )?;

    Ok(())
}

fn string<'a>(item: &'a Item, key: &str) -> Result<&'a str> {
    item.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("missing string field {key}"))
}

fn value_f(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(0.0)
}

fn extract_damage_pair(value: Option<&Value>) -> (f64, f64) {
    match value {
        Some(Value::Array(values)) if values.is_empty() => (0.0, 0.0),
        Some(Value::Array(values)) if values.len() == 1 => {
            let damage = value_f(values.first());
            (damage, damage)
        }
        Some(Value::Array(values)) => (value_f(values.first()), value_f(values.get(1))),
        Some(value) => {
            let damage = value.as_f64().unwrap_or(0.0);
            (damage, damage)
        }
        None => (0.0, 0.0),
    }
}

pub fn download_sheets() -> Result<()> {
    SheetDownloader::new(SHEET_ID, SHEET_FOLDER)?.download(&[
        SheetExport::new(CORES_SHEET_GID, CORES_SHEET),
        SheetExport::new(PARTS_V2_SHEET_GID, PARTS_V2_SHEET),
    ])
}

pub fn run() -> Result<()> {
    download_sheets()?;
    let data = build_full_data(Path::new(PARTS_V2_SHEET), Path::new(CORES_SHEET))?;
    save_sqlite(&data, Path::new(OUTPUT_FILE))
}
