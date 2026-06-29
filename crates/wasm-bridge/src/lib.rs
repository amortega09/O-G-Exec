//! WASM entry point. Thin wasm-bindgen glue that exposes the sim engine to
//! JavaScript. No game logic lives here — it's pure translation between the
//! Rust sim types and JS-friendly values.

use wasm_bindgen::prelude::*;

use sim::config::GameConfig;
use sim::state::PlayerAction;
use sim::{GameState, GameStatus};

/// Initialise panic hook for readable error messages in the browser console.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// The main game handle exposed to JavaScript.
#[wasm_bindgen]
pub struct Game {
    state: GameState,
    config: GameConfig,
}

#[wasm_bindgen]
impl Game {
    /// Create a new game. `scenario_json` is the content of a scenario config
    /// file (e.g. tutorial.json). `refinery_json` is the refinery model JSON.
    /// `seed` is the PRNG seed for deterministic replay.
    #[wasm_bindgen(constructor)]
    pub fn new(scenario_json: &str, refinery_json: &str, seed: u64) -> Result<Game, JsError> {
        let config: GameConfig =
            serde_json::from_str(scenario_json).map_err(|e| JsError::new(&e.to_string()))?;
        let refinery: refinery_lp::model::Refinery =
            serde_json::from_str(refinery_json).map_err(|e| JsError::new(&e.to_string()))?;
        let state = sim::new_game(refinery, &config, seed);
        Ok(Game { state, config })
    }

    /// Advance the game by one week. `actions_json` is a JSON array of
    /// PlayerAction values (or an empty array "[]" for no actions).
    /// Returns the full GameView as a JS object.
    pub fn tick(&mut self, actions_json: &str) -> Result<JsValue, JsError> {
        let actions: Vec<PlayerAction> =
            serde_json::from_str(actions_json).map_err(|e| JsError::new(&e.to_string()))?;
        let view = sim::tick(&mut self.state, &actions, &self.config);
        serde_wasm_bindgen::to_value(&view).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Get the current game view without advancing time.
    pub fn view(&self) -> Result<JsValue, JsError> {
        let view = self.state.view(&self.config);
        serde_wasm_bindgen::to_value(&view).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Check if the game is still running.
    pub fn is_running(&self) -> bool {
        self.state.status == GameStatus::Running
    }

    /// Get current week number.
    pub fn week(&self) -> u32 {
        self.state.week
    }

    /// Get current cash.
    pub fn cash(&self) -> f64 {
        self.state.cash
    }

    /// Get current valuation.
    pub fn valuation(&self) -> f64 {
        self.state.valuation
    }
}
