# O-G-Exec — Roadmap

Forward plan, aligned to [vision.md](vision.md). "You are here" + what's next.
LP technical spec lives in [lp-formulation.md](lp-formulation.md).

## Done

- **Phase 0** — refinery LP spike (ADU + FCC, 2 products), ~165µs, wasm-confirmed.
- **Phase 1** — workspace, serde on all model types, JSON-driven config.
- **Phase 2** — sim engine: weekly tick, market (OU crack spreads), degradation,
  capital projects, valuation.
- **Phase 3** — wasm-bridge (`Game::new/tick/view`).
- **Phase 4** — browser command-center UI (schematic, telemetry, charts).
- **Phase A** — truthful P&L (LP `Finances` reconciles to cash), sliders wired into the
  LP (utilization↔reliability live), win gated on full lookback, data deduplicated.
- **Phase B** — debt financing (borrow/repay/interest/insolvency); valuation =
  enterprise value (the win metric), equity shown separately.

The core loop is a real game: do-nothing fails, maintenance sustains, borrow-to-build
wins faster. Honest economics, deterministic per seed. Tests: refinery-lp 5, sim 11.

## Next — building the world (per vision §Build priority)

### Phase C — event-driven stochastic spine  ← IN PROGRESS
The architectural pivot from deterministic calculator to FM-style simulation.
- [x] **Split seeded RNG** ([rng.rs](../crates/sim/src/rng.rs)) — one master seed,
  independent streams per subsystem (market, reliability, execution, events).
- [x] **Stochastic outage hazard** ([equipment.rs](../crates/sim/src/equipment.rs)) —
  trip probability rises non-linearly as health falls and with how hard you run; the
  deterministic threshold is gone. `outage_risk` surfaced per unit in the UI.
- [x] **Execution noise** ([config.rs](../crates/sim/src/config.rs) `ExecutionConfig`,
  [solve.rs](../crates/refinery-lp/src/solve.rs) `SolveResult::scaled`): realized output
  = LP plan × a per-week efficiency factor (≤ 1, its own RNG stream). Scales the whole
  solve incl. the `Finances` breakdown, so throughput/degradation/P&L stay consistent and
  reconcile to cash. Surfaced as "Execution vs Plan %".
- [ ] **Typed event queue** alongside the tick pipeline: physics emits events; entities
  react and emit more; some surface as player choices. *Defer until there are entities
  (people/competitors) that need to react — don't build the bus before there's traffic.*
- [ ] Enabler: expose the LP's **real solved flows** through `GameView` (the schematic
  currently re-derives them in JS — see [app.js](../web/src/app.js) `renderSchematic`).

### Phase D — multi-crude procurement  ← IN PROGRESS
- [x] **Multi-crude LP** — crudes are first-class assays ([model.rs](../crates/refinery-lp/src/model.rs)
  `Crude`); the ADU blends across grades and the LP picks the mix. Each grade is priced
  off the benchmark + a grade differential each tick. Two grades shipped (Brent Light,
  Urals Heavy); `crude_mix`/`crude_prices` surfaced in the UI market panel.
- [x] **Hydrocracker capital project** — a dormant Hydrocracker unit (capacity 0) that
  the "Build Hydrocracker" project (£80M, 40 wk) switches on. It upgrades low-value
  residue into high-cetane diesel, lifting margin ~£2.4M/wk (~33-wk payback) and flipping
  crude selection to ~96% heavy. This is the design doc's "£80M bet on the heavy-light
  spread": committing capital + debt over a long build, on an ROI that rides future
  diesel cracks. Turns the crude/capex question into a genuine decision.
- [ ] **Still owed for full depth:** crude *sulfur → product-spec* coupling (needs
  crude-specific streams; watch the FCC-laundering loophole) so sour crude has a
  processing downside even before upgrading.
- [ ] Explicit procurement (approve slate / buy cargoes with lead time) + a moving
  heavy-light differential.

### Phase E — living market
- [x] **Realistic market + shock events** — OU params recalibrated to historical Brent
  (~£66 mean, £20–134 range, ~40% vol) and crack spreads; discrete fat-tail shocks
  (supply/demand/OPEC/refining) sized from real events. Full data backing in
  [market-calibration.md](market-calibration.md). Valuation moved to a 52-week lookback
  so wins need *sustained* earnings, not a volatility spike.
- [ ] Crude suppliers + product buyers + spreads that respond to supply/demand
  (including the player's and rivals' output).

> **Owed: a strategy-depth balance pass.** The realistic market exposed that the player's
> levers are too weak against market swings — maintenance barely matters (degradation too
> slow) and capex timing is crude, so outcomes are market-luck-dominated. Make maintenance
> bite, add rate-cutting (vs binary idle), and reward counter-cyclical investment so skill
> tilts the odds more than luck. Best done with multi-crude (Phase D), which adds the
> decisions the market should reward.

### Phase F — competitors
Other firms running plants on the same LP engine; their moves perturb the market.

### Phase G — M&A / asset market
Buy and sell refineries; the LP values targets; due diligence = running their LP.

### Phase H — people + board
Staff with attributes (incl. **planning capability** = LP solve quality, per vision),
hire/fire/morale; a Board that reacts to performance and controls your mandate.

TEA (NPV/IRR/payback) threads through D–H as the decision-support that makes each bet
legible.

## The reusable primitive (D onward)

Most world features are the **same Entity**: hidden true attributes + a noisy player
estimate + observation that narrows it. Staff, competitors, and acquisition targets are
all this object in different clothes; "scouting" = inspection / assay / due diligence.

## Standing gate

Phase 1.5 "is it fun?" is continuous, not a milestone: after each phase, can a human
agonise over a real decision in the web harness? If not, fix the design, not the UI.
