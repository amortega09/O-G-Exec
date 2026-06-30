//! Tick engine. Advances the game by one week, applying player actions, updating
//! markets, solving the LP, and mutating the game state.

use crate::config::GameConfig;
use crate::equipment::UnitState;
use crate::market::MarketState;
use crate::projects::ProjectState;
use crate::rng::RngStreams;
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
        debt: 0.0,
        valuation: 0.0,
        status: GameStatus::Running,
        player: PlayerSettings::default(),
        market,
        units,
        projects: Vec::new(),
        history: Vec::new(),
        events: Vec::new(),
        rng: RngStreams::from_seed(seed),
        last_execution_efficiency: 1.0,
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
            PlayerAction::Borrow(amount) => {
                let drawn = crate::finance::draw(state.debt, *amount, &cfg.finance);
                if drawn > 0.0 {
                    state.debt += drawn;
                    state.cash += drawn;
                    week_events.push(GameEvent {
                        week: state.week,
                        message: format!("Drew £{:.0}M debt (balance £{:.0}M)",
                            drawn / 1_000_000.0, state.debt / 1_000_000.0),
                        severity: EventSeverity::Info,
                    });
                }
            }
            PlayerAction::Repay(amount) => {
                let paid = crate::finance::repay(state.debt, state.cash, *amount);
                if paid > 0.0 {
                    state.debt -= paid;
                    state.cash -= paid;
                    week_events.push(GameEvent {
                        week: state.week,
                        message: format!("Repaid £{:.0}M debt (balance £{:.0}M)",
                            paid / 1_000_000.0, state.debt / 1_000_000.0),
                        severity: EventSeverity::Info,
                    });
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
        &mut state.rng.market,
    );

    // Discrete fat-tail shocks (supply/demand/OPEC/refining) on their own RNG stream.
    for msg in state.market.roll_shocks(&cfg.market, &mut state.rng.events) {
        week_events.push(GameEvent {
            week: state.week,
            message: format!("📰 {msg}"),
            severity: EventSeverity::Warning,
        });
    }

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

    // --- 4. Solve LP (the optimal plan), then apply execution noise ---
    let opts = build_solve_options(state);
    let plan = refinery_lp::solve::solve_opts(&state.refinery, &opts);
    // Realized output falls short of plan by a random amount — the match-engine gap.
    // Scaling the whole solve keeps throughput, degradation, and the P&L consistent.
    let exec_efficiency = cfg.execution.draw_efficiency(&mut state.rng.execution);
    let solve_result = plan.scaled(exec_efficiency);
    state.last_execution_efficiency = exec_efficiency;
    if exec_efficiency < 0.92 {
        week_events.push(GameEvent {
            week: state.week,
            message: format!(
                "Operational shortfall — ran at {:.0}% of plan this week",
                exec_efficiency * 100.0
            ),
            severity: EventSeverity::Warning,
        });
    }

    // --- 5. Update cash ---
    // Operating margin (EBITDA): banked before interest. Used for valuation.
    let daily_margin = solve_result.margin;
    let weekly_margin = daily_margin * 7.0;
    let net_margin = weekly_margin - cfg.fixed_opex_per_week;
    state.cash += net_margin;
    // Interest on debt sits below EBITDA — a financing cost, not an operating one.
    let interest = crate::finance::weekly_interest(state.debt, &cfg.finance);
    state.cash -= interest;

    // --- 6. Degrade equipment, then roll the stochastic outage hazard ---
    // Unit 0 is the ADU (severity factor 1.0); the rest are conversion units whose
    // severity factor rises with how hard the player runs them.
    for ui in 0..state.units.len() {
        let unit_name = state.units[ui].unit_name.clone();
        let ecfg = match cfg.equipment_for(&unit_name) {
            Some(e) => e,
            None => continue,
        };

        let (throughput, sev_factor) = if ui == 0 {
            (solve_result.crude_charge, 1.0)
        } else if let Some(cr) = solve_result.conversions.get(ui - 1) {
            let sf = if cr.realised_severity.is_nan() {
                1.0
            } else {
                let base_sev = 0.6; // low_sev baseline
                1.0 + (cr.realised_severity - base_sev).max(0.0)
                    * ecfg.high_severity_degradation_mult
            };
            (cr.throughput, sf)
        } else {
            (0.0, 1.0)
        };

        // Degrade, then compute + store this week's outage probability.
        let hazard = {
            let unit = &mut state.units[ui];
            unit.degrade(throughput, sev_factor, ecfg);
            let h = unit.outage_hazard(sev_factor, ecfg);
            unit.last_hazard = h;
            h
        };

        // Roll the gamble on its own RNG stream.
        if state.units[ui].is_running() && state.rng.reliability.chance(hazard) {
            state.units[ui].trip(ecfg);
            state.cash -= ecfg.trip_outage_cost;
            week_events.push(GameEvent {
                week: state.week,
                message: format!(
                    "⚠️ {} TRIPPED! Unplanned outage ({} weeks, £{:.0}M)",
                    unit_name,
                    ecfg.trip_outage_weeks,
                    ecfg.trip_outage_cost / 1_000_000.0
                ),
                severity: EventSeverity::Critical,
            });
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
    // Enterprise value of operations (floored at 0 — a loss-making plant isn't worth a
    // negative sale price). This is the win metric: you grow it by growing EBITDA, not
    // by hoarding cash. Cash/debt govern survival and fund growth, but don't count here.
    state.valuation = (avg_weekly_margin * 52.0 * cfg.valuation_multiple).max(0.0);

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
        interest,
        debt: state.debt,
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
    } else if state.cash < cfg.finance.bankruptcy_cash_floor {
        state.status = GameStatus::Lost {
            week: state.week,
            reason: format!(
                "Insolvent — cash £{:.0}M below the £{:.0}M floor",
                state.cash / 1_000_000.0,
                cfg.finance.bankruptcy_cash_floor / 1_000_000.0
            ),
        };
        week_events.push(GameEvent {
            week: state.week,
            message: "💀 Insolvent! Out of cash and borrowing capacity.".into(),
            severity: EventSeverity::Critical,
        });
    }

    state.events.extend(week_events);
    state.last_solve = Some(solve_result);

    state.view(cfg)
}
