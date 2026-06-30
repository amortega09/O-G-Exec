//! Tick engine. Advances the game by one week, applying player actions, updating
//! markets, solving the LP, and mutating the game state.

use crate::config::GameConfig;
use crate::equipment::UnitState;
use crate::market::{MarketState, Rng};
use crate::projects::ProjectState;
use crate::state::*;
use refinery_lp::model::Refinery;

/// Create a new game from a refinery config and game config.
pub fn new_game(refinery: Refinery, cfg: &GameConfig, seed: u64) -> GameState {
    let base_gasoline_demand = refinery
        .products
        .iter()
        .find(|p| p.name == "gasoline")
        .map(|p| p.demand)
        .unwrap_or(60_000.0);
    let base_diesel_demand = refinery
        .products
        .iter()
        .find(|p| p.name == "diesel")
        .map(|p| p.demand)
        .unwrap_or(60_000.0);

    let market = MarketState::new(&cfg.market, base_gasoline_demand, base_diesel_demand);

    // Create unit states: ADU first, then conversion units.
    let mut units = vec![UnitState::new(refinery.adu.name.clone())];
    for conv in &refinery.conversions {
        units.push(UnitState::new(conv.name.clone()));
    }

    GameState {
        week: 0,
        cash: cfg.starting_cash,
        valuation: 0.0,
        status: GameStatus::Running,
        player: PlayerSettings::default(),
        market,
        units,
        projects: Vec::new(),
        history: Vec::new(),
        events: Vec::new(),
        rng: Rng::new(seed),
        refinery: refinery.clone(),
        base_refinery: refinery,
        last_solve: None,
    }
}

/// £/bbl objective swing applied to a product at full (±1) tilt.
const TILT_STRENGTH: f64 = 8.0;

/// Translate the player's two operating sliders into LP [`SolveOptions`].
///
/// - **Severity** (0..1) maps to a feed-weighted-average severity *floor* between the
///   FCC's lowest and highest mode severities, so cranking it forces feed through the
///   high-severity recipe: more gasoline/LPG, but more opex now and faster degradation.
/// - **Product tilt** (-1..1) adds a ±`TILT_STRENGTH` £/bbl preference to diesel vs
///   gasoline, nudging the slate without dictating flows. It is removed from reported
///   margin inside the LP, so cash stays honest.
fn build_solve_options(state: &GameState) -> refinery_lp::solve::SolveOptions {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for conv in &state.refinery.conversions {
        for m in &conv.modes {
            lo = lo.min(m.severity);
            hi = hi.max(m.severity);
        }
    }
    let min_severity = if hi > lo {
        Some(lo + state.player.severity_target.clamp(0.0, 1.0) * (hi - lo))
    } else {
        None
    };

    let tilt = state.player.product_tilt.clamp(-1.0, 1.0);
    let product_bonus = state
        .refinery
        .products
        .iter()
        .map(|p| match p.name.as_str() {
            "diesel" => tilt * TILT_STRENGTH,
            "gasoline" => -tilt * TILT_STRENGTH,
            _ => 0.0,
        })
        .collect();

    refinery_lp::solve::SolveOptions { min_severity, product_bonus }
}

/// Advance the game by one tick (one week). Returns the updated GameView.
pub fn tick(state: &mut GameState, actions: &[PlayerAction], cfg: &GameConfig) -> GameView {
    if state.status != GameStatus::Running {
        return state.view(cfg);
    }

    state.week += 1;
    let mut week_events: Vec<GameEvent> = Vec::new();

    // --- 1. Apply player actions ---
    for action in actions {
        match action {
            PlayerAction::SetSeverity(s) => {
                state.player.severity_target = s.clamp(0.0, 1.0);
            }
            PlayerAction::SetProductTilt(t) => {
                state.player.product_tilt = t.clamp(-1.0, 1.0);
            }
            PlayerAction::ScheduleTurnaround(unit_name) => {
                if let Some(unit) = state.units.iter_mut().find(|u| u.unit_name == *unit_name) {
                    if let Some(ecfg) = cfg.equipment_for(unit_name) {
                        if let Some(cost) = unit.start_turnaround(ecfg) {
                            state.cash -= cost;
                            week_events.push(GameEvent {
                                week: state.week,
                                message: format!(
                                    "{} turnaround started (£{:.0}M, {} weeks)",
                                    unit_name,
                                    cost / 1_000_000.0,
                                    ecfg.turnaround_weeks
                                ),
                                severity: EventSeverity::Info,
                            });
                        }
                    }
                }
            }
            PlayerAction::ApproveProject(idx) => {
                if let Some(pcfg) = cfg.projects.get(*idx) {
                    // Check not already active.
                    let already_active = state.projects.iter().any(|p| p.config_index == *idx);
                    if !already_active && state.cash >= pcfg.cost {
                        state.cash -= pcfg.cost;
                        state.projects.push(ProjectState::from_config(pcfg, *idx));
                        week_events.push(GameEvent {
                            week: state.week,
                            message: format!(
                                "Project approved: {} (£{:.0}M, {} weeks)",
                                pcfg.name,
                                pcfg.cost / 1_000_000.0,
                                pcfg.duration_weeks
                            ),
                            severity: EventSeverity::Info,
                        });
                    }
                }
            }
        }
    }

    // --- 2. Update market prices ---
    let base_gasoline_demand = state
        .base_refinery
        .products
        .iter()
        .find(|p| p.name == "gasoline")
        .map(|p| p.demand)
        .unwrap_or(60_000.0);
    let base_diesel_demand = state
        .base_refinery
        .products
        .iter()
        .find(|p| p.name == "diesel")
        .map(|p| p.demand)
        .unwrap_or(60_000.0);

    state.market.step(
        state.week,
        &cfg.market,
        base_gasoline_demand,
        base_diesel_demand,
        &mut state.rng,
    );

    // --- 3. Patch refinery config with current market prices + effective capacities ---
    // Update crude price.
    state.refinery.adu.crude_price = state.market.crude_price;
    // Update product prices and demands.
    for prod in &mut state.refinery.products {
        match prod.name.as_str() {
            "gasoline" => {
                prod.price = state.market.gasoline_price;
                prod.demand = state.market.gasoline_demand;
            }
            "diesel" => {
                prod.price = state.market.diesel_price;
                prod.demand = state.market.diesel_demand;
            }
            _ => {}
        }
    }
    // Update effective capacities from equipment health.
    state.refinery.adu.capacity = state.base_refinery.adu.capacity * state.units[0].capacity_factor();
    for (i, conv) in state.refinery.conversions.iter_mut().enumerate() {
        conv.capacity = state.base_refinery.conversions[i].capacity
            * state.units[i + 1].capacity_factor();
    }

    // --- 4. Solve LP under the player's operating policy (Level-2 sliders) ---
    let opts = build_solve_options(state);
    let solve_result = refinery_lp::solve::solve_opts(&state.refinery, &opts);

    // --- 5. Update cash ---
    let daily_margin = solve_result.margin;
    let weekly_margin = daily_margin * 7.0;
    let net_margin = weekly_margin - cfg.fixed_opex_per_week;
    state.cash += net_margin;

    // --- 6. Degrade equipment ---
    // ADU
    if let Some(ecfg) = cfg.equipment_for(&state.units[0].unit_name) {
        let old_health = state.units[0].health;
        state.units[0].degrade(solve_result.crude_charge, 1.0, ecfg);
        // Check for ADU trip.
        if !state.units[0].is_running() && old_health > ecfg.trip_threshold {
            state.cash -= ecfg.trip_outage_cost;
            week_events.push(GameEvent {
                week: state.week,
                message: format!(
                    "⚠️ ADU TRIPPED! Unplanned outage ({} weeks, £{:.0}M)",
                    ecfg.trip_outage_weeks,
                    ecfg.trip_outage_cost / 1_000_000.0
                ),
                severity: EventSeverity::Critical,
            });
        }
    }
    // Conversion units
    for (i, conv_result) in solve_result.conversions.iter().enumerate() {
        let unit = &mut state.units[i + 1];
        if let Some(ecfg) = cfg.equipment_for(&unit.unit_name) {
            // Higher severity → faster degradation.
            let sev_factor = if conv_result.realised_severity.is_nan() {
                1.0
            } else {
                let base_sev = 0.6; // low_sev baseline
                1.0 + (conv_result.realised_severity - base_sev).max(0.0)
                    * ecfg.high_severity_degradation_mult
            };
            let old_health = unit.health;
            unit.degrade(conv_result.throughput, sev_factor, ecfg);
            // Check for trip event.
            if !unit.is_running() && old_health > ecfg.trip_threshold {
                state.cash -= ecfg.trip_outage_cost;
                week_events.push(GameEvent {
                    week: state.week,
                    message: format!(
                        "⚠️ {} TRIPPED! Unplanned outage ({} weeks, £{:.0}M)",
                        unit.unit_name,
                        ecfg.trip_outage_weeks,
                        ecfg.trip_outage_cost / 1_000_000.0
                    ),
                    severity: EventSeverity::Critical,
                });
            }
        }
    }

    // --- 7. Advance maintenance timers ---
    for unit in &mut state.units {
        if unit.tick_maintenance() {
            week_events.push(GameEvent {
                week: state.week,
                message: format!("{} back online — health restored", unit.unit_name),
                severity: EventSeverity::Info,
            });
        }
    }

    // --- 8. Advance capital projects ---
    let mut completed_projects = Vec::new();
    state.projects.retain_mut(|proj| {
        if let Some((unit_name, cap_gain)) = proj.tick() {
            completed_projects.push((proj.name.clone(), unit_name, cap_gain));
            false // remove completed project
        } else {
            true
        }
    });
    for (name, unit_name, cap_gain) in completed_projects {
        // Apply capacity gain to base refinery.
        if unit_name == state.base_refinery.adu.name {
            state.base_refinery.adu.capacity += cap_gain;
        } else {
            for conv in &mut state.base_refinery.conversions {
                if conv.name == unit_name {
                    conv.capacity += cap_gain;
                }
            }
        }
        week_events.push(GameEvent {
            week: state.week,
            message: format!(
                "🏗️ {} complete! {} capacity +{:.0} bbl/d",
                name, unit_name, cap_gain
            ),
            severity: EventSeverity::Info,
        });
    }

    // --- 9. Calculate valuation ---
    let lookback = cfg.valuation_lookback_weeks as usize;
    let trailing_margins: Vec<f64> = state
        .history
        .iter()
        .rev()
        .take(lookback)
        .map(|s| s.margin)
        .collect();
    let avg_weekly_margin = if trailing_margins.is_empty() {
        net_margin
    } else {
        trailing_margins.iter().sum::<f64>() / trailing_margins.len() as f64
    };
    state.valuation = avg_weekly_margin * 52.0 * cfg.valuation_multiple;

    // --- 10. Record snapshot ---
    let gasoline_vol = solve_result
        .products
        .iter()
        .find(|p| p.name == "gasoline")
        .map(|p| p.volume)
        .unwrap_or(0.0);
    let diesel_vol = solve_result
        .products
        .iter()
        .find(|p| p.name == "diesel")
        .map(|p| p.volume)
        .unwrap_or(0.0);

    state.history.push(WeekSnapshot {
        week: state.week,
        margin: net_margin,
        cash: state.cash,
        valuation: state.valuation,
        crude_price: state.market.crude_price,
        gasoline_price: state.market.gasoline_price,
        diesel_price: state.market.diesel_price,
        crude_charge: solve_result.crude_charge,
        gasoline_volume: gasoline_vol,
        diesel_volume: diesel_vol,
    });

    // --- 11. Check win/loss ---
    // Only crown a win once the trailing window is actually full, so a single strong
    // week can't end the game (the valuation is a trailing-average board review).
    let lookback_full = state.week >= cfg.valuation_lookback_weeks;
    if lookback_full && state.valuation >= cfg.victory_valuation {
        state.status = GameStatus::Won { week: state.week };
        week_events.push(GameEvent {
            week: state.week,
            message: format!(
                "🏆 Victory! Valuation reached £{:.0}M in week {}!",
                state.valuation / 1_000_000.0,
                state.week
            ),
            severity: EventSeverity::Info,
        });
    } else if state.cash < -50_000_000.0 {
        state.status = GameStatus::Lost {
            week: state.week,
            reason: "Bankruptcy — cash below -£50M".into(),
        };
        week_events.push(GameEvent {
            week: state.week,
            message: "💀 Bankrupt! Cash dropped below -£50M.".into(),
            severity: EventSeverity::Critical,
        });
    }

    state.events.extend(week_events);
    state.last_solve = Some(solve_result);

    state.view(cfg)
}
