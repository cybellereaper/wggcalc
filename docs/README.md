# Web calculator

`docs/` contains the static GitHub Pages frontend for WGG Calc. It performs the brute-force search entirely in the browser and recalculates automatically whenever a control changes.

## Architecture

- `src/app.js` owns UI state, validation, persistence, and rendering.
- `src/worker.js` runs calculations off the main thread so large searches do not freeze the interface.
- `src/engine.js` mirrors the optimized Rust ranking/search behavior and supports the same sort metrics and numeric filters.
- `data.json` is generated from the canonical `Data/FullData.sqlite3` database by the Rust `export_web_data` binary.

The generated JSON is intentionally not edited by hand. The scheduled data-refresh workflow regenerates both the SQLite source data and the browser dataset.

## Local preview

Generate the browser dataset, then serve the repository root:

```bash
cargo run --bin export_web_data
python3 -m http.server 8080
```

Open `http://localhost:8080/docs/`.

## Tests

```bash
node --test docs/tests/engine.test.mjs
node --check docs/src/engine.js
node --check docs/src/worker.js
node --check docs/src/app.js
```

These checks are also part of the repository CI workflow.

## GitHub Pages

Configure Pages to deploy from the default branch and `/docs` folder. The data-refresh workflow keeps `docs/data.json` synchronized with `Data/FullData.sqlite3`.
