# O-G-Exec

Oil refinery business simulation game. Rust core (sim strictly independent of UI),
real single-period LP, WASM web surface, victory at £500M valuation.

Full design and LP formulation: [docs/lp-formulation.md](docs/lp-formulation.md).

## Status
- **Phase 0** done: single-period refinery LP, solves in ~165µs, WASM-confirmed.
- **Phase 1** done: workspace restructure, serde on all model types, JSON scenario config.
- **Phase 2** done: game simulation engine — time, cash, markets, degradation, capital projects.
- **Phase 3** done: WASM bridge via wasm-bindgen.
- **Phase 4** done: browser dashboard UI (Vite + vanilla JS).

## Project Structure

```
O-G-Exec/
├── Cargo.toml                 # workspace root
├── crates/
│   ├── refinery-lp/           # LP solver + refinery model
│   ├── sim/                   # game engine (time, markets, degradation, projects)
│   └── wasm-bridge/           # wasm-bindgen glue layer
├── data/scenarios/            # JSON scenario configs (source of truth)
├── web/                       # browser frontend (Vite + vanilla JS)
│   ├── public/data/           # static copies of scenario/refinery JSON
│   └── src/                   # app.js, style.css, charts.js
└── docs/                      # design docs
```

## Quick Start

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Run tests
cargo test --workspace

# Build WASM
cargo install wasm-pack
wasm-pack build crates/wasm-bridge --target web --out-dir ../../web/pkg

# Run web dev server
cd web && npm install && npm run dev
```

## Conventions
- Keep the sim free of UI/rendering deps. Balancing lives in data (JSON), not code.
- `cargo test` must keep producing sane economics; specs must bind.
- All game-balance tunables live in `data/scenarios/*.json`.
- The sim crate's public API is `new_game()`, `tick()`, and `GameView`.
