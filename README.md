# WeirdGunGameCalc (Rust)

High-performance brute-force calculator for the Roblox game **Weird Gun Game**, with both a Rust CLI and a real-time browser interface.

## Build

```bash
cargo build --release
```

Executable output:

```bash
./target/release/wggcalc --help
```

## Run examples

```bash
./target/release/wggcalc --top 10 --sort ttk --include AR,SMG
./target/release/wggcalc --sort dps --priority highest --dps-min 100
./target/release/wggcalc --ttk-max 0.25 --mh 100
```

## Supported flags

- `--data <path>` path to `FullData.sqlite3` (legacy JSON is also supported)
- `--top <n>` number of returned builds
- `--mh <health>` max player health
- `--sort <ttk|dps|damage|damageend|firerate|magazine>`
- `--priority <highest|lowest|auto>`
- `--include <cat1,cat2,...>` include weapon categories
- `--part-pool <n>` candidate parts per type per core
- `--damage-min`, `--damage-max`
- `--damage-end-min`, `--damage-end-max`
- `--ttk-min`, `--ttk-max` (seconds)
- `--dps-min`, `--dps-max`
- `--metrics`

## Real-time web calculator

The static site in `docs/` recalculates builds automatically as settings change. Computation runs in a Web Worker, keeping the page responsive even while evaluating large candidate sets.

Generate its browser-readable dataset from the canonical SQLite database:

```bash
cargo run --bin export_web_data
```

Preview locally:

```bash
python3 -m http.server 8080
```

Then open `http://localhost:8080/docs/`.

The scheduled data workflow refreshes `Data/FullData.sqlite3` and regenerates `docs/data.json`, so the CLI and website consume the same source data.

## Test

```bash
cargo test
node --test docs/tests/engine.test.mjs
```

CI additionally enforces `rustfmt`, Clippy with warnings denied, browser-module syntax checks, a web-data export smoke test, release builds, and a real-dataset CLI smoke test.

## Regenerating sheet data

```bash
cargo run --release --bin parse_sheet
cargo run --release --bin export_web_data
```

The Rust data utility downloads the Google Sheet CSV exports and recreates `Data/FullData.sqlite3`. The web exporter then produces the compact `docs/data.json` used by GitHub Pages.
