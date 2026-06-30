//! Translate a [`Refinery`] into a flow-network LP and solve it (formulation §2–§6).
//!
//! Built against `microlp` (pure Rust → clean wasm32) through a small, explicit build
//! step. The solver is touched in exactly one place (`solve_problem`), so swapping in a
//! HiGHS-backed solver later is a localized change, not a rewrite.

use crate::model::{Refinery, SpecKind};
use microlp::{ComparisonOp, LinearExpr, OptimizationDirection, Problem, Variable};
use std::collections::BTreeMap;

/// Per-unit breakdown of how throughput was split across operating modes.
#[derive(Debug, Clone)]
pub struct UnitResult {
    pub name: String,
    pub throughput: f64,
    pub capacity: f64,
    pub realised_severity: f64,    // feed-weighted average; NaN if no conv modes
    pub per_mode: Vec<(String, f64)>,
}

#[derive(Debug, Clone)]
pub struct ProductResult {
    pub name: String,
    pub volume: f64,
    pub blend: Vec<(String, f64)>, // (stream name, bbl/day into this pool)
}

/// Truthful £/day financial breakdown of a solve, computed from the actual flows and
/// real prices (no tilt bonus). `margin == revenue() - crude_cost - opex` by
/// construction — the sim and UI consume this instead of reverse-engineering numbers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Finances {
    pub product_revenue: f64,   // finished-product sales
    pub byproduct_revenue: f64, // raw stream dispositions (LPG, fuel oil, naphtha)
    pub crude_cost: f64,        // crude charged × replacement price
    pub opex: f64,              // variable unit opex (ADU + conversions)
}

impl Finances {
    pub fn revenue(&self) -> f64 {
        self.product_revenue + self.byproduct_revenue
    }
    /// Contribution margin = total revenue − crude − variable opex.
    pub fn margin(&self) -> f64 {
        self.revenue() - self.crude_cost - self.opex
    }
}

#[derive(Debug, Clone)]
pub struct SolveResult {
    pub margin: f64, // £/day TRUE contribution margin (== finances.margin())
    pub finances: Finances,
    pub crude_charge: f64,
    pub adu: UnitResult,
    pub conversions: Vec<UnitResult>,
    pub products: Vec<ProductResult>,
    pub sales: Vec<(String, f64)>, // raw stream dispositions with value
}

/// Player operating policy (the Level-2 sliders), fed into the LP each tick.
/// Formulation §7: sliders enter as bounds / objective weights, never new
/// nonlinearities.
#[derive(Debug, Clone, Default)]
pub struct SolveOptions {
    /// Minimum feed-weighted average severity each conversion unit must run at.
    /// `None` = let the LP choose freely. The severity dial maps to this floor; the
    /// LP cannot duck to the cheap low-severity mode below it, and the extra opex of
    /// the high-severity recipe is paid in real margin (and degradation, in the sim).
    pub min_severity: Option<f64>,
    /// Additive £/bbl objective bonus per product, aligned to `Refinery::products`.
    /// The product-tilt slider sets this to steer the slate. It is a *preference*, not
    /// revenue: its contribution is subtracted back out of the reported margin so cash
    /// reflects true economics.
    pub product_bonus: Vec<f64>,
}

/// Accumulates a sparse linear expression as `var-id -> coefficient`, then materialises
/// it once against the created [`Variable`] handles. Avoids relying on duplicate-term
/// behaviour in the solver's expression builder.
fn build_expr(terms: &BTreeMap<usize, f64>, vars: &[Variable]) -> LinearExpr {
    let mut e = LinearExpr::empty();
    for (&id, &c) in terms {
        e.add(vars[id], c);
    }
    e
}

const INF: f64 = f64::INFINITY;

/// Build and solve the LP for `r`. Capacity overrides (unit name -> capacity) let the
/// shadow-price routine re-solve with a perturbed bottleneck without mutating the model.
fn solve_with(
    r: &Refinery,
    cap_override: &dyn Fn(&str) -> Option<f64>,
    opts: &SolveOptions,
) -> SolveResult {
    let mut p = Problem::new(OptimizationDirection::Maximize);
    let mut vars: Vec<Variable> = Vec::new();
    // Stream balance accumulators: stream idx -> (var-id -> signed coeff).
    // Production positive, consumption negative; constrained == 0.
    let mut bal: Vec<BTreeMap<usize, f64>> = vec![BTreeMap::new(); r.streams.len()];

    let add_var = |p: &mut Problem, vars: &mut Vec<Variable>, obj: f64| -> usize {
        let v = p.add_var(obj, (0.0, INF));
        vars.push(v);
        vars.len() - 1
    };

    // --- ADU charge variable -------------------------------------------------
    let adu_obj = -(r.adu.crude_price + r.adu.opex);
    let x_adu = add_var(&mut p, &mut vars, adu_obj);
    for &(s, yld) in &r.adu.yields {
        *bal[s].entry(x_adu).or_insert(0.0) += yld;
    }

    // --- Conversion mode variables ------------------------------------------
    // unit index -> Vec<(mode name, var-id)>
    let mut conv_vars: Vec<Vec<(String, usize)>> = Vec::new();
    for unit in &r.conversions {
        let mut modes = Vec::new();
        for m in &unit.modes {
            let id = add_var(&mut p, &mut vars, -m.opex);
            // consumes one bbl of feed per bbl processed
            *bal[unit.feed_stream].entry(id).or_insert(0.0) -= 1.0;
            for &(s, yld) in &m.yields {
                *bal[s].entry(id).or_insert(0.0) += yld;
            }
            modes.push((m.name.clone(), id));
        }
        conv_vars.push(modes);
    }

    // --- Blend variables (one per allowed stream per product) ----------------
    // product index -> Vec<(stream idx, var-id)>
    let mut blend_vars: Vec<Vec<(usize, usize)>> = Vec::new();
    for (pi, prod) in r.products.iter().enumerate() {
        let bonus = opts.product_bonus.get(pi).copied().unwrap_or(0.0);
        let mut bv = Vec::new();
        for &s in &prod.allowed {
            // objective = (real price + tilt preference bonus) * b
            let id = add_var(&mut p, &mut vars, prod.price + bonus);
            *bal[s].entry(id).or_insert(0.0) -= 1.0; // consumes the stream
            bv.push((s, id));
        }
        blend_vars.push(bv);
    }

    // --- Raw-disposition (sales/slop) variables, one per stream --------------
    let mut sale_vars: Vec<usize> = Vec::with_capacity(r.streams.len());
    for (s, stream) in r.streams.iter().enumerate() {
        let id = add_var(&mut p, &mut vars, stream.sale_price);
        *bal[s].entry(id).or_insert(0.0) -= 1.0;
        sale_vars.push(id);
    }

    // === Constraints =========================================================
    // Stream mass balance: production - disposition == 0.
    for terms in &bal {
        p.add_constraint(build_expr(terms, &vars), ComparisonOp::Eq, 0.0);
    }

    // ADU capacity.
    let adu_cap = cap_override(&r.adu.name).unwrap_or(r.adu.capacity);
    p.add_constraint(build_expr(&one(x_adu), &vars), ComparisonOp::Le, adu_cap);

    // Conversion-unit capacity: sum of mode feeds <= cap.
    for (ui, unit) in r.conversions.iter().enumerate() {
        let mut terms = BTreeMap::new();
        for &(_, id) in &conv_vars[ui] {
            terms.insert(id, 1.0);
        }
        let cap = cap_override(&unit.name).unwrap_or(unit.capacity);
        p.add_constraint(build_expr(&terms, &vars), ComparisonOp::Le, cap);
    }

    // Severity floor (the severity dial): feed-weighted avg severity >= min_severity.
    //   Σ σ_m g_m  >=  min_sev · Σ g_m   ⟺   Σ (σ_m - min_sev) g_m >= 0   (linear, §7).
    if let Some(min_sev) = opts.min_severity {
        for (ui, unit) in r.conversions.iter().enumerate() {
            let mut terms = BTreeMap::new();
            for ((_, id), m) in conv_vars[ui].iter().zip(&unit.modes) {
                terms.insert(*id, m.severity - min_sev);
            }
            p.add_constraint(build_expr(&terms, &vars), ComparisonOp::Ge, 0.0);
        }
    }

    // Product demand ceiling, contract floor, and quality specs.
    for (pi, prod) in r.products.iter().enumerate() {
        let mut vol = BTreeMap::new();
        for &(_, id) in &blend_vars[pi] {
            vol.insert(id, 1.0);
        }
        p.add_constraint(build_expr(&vol, &vars), ComparisonOp::Le, prod.demand);
        if prod.contract > 0.0 {
            p.add_constraint(build_expr(&vol, &vars), ComparisonOp::Ge, prod.contract);
        }
        for spec in &prod.specs {
            // sum (idx_q - limit) * b   {>=,<=} 0
            let mut terms = BTreeMap::new();
            for &(s, id) in &blend_vars[pi] {
                let q = r.streams[s].quality[spec.property];
                terms.insert(id, q - spec.limit);
            }
            let op = match spec.kind {
                SpecKind::Min => ComparisonOp::Ge,
                SpecKind::Max => ComparisonOp::Le,
            };
            p.add_constraint(build_expr(&terms, &vars), op, 0.0);
        }
    }

    // === Solve ===============================================================
    let sol = solve_problem(&p);

    // === Extract =============================================================
    let crude_charge = sol.var_value(vars[x_adu]);
    let mut var_opex = crude_charge * r.adu.opex; // ADU opex; conversion opex added below
    let adu = UnitResult {
        name: r.adu.name.clone(),
        throughput: crude_charge,
        capacity: adu_cap,
        realised_severity: f64::NAN,
        per_mode: Vec::new(),
    };

    let mut conversions = Vec::new();
    for (ui, unit) in r.conversions.iter().enumerate() {
        let mut per_mode = Vec::new();
        let mut feed = 0.0;
        let mut sev_acc = 0.0;
        for ((name, id), m) in conv_vars[ui].iter().zip(&unit.modes) {
            let v = sol.var_value(vars[*id]);
            feed += v;
            sev_acc += m.severity * v;
            var_opex += m.opex * v;
            per_mode.push((name.clone(), v));
        }
        conversions.push(UnitResult {
            name: unit.name.clone(),
            throughput: feed,
            capacity: cap_override(&unit.name).unwrap_or(unit.capacity),
            realised_severity: if feed > 1e-9 { sev_acc / feed } else { f64::NAN },
            per_mode,
        });
    }

    let mut products = Vec::new();
    let mut product_revenue = 0.0;
    let mut tilt_contribution = 0.0; // artificial bonus baked into the objective
    for (pi, prod) in r.products.iter().enumerate() {
        let mut blend = Vec::new();
        let mut volume = 0.0;
        for &(s, id) in &blend_vars[pi] {
            let v = sol.var_value(vars[id]);
            volume += v;
            if v > 1e-6 {
                blend.push((r.streams[s].name.clone(), v));
            }
        }
        product_revenue += volume * prod.price; // real price, never the tilt bonus
        tilt_contribution += opts.product_bonus.get(pi).copied().unwrap_or(0.0) * volume;
        products.push(ProductResult { name: prod.name.clone(), volume, blend });
    }

    let mut sales = Vec::new();
    let mut byproduct_revenue = 0.0;
    for (s, stream) in r.streams.iter().enumerate() {
        let v = sol.var_value(vars[sale_vars[s]]);
        byproduct_revenue += v * stream.sale_price;
        if v > 1e-6 && stream.sale_price > 0.0 {
            sales.push((stream.name.clone(), v));
        }
    }

    let finances = Finances {
        product_revenue,
        byproduct_revenue,
        crude_cost: crude_charge * r.adu.crude_price,
        opex: var_opex,
    };
    // The breakdown must reconcile with the LP objective (minus the tilt preference).
    debug_assert!(
        (finances.margin() - (sol.objective() - tilt_contribution)).abs() < 1e-3,
        "finances breakdown does not reconcile with LP objective"
    );

    SolveResult {
        margin: finances.margin(),
        finances,
        crude_charge,
        adu,
        conversions,
        products,
        sales,
    }
}

fn one(id: usize) -> BTreeMap<usize, f64> {
    let mut m = BTreeMap::new();
    m.insert(id, 1.0);
    m
}

/// The single point of contact with the LP backend.
fn solve_problem(p: &Problem) -> microlp::Solution {
    p.solve().expect("LP should be feasible and bounded")
}

/// Solve at the configured capacities with no operating policy (LP chooses freely).
pub fn solve(r: &Refinery) -> SolveResult {
    solve_with(r, &|_| None, &SolveOptions::default())
}

/// Solve under a player operating policy (severity floor, product tilt).
pub fn solve_opts(r: &Refinery, opts: &SolveOptions) -> SolveResult {
    solve_with(r, &|_| None, opts)
}

/// Marginal value of capacity per unit (£/day per bbl/day), recovered by perturb-and-
/// resolve over a finite step `delta`. This is the capacity shadow price the TEA layer
/// needs to value a debottleneck (formulation §8) — solver-agnostic, since microlp does
/// not expose duals directly.
pub fn capacity_shadow_prices(r: &Refinery, delta: f64) -> Vec<(String, f64)> {
    let base = solve(r).margin;
    let mut out = Vec::new();
    let mut units: Vec<(String, f64)> = vec![(r.adu.name.clone(), r.adu.capacity)];
    for u in &r.conversions {
        units.push((u.name.clone(), u.capacity));
    }
    for (name, cap) in units {
        let bumped = solve_with(
            r,
            &|n: &str| if n == name { Some(cap + delta) } else { None },
            &SolveOptions::default(),
        );
        out.push((name, (bumped.margin - base) / delta));
    }
    out
}
