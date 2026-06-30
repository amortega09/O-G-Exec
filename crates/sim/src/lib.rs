//! O-G-Exec simulation engine. Wraps the refinery LP solver in a game loop
//! with time progression, market dynamics, equipment degradation, and capital
//! projects.
//!
//! This crate is UI-independent — it produces serializable state that the
//! WASM bridge passes to the browser frontend.

pub mod config;
pub mod equipment;
pub mod finance;
pub mod market;
pub mod rng;
pub mod projects;
pub mod state;
pub mod tick;

// Re-export key types for convenience.
pub use config::GameConfig;
pub use state::{GameState, GameStatus, GameView, PlayerAction};
pub use tick::{new_game, tick};

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> GameConfig {
        serde_json::from_str(include_str!("../../../data/scenarios/tutorial.json"))
            .expect("tutorial.json should parse")
    }

    fn test_refinery() -> refinery_lp::model::Refinery {
        refinery_lp::phase0_refinery()
    }

    #[test]
    fn new_game_starts_correctly() {
        let cfg = test_config();
        let r = test_refinery();
        let state = new_game(r, &cfg, 42);
        assert_eq!(state.week, 0);
        assert_eq!(state.cash, cfg.starting_cash);
        assert_eq!(state.status, GameStatus::Running);
        assert_eq!(state.units.len(), 2); // ADU + FCC
    }

    #[test]
    fn tick_advances_week() {
        let cfg = test_config();
        let r = test_refinery();
        let mut state = new_game(r, &cfg, 42);
        let view = tick(&mut state, &[], &cfg);
        assert_eq!(view.week, 1);
        assert!(view.weekly_margin > 0.0, "should have positive margin");
        assert!(state.cash > cfg.starting_cash, "cash should grow on positive margin");
    }

    #[test]
    fn view_pnl_reconciles_with_cash_movement() {
        // The P&L the player reads must equal the cash the bank actually moves:
        // revenue − crude − variable_opex − fixed_opex == Δcash for the week.
        let cfg = test_config();
        let mut s = new_game(test_refinery(), &cfg, 42);
        let cash_before = s.cash;
        let v = tick(&mut s, &[], &cfg);
        let delta = s.cash - cash_before;
        let pnl = v.revenue - v.crude_cost - v.variable_opex - v.fixed_opex;
        assert!(
            (pnl - delta).abs() < 1.0,
            "P&L {pnl} must reconcile with cash delta {delta}"
        );
        assert!(
            (v.weekly_margin - delta).abs() < 1.0,
            "weekly_margin {} must equal cash delta {delta}",
            v.weekly_margin
        );
    }

    #[test]
    fn borrow_increases_cash_and_debt_then_interest_accrues() {
        let cfg = test_config();
        let mut s = new_game(test_refinery(), &cfg, 42);
        let cash0 = s.cash;
        tick(&mut s, &[PlayerAction::Borrow(50_000_000.0)], &cfg);
        assert!((s.debt - 50_000_000.0).abs() < 1.0, "debt should be 50M");
        // Cash went up by the draw, then down by operating result + one week interest.
        let interest = cfg.finance.weekly_rate() * 50_000_000.0;
        assert!(s.cash > cash0, "drawing debt should raise cash on net this tick");
        // Repay knocks debt back down.
        tick(&mut s, &[PlayerAction::Repay(20_000_000.0)], &cfg);
        assert!((s.debt - 30_000_000.0).abs() < 1.0, "debt should be 30M after repay");
        assert!(interest > 0.0);
    }

    #[test]
    fn cannot_borrow_beyond_the_limit() {
        let cfg = test_config();
        let mut s = new_game(test_refinery(), &cfg, 42);
        tick(&mut s, &[PlayerAction::Borrow(1_000_000_000.0)], &cfg);
        assert!(
            s.debt <= cfg.finance.max_debt + 1.0,
            "debt {} must not exceed max {}",
            s.debt,
            cfg.finance.max_debt
        );
    }

    #[test]
    fn valuation_is_enterprise_value_not_cash_hoard() {
        // Equity counts cash; the win metric (valuation = enterprise value) does not —
        // so banking cash must not move valuation on its own.
        let cfg = test_config();
        let mut s = new_game(test_refinery(), &cfg, 42);
        for _ in 0..15 {
            tick(&mut s, &[], &cfg);
        }
        let v = s.view(&cfg);
        // equity = EV + cash − debt, and with positive cash equity exceeds EV.
        assert!((v.equity - (v.enterprise_value + s.cash - s.debt)).abs() < 1.0);
        assert!(v.enterprise_value <= v.equity + 1.0);
    }

    #[test]
    fn execution_efficiency_in_range_and_varies() {
        // Realized efficiency must stay in [min, 1] (never beats plan) and actually vary.
        let cfg = test_config();
        let mut s = new_game(test_refinery(), &cfg, 42);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            let v = tick(&mut s, &[], &cfg);
            assert!(
                v.execution_efficiency >= cfg.execution.min_efficiency - 1e-9
                    && v.execution_efficiency <= 1.0 + 1e-9,
                "efficiency out of range: {}",
                v.execution_efficiency
            );
            seen.insert((v.execution_efficiency * 1000.0) as i64);
        }
        assert!(seen.len() > 5, "execution efficiency should vary week to week");
    }

    #[test]
    fn outage_timing_is_stochastic_across_seeds() {
        // The reliability gamble must actually be random: different seeds should produce
        // different first-trip timing (whereas a given seed stays deterministic — see
        // `deterministic_replay`).
        use equipment::MaintenanceStatus::Tripped;
        let cfg = test_config();
        let mut first_trip = Vec::new();
        for seed in 0..40u64 {
            let mut s = new_game(test_refinery(), &cfg, seed);
            let mut wk_tripped = None;
            for wk in 1..=250u32 {
                tick(&mut s, &[PlayerAction::SetSeverity(0.6)], &cfg);
                if s.units.iter().any(|u| matches!(u.maintenance, Tripped { .. })) {
                    wk_tripped = Some(wk);
                    break;
                }
            }
            first_trip.push(wk_tripped);
        }
        let distinct: std::collections::HashSet<_> = first_trip.iter().collect();
        assert!(
            distinct.len() > 3,
            "trip timing should vary across seeds, got {distinct:?}"
        );
    }

    #[test]
    fn do_nothing_does_not_instantly_win() {
        // Regression: the win can't fire before the trailing window is even full, so a
        // strong opening market can't end the game in the first few weeks.
        let cfg = test_config();
        let mut s = new_game(test_refinery(), &cfg, 42);
        for _ in 0..8 {
            tick(&mut s, &[], &cfg);
        }
        assert_eq!(
            s.status,
            GameStatus::Running,
            "should not win within the first few weeks"
        );
    }

    #[test]
    fn severity_shifts_slate_and_degrades_faster() {
        // The severity slider must actually reach the LP: higher severity -> more
        // gasoline now, but faster FCC degradation (the core utilization↔reliability tension).
        let cfg = test_config();
        let mut lo = new_game(test_refinery(), &cfg, 7);
        let mut hi = new_game(test_refinery(), &cfg, 7);
        tick(&mut lo, &[PlayerAction::SetSeverity(0.0)], &cfg);
        tick(&mut hi, &[PlayerAction::SetSeverity(1.0)], &cfg);
        assert!(
            hi.history[0].gasoline_volume > lo.history[0].gasoline_volume,
            "high severity should yield more gasoline: {} vs {}",
            hi.history[0].gasoline_volume,
            lo.history[0].gasoline_volume
        );
        for _ in 0..10 {
            tick(&mut lo, &[PlayerAction::SetSeverity(0.0)], &cfg);
            tick(&mut hi, &[PlayerAction::SetSeverity(1.0)], &cfg);
        }
        assert!(
            hi.units[1].health < lo.units[1].health,
            "high severity should degrade the FCC faster: {} vs {}",
            hi.units[1].health,
            lo.units[1].health
        );
    }

    #[test]
    fn deterministic_replay() {
        let cfg = test_config();
        let r = test_refinery();

        let mut s1 = new_game(r.clone(), &cfg, 42);
        let mut s2 = new_game(r, &cfg, 42);

        for _ in 0..20 {
            tick(&mut s1, &[], &cfg);
            tick(&mut s2, &[], &cfg);
        }

        assert!(
            (s1.cash - s2.cash).abs() < 1e-6,
            "same seed should produce identical results"
        );
    }

    #[test]
    fn degradation_reduces_health() {
        let cfg = test_config();
        let r = test_refinery();
        let mut state = new_game(r, &cfg, 42);

        for _ in 0..10 {
            tick(&mut state, &[], &cfg);
        }

        // After 10 weeks of running, health should have decreased.
        assert!(
            state.units[0].health < 1.0,
            "ADU health should degrade: {}",
            state.units[0].health
        );
    }

    #[test]
    fn turnaround_restores_health() {
        let mut cfg = test_config();
        cfg.victory_valuation = f64::MAX; // Prevent early win
        let r = test_refinery();
        let mut state = new_game(r, &cfg, 42);

        // Run for a while to degrade.
        for _ in 0..20 {
            tick(&mut state, &[], &cfg);
        }
        let health_before = state.units[0].health;
        assert!(health_before < 1.0, "health should have degraded, got {}", health_before);

        // Schedule turnaround.
        tick(
            &mut state,
            &[PlayerAction::ScheduleTurnaround("ADU".into())],
            &cfg,
        );

        let ta_weeks = cfg.equipment_for("ADU").unwrap().turnaround_weeks;
        for _ in 0..(ta_weeks + 2) {
            tick(&mut state, &[], &cfg);
        }

        assert!(
            state.units[0].health > health_before,
            "health should be restored after turnaround: before={}, after={}",
            health_before,
            state.units[0].health,
        );
    }
}
