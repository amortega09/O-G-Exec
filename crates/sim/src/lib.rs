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
        for i in 0..(ta_weeks + 2) {
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
