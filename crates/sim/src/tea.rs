//! Techno-economic assessment (the boardroom). For each capital project, estimate its
//! incremental margin by re-running the LP with the project applied on a *forecast* price
//! deck (long-run means — you don't appraise a 15-year asset on today's spot), then build
//! a cash-flow series and compute NPV / IRR / payback. Decision-support, not a blank
//! spreadsheet: the game computes the numbers; the player makes the allocation call.

use crate::config::{GameConfig, ProjectConfig};
use crate::state::GameState;
use refinery_lp::model::Refinery;
use refinery_lp::solve::solve;

const OPERATING_DAYS: f64 = 350.0; // days/yr a unit actually earns (allowing downtime)
const PROJECT_LIFE_YEARS: usize = 15;
const DISCOUNT_RATE: f64 = 0.10; // WACC used for NPV

/// A project's economic appraisal at forecast prices.
#[derive(Clone, Debug)]
pub struct ProjectAppraisal {
    pub config_index: usize,
    pub incremental_annual_margin: f64, // £/yr the project is estimated to add
    pub npv: f64,                       // £, at DISCOUNT_RATE
    pub irr: f64,                       // annual fraction; NaN if not computable
    pub payback_years: f64,             // incl. build time; INFINITY if it never pays back
}

/// The refinery at long-run mean prices — the forecast deck for capex appraisal.
fn forecast_refinery(state: &GameState, cfg: &GameConfig) -> Refinery {
    let mut r = state.base_refinery.clone();
    let m = &cfg.market;
    for crude in &mut r.crudes {
        crude.price = m.crude_mean + crude.differential;
    }
    for p in &mut r.products {
        match p.name.as_str() {
            "gasoline" => p.price = m.crude_mean + m.gasoline_spread_mean,
            "diesel" => p.price = m.crude_mean + m.diesel_spread_mean,
            _ => {}
        }
    }
    r
}

/// Apply a project's effect to a refinery (same effect it has on completion): add its
/// capacity gain to the named unit.
fn apply_project(r: &mut Refinery, pcfg: &ProjectConfig) {
    if pcfg.unit_name == r.adu.name {
        r.adu.capacity += pcfg.capacity_gain;
    } else {
        for c in &mut r.conversions {
            if c.name == pcfg.unit_name {
                c.capacity += pcfg.capacity_gain;
            }
        }
    }
}

/// NPV of a project: capex at t=0, then incremental annual margin over its life, starting
/// after the build finishes.
fn npv(capex: f64, annual: f64, build_years: f64, life: usize, r: f64) -> f64 {
    let mut v = -capex;
    for k in 1..=life {
        let t = build_years + k as f64; // cash at the end of each operating year
        v += annual / (1.0 + r).powf(t);
    }
    v
}

/// IRR by bisection — the discount rate at which NPV = 0.
fn irr(capex: f64, annual: f64, build_years: f64, life: usize) -> f64 {
    if annual <= 0.0 {
        return f64::NAN;
    }
    let (mut lo, mut hi) = (-0.9f64, 5.0f64);
    if npv(capex, annual, build_years, life, lo) < 0.0 {
        return f64::NAN; // underwater even at a near-zero discount rate
    }
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if npv(capex, annual, build_years, life, mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Appraise every project that is currently available and not already under way.
pub fn appraise(state: &GameState, cfg: &GameConfig) -> Vec<ProjectAppraisal> {
    // Nothing on offer → skip the (otherwise per-tick) LP re-runs entirely.
    let any = cfg.projects.iter().enumerate().any(|(i, p)| {
        p.available_after_week <= state.week && !state.projects.iter().any(|a| a.config_index == i)
    });
    if !any {
        return Vec::new();
    }
    let base = forecast_refinery(state, cfg);
    let base_margin = solve(&base).margin; // daily, at forecast prices
    let mut out = Vec::new();
    for (i, pcfg) in cfg.projects.iter().enumerate() {
        if pcfg.available_after_week > state.week {
            continue;
        }
        if state.projects.iter().any(|p| p.config_index == i) {
            continue;
        }
        let mut with = base.clone();
        apply_project(&mut with, pcfg);
        let inc_daily = (solve(&with).margin - base_margin).max(0.0);
        let annual = inc_daily * OPERATING_DAYS;
        let build_years = pcfg.duration_weeks as f64 / 52.0;
        let payback = if annual > 0.0 {
            build_years + pcfg.cost / annual
        } else {
            f64::INFINITY
        };
        out.push(ProjectAppraisal {
            config_index: i,
            incremental_annual_margin: annual,
            npv: npv(pcfg.cost, annual, build_years, PROJECT_LIFE_YEARS, DISCOUNT_RATE),
            irr: irr(pcfg.cost, annual, build_years, PROJECT_LIFE_YEARS),
            payback_years: payback,
        });
    }
    out
}
