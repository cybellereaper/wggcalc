# WeirdGunGameCalc (Rust)

Rust rewrite of the Weird Gun Game bruteforce calculator.

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

## Test

```bash
cargo test
```

## Regenerating sheet data

```bash
cargo run --release --bin parse_sheet
```

The Rust data utility downloads the same Google Sheet CSV exports and recreates `Data/FullData.sqlite3` with the schema used by the calculator.

## Web app

The browser app under `docs/` is independent JavaScript and remains unchanged. The calculator, data parser/generator, tests, build commands, and scheduled data-refresh workflow no longer require Crystal or Shards.
