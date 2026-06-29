# Refinery LP — Single-Period Formulation

The engine room. One LP, re-solved each tick. This document is the spec the Phase 0
spike must implement and the contract the rest of the sim consumes.

## 0. Modelling stance

- **Flow-network LP.** Nodes are *streams*; variables are *flows on arcs* between
  producers (units), consumers (units), blend pools (products), and sinks (sales,
  fuel, slop). This is the PIMS-style refinery LP and it extends cleanly from the
  Phase 0 two-unit toy to the full ADU/VDU/FCC/HC/reformer plant without restructuring.
- **Stay a true LP.** Two things threaten linearity; both are handled by construction:
  1. *Severity × throughput* is bilinear → modelled as **parallel linear operating
     modes** the LP blends feed across (§3).
  2. *Pool quality* is recursive/non-convex when intermediate pools have
     variable-quality feeds → avoided by keeping **every stream's quality a fixed
     datum** (assay- or mode-derived), so blend constraints are linear (§5).
- **Basis:** volume (bbl/day) for flows; quality on **linear blend indices** so all
  spec constraints are linear. Mass closure is enforced in mass; volume shrink/swell
  carried as per-stream factors. (Phase 0 may assume volume≈mass and ignore swell.)

## 1. Sets

| Symbol | Meaning | Phase 0 instance |
|---|---|---|
| `C` | crudes (assays) | 1 |
| `U` | process units | ADU, FCC |
| `S` | streams (cuts + unit products) | naphtha, gasoil, residue, fcc_gaso, lco, lpg |
| `P` | finished products | gasoline, diesel |
| `Q` | quality properties | octane, rvp, cetane, sulfur, density |
| `M_k` | operating modes of unit `k` | FCC: {low_sev, high_sev} |

## 2. Decision variables (all ≥ 0, bbl/day)

- `x_c` — crude `c` charged to ADU.
- `g_{k,m}` — feed processed by unit `k` in mode `m`.
- `r_{s→d}` — stream `s` routed to destination `d` (a unit-mode feed, a product pool,
  fuel, or sales sink).
- `b_{s→p}` — blendstock `s` blended into product `p` (a routing arc; named separately
  because the spec constraints live on it).
- Derived (not free): `f_s` produced of stream `s`; `u_k = Σ_m g_{k,m}` unit
  throughput; `P_p = Σ_s b_{s→p}` product volume.

## 3. Transformations (how streams are made)

**ADU** — charging crude `c` yields cut `s` at volumetric assay yield `a_{c,s}`:

```
produced_into(s)  +=  Σ_c  a_{c,s} · x_c
```

**FCC (and any conversion unit)** — each mode `m` is a *fixed linear recipe*. Feeding
`g_{FCC,m}` yields product stream `s'` at mode yield `y_{m,s'}`:

```
produced_into(s')  +=  Σ_m  y_{m,s'} · g_{FCC,m}
```

Severity is therefore **not a continuous coefficient multiplier** (which would be
bilinear); it is the *identity of the mode*. `low_sev` and `high_sev` are two recipes;
the LP chooses how much feed goes to each. The realised severity is the feed-weighted
average — a linear quantity:

```
σ_realised · u_FCC  =  Σ_m  σ_m · g_{FCC,m}
```

Coke and dry gas are yields too; coke is consumed internally (mass + heat), not sold.

## 4. Balance & capacity constraints

**Stream mass balance** (every stream node, production = disposition):

```
Σ_c a_{c,s} x_c  +  Σ_{k,m} y_{m,s} g_{k,m}     // made by ADU + conversion units
   =  Σ_d r_{s→d}  +  Σ_p b_{s→p}               // to unit feeds, sales, fuel, products
```

**Unit capacity** (degradation couples in *here* — effective cap scales with health):

```
u_k = Σ_m g_{k,m}  ≤  Cap_k · avail_k(health_k)
```

**Crude availability:**  `x_c ≤ inventory_c`  (or fixed to standing slate).

**Demand / contracts:**  `contract_p ≤ P_p ≤ demand_p`  (contract = floor the player
committed to; demand = market ceiling).

## 5. Product quality (the linear spec constraints)

Each property `q` blends on a **linear index** `idx_{s,q}` (octane via blend number,
RVP via RVP^1.25 index, sulfur/cetane/density ~linear). Because `P_p = Σ_s b_{s→p}`,
min and max specs are linear in `b`:

```
min-spec (octane, cetane):  Σ_s idx_{s,q} · b_{s→p}  ≥  spec^min_{p,q} · P_p
max-spec (sulfur, rvp):     Σ_s idx_{s,q} · b_{s→p}  ≤  spec^max_{p,q} · P_p
```

This stays LP **only because** each blendstock's `idx` is a fixed datum. If an
intermediate pool blended variable-quality feeds we'd be in NLP/pooling territory — we
don't; streams carry assay/mode quality straight to the final pool.

## 6. Objective — maximise daily contribution margin

```
max   Σ_p price_p · P_p                      // finished product revenue
    + Σ_s price_s · sales_s                  // intermediate sales (LPG, fuel oil, slurry)
    − Σ_c cost_c · x_c                       // crude at REPLACEMENT price (see note)
    − Σ_{k,m} opex_{k,m} · g_{k,m}           // variable opex; high_sev mode costs more
    − Σ_{k,m} maint_shadow_{k,m} · g_{k,m}   // optional: shadow cost of degradation
    + slider terms (§7)
```

**Crude costing note.** Within a tick the crude in the tank is sunk, but if the LP sees
it as free it will run garbage slate. So **cost crude at market replacement price** in
the objective; the Finance module does the inventory/COGS accounting separately.

## 7. Level-2 slider integration

Sliders enter as **objective weights or soft/bound constraints** — never as new
nonlinearities.

- **Severity dial** → bound on high-severity feed share: `g_{FCC,high} ≤ s_sev · u_FCC`,
  *and* raises `opex_{FCC,high}` and the degradation rate consumed by the reliability
  module. (Optionally a soft target with penalty `λ·|σ_realised − σ̄|`.)
- **Diesel-tilt slider** → objective bonus `τ · P_diesel` or a soft floor on the
  diesel/gasoline ratio. Nudges the slate without dictating flows.

The LP optimises *within* the envelope the player sets — exactly the Level-2 hybrid.

## 8. Outputs the LP returns to the rest of the sim

1. **Objective value** = daily contribution margin → Finance.
2. **All flows** → production, sales, cash, throughput-driven degradation.
3. **Dual values on capacity constraints** = marginal £/bbl·day of ADU/FCC capacity.
   This is gold: it is *the* number TEA needs to value a debottleneck, and it tells the
   player which unit is the binding constraint.
4. **Binding spec duals** = which quality (octane? sulfur?) is limiting the blend, and
   what relaxing it is worth.

Capture duals from the start — they are most of what makes the boardroom layer honest.

## 9. Phase 0 spike — concrete minimal instance to code

- **1 crude** → assay yields: naphtha 0.25, gasoil 0.45, residue 0.30.
- **ADU**: cap, opex/bbl, the yields above.
- **FCC**: feed = gasoil; two modes {low_sev, high_sev} with fixed yield vectors into
  {fcc_gaso, lco, lpg, coke}; high_sev = more gaso+lpg+coke, less lco, higher opex.
- **Products**: gasoline = naphtha + fcc_gaso (octane ≥ min, rvp ≤ max);
  diesel = lco + gasoil (cetane ≥ min, sulfur ≤ max).
- **Goal**: maximise margin; assert solve < 1 ms; print flows + capacity duals and
  sanity-check signs/magnitudes.

## 10. Solver choice (decide in the spike, because WASM)

The LP is tiny; solve time is a non-issue. The real constraint is the **WASM target**.

- **Recommended:** lead with a **pure-Rust LP solver** (`microlp`/`minilp`) behind a
  thin `trait Solver { solve(model) -> {obj, primal, duals} }`. Pure Rust → compiles to
  WASM with zero C++/emscripten toolchain pain, and it returns duals. Plenty for these
  sizes.
- **`good_lp` + HiGHS** is the battle-tested option for when the model grows, but HiGHS
  is C++ → adds a WASM build risk to verify. Keep it behind the same trait so it's a
  swap, not a rewrite.
- `clarabel` is conic — overkill for an LP and clumsier dual extraction. Skip.

**Action for Phase 0:** implement against the trait with the pure-Rust backend, and as
part of the spike confirm it cross-compiles to `wasm32-unknown-unknown`. That retires
both the formulation risk *and* the WASM-solver risk in week one.

## 11. Phase 0 result (built — `refinery-lp/`)

Done and passing. The §9 instance solves in **~165 µs** to a £416k/day margin with
ADU binding (100%) and FCC slack (56%).

- **Solver:** `microlp` 0.4 (pure Rust). Confirmed it cross-compiles to
  `wasm32-unknown-unknown`. → both Phase 0 risks retired.
- **Duals:** microlp exposes no dual accessor, so capacity shadow prices are recovered
  by **perturb-and-resolve** (`solve::capacity_shadow_prices`, Δ=100 bbl/d). Sub-ms
  solves make this cheap, it is solver-agnostic, and it answers TEA's question over a
  *finite* debottleneck step — arguably better than a marginal dual. Result is
  economically coherent: ADU £4.16/bbl·d (binding), FCC £0 (slack).
- **Data tuning learned the hard way:** the first instance was infeasible for diesel
  (sulfur-max forced ≤40% gasoil, cetane-min needed ≥80%) and had a negative crack
  spread → optimum was idle. The model was *correct to refuse*; the lesson is that
  spec windows and the crack spread are the balancing levers, and they are tight.
- **Not yet built (deferred to Phase 1):** serde/JSON load of `Refinery` (model is
  already a pure data description, so this is mechanical); the `Solver` trait extraction
  (solver is isolated to one function, `solve::solve_problem`, so it is a localized
  change when HiGHS becomes worthwhile); multi-crude; fuel-system energy balance;
  the maintenance shadow-cost term that lets the LP "feel" future degradation.
```
