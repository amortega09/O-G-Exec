# O-G-Exec — Vision & North Star

Read this first. Every feature should be checkable against it: *does this serve the
aim, and does it sit in the right layer?*

## The aim

> **An oil-refinery business simulator where you are the owner/operator growing a
> company inside a living global oil market — and underneath it, a real LP gives every
> refinery (yours, your rivals', your acquisition targets') authentic economics, so the
> world behaves like the real industry instead of faking it.**

Victory: build the business to a **£500M valuation**. The fantasy is *tycoon/exec*, not
*plant operator* — though operating depth is available to those who want it.

## The core principle: the LP is the world's economic truth engine

The linear program is **not the game** — in the real industry it's the planning tool
operators run to pick the crude slate and run plan. In our game it is the **economic
physics of the world**: the thing that makes a simulated refinery behave like a real one.

- It values **acquisition targets** (due diligence = running their LP on a price deck).
- It runs **competitors'** plants, so their behaviour and market clearing are real.
- It prices the consequences of **your** commercial and strategic decisions.

This is our moat. Most tycoon games fake consequences with a formula; we have a real
economic engine. In Football Manager terms, **the LP is the match engine** — the
deterministic kernel that makes the world credible. The player does not have to operate
it for it to be the foundation of everything. **The LP is good enough; build the world
that uses it.**

## The five layers (and where the player lives)

A refinery business stacks like this:

1. **Process / chemistry** — units, yields, specs · *the LP encodes this*
2. **Operational planning** — crude slate, run plan, blending · *the LP does this*
3. **Asset management** — reliability, maintenance, capex
4. **Commercial** — crude buying, product offtake, contracts, hedging, trading
5. **Corporate / strategic** — financing, M&A, markets, board, people

**The player lives in layers 3–5** (the tycoon fantasy). Layers 1–2 are the realistic
substrate the LP provides. Operations (layer 2) are exposed as an **optional depth dial**
(the Level-2 sliders) — engage if you want, ignore if you don't.

FM parallel: the manager plays tactics + transfers + man-management (3–5); the match
engine runs the physics (1–2).

## The mechanic that keeps the LP honest *and* gives the player agency

If the LP always solves *optimally*, the player adds no value at the operational layer.
So the LP must **not be omniscient**:

- It optimises on a **forecast price deck that can be wrong** (you bet on the future).
- Its quality is a **planning capability you invest in** — better planners / software =
  a better solve, tighter forecasts, less left on the table.

This makes planning quality a **hireable, upgradeable asset** (the people layer and the
uncertainty layer, arriving together) and justifies the LP's centrality without forcing
the player to micromanage it.

## What this is NOT (scope guards)

- **Not** a plant-operations micro-sim. We don't simulate valves, controllers, or
  utilities line-items unless they create a *decision*.
- **Not** realism for its own sake. Add realism **only where it creates a decision under
  uncertainty** (FM is realistic where it matters, abstract everywhere else).
- **Not** breadth before depth. One tense core loop with a person agonising over a
  decision beats five shallow systems.

## Build priority (what serves the aim next)

The world is the gap, not the LP. In order:

1. **Event-driven stochastic spine** — event queue + split seeded RNG + probabilistic
   outage hazard + plan-vs-actual execution noise. The architectural unlock for
   everything FM-flavoured.
2. **Multi-crude procurement** — buy assays by price/yield/sulfur. The central *real*
   refinery decision; the LP already supports it structurally.
3. **A living market** — crude suppliers, product buyers, crack spreads that respond to
   supply/demand (including yours and rivals').
4. **Competitors** — other firms running plants on the same engine.
5. **M&A / asset market** — buy and sell refineries; the LP values them.
6. **People + Board** — staff (incl. planning capability) and a board that reacts to
   performance, controls your mandate, and can fire you.

TEA (NPV/IRR/payback decision-support) threads through 2–6 as the layer that makes every
bet legible.

## The non-negotiable gate

Before any polished client work: a human must make real decisions in the ugly web
harness and **agonise over them**. If it's boring there, fix the design (tighten the
couplings, sharpen the failure modes) — not the graphics.
