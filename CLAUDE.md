# O-G-Exec

Oil refinery business sim. Rust core (sim strictly independent of UI), real
single-period LP, WASM web surface, victory at £500M valuation. Full design and LP
formulation: [docs/lp-formulation.md](docs/lp-formulation.md).

## Status
Phase 0 done: `refinery-lp/` — single-period refinery LP (ADU + FCC, 2 products),
solves in ~165µs, cross-compiles to wasm32.

## refinery-lp/
- `model.rs` — plain data description of the refinery (serde-ready, no solver types).
- `solve.rs` — flow-network LP via `microlp`; capacity shadow prices by perturb-and-resolve.
- `lib.rs` — the Phase 0 instance + economic-sanity tests. `main.rs` — spike runner.

## Conventions
- Keep the sim free of UI/rendering deps. Balancing lives in data, not code.
- `cargo test` must keep producing sane economics; specs must bind.
