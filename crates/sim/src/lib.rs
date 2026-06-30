//! O-G-Exec simulation engine. Wraps the refinery LP solver in a game loop
//! with time progression, market dynamics, equipment degradation, and capital
//! projects.
//!
//! This crate is UI-independent — it produces serializable state that the
//! WASM bridge passes to the browser frontend.

pub mod config;
pub mod equipment;
pub mod market;
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
    fn do_nothing_does_not_instantly_win() {
        // Regression: a healthy plant's valuation spikes early, but the trailing-average
        // board review must not crown a win within the first lookback window.
        let cfg = test_config();
        let mut s = new_game(test_refinery(), &cfg, 42);
        for _ in 0..cfg.valuation_lookback_weeks {
            tick(&mut s, &[], &cfg);
        }
        assert_eq!(
            s.status,
            GameStatus::Running,
            "should still be running through the first lookback window"
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
