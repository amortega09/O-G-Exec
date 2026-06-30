//! Market price model. Prices follow an Ornstein-Uhlenbeck (mean-reverting)
//! process with optional seasonal overlays. Deterministic given a seed, so
//! games are replayable.

use crate::config::MarketConfig;
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// Current market state — carried in GameState, updated each tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketState {
    pub crude_price: f64,
    pub gasoline_price: f64,
    pub diesel_price: f64,
    pub gasoline_demand: f64,
    pub diesel_demand: f64,
    /// Internal: current crack spreads (for mean-reversion tracking).
    gasoline_spread: f64,
    diesel_spread: f64,
}

impl MarketState {
    /// Initialise market state from config.
    pub fn new(cfg: &MarketConfig, base_gasoline_demand: f64, base_diesel_demand: f64) -> Self {
        Self {
            crude_price: cfg.crude_mean,
            gasoline_spread: cfg.gasoline_spread_mean,
            diesel_spread: cfg.diesel_spread_mean,
            gasoline_price: cfg.crude_mean + cfg.gasoline_spread_mean,
            diesel_price: cfg.crude_mean + cfg.diesel_spread_mean,
            gasoline_demand: base_gasoline_demand,
            diesel_demand: base_diesel_demand,
        }
    }

    /// Advance the market by one week. `week` is the current week number (for
    /// seasonality), `rng` is the shared game PRNG.
    pub fn step(
        &mut self,
        week: u32,
        cfg: &MarketConfig,
        base_gasoline_demand: f64,
        base_diesel_demand: f64,
        rng: &mut Rng,
    ) {
        // --- Crude price: OU mean-reversion ---
        let crude_noise = rng.normal() * cfg.crude_volatility;
        self.crude_price +=
            cfg.crude_reversion * (cfg.crude_mean - self.crude_price) + crude_noise;
        self.crude_price = self.crude_price.max(20.0); // floor: crude can't go below £20

        // --- Crack spreads: OU + seasonality ---
        let gasoline_seasonal = cfg.gasoline_seasonal_amplitude
            * (2.0 * std::f64::consts::PI * (week as f64 - 20.0) / 52.0).sin();
        let gasoline_target = cfg.gasoline_spread_mean + gasoline_seasonal;

        let gaso_noise = rng.normal() * cfg.spread_volatility;
        self.gasoline_spread +=
            cfg.spread_reversion * (gasoline_target - self.gasoline_spread) + gaso_noise;
        self.gasoline_spread = self.gasoline_spread.max(2.0); // minimum spread

        let diesel_noise = rng.normal() * cfg.spread_volatility;
        self.diesel_spread +=
            cfg.spread_reversion * (cfg.diesel_spread_mean - self.diesel_spread) + diesel_noise;
        self.diesel_spread = self.diesel_spread.max(2.0);

        // --- Product prices ---
        self.gasoline_price = self.crude_price + self.gasoline_spread;
        self.diesel_price = self.crude_price + self.diesel_spread;

        // --- Demand ceilings: slow random walk around base ---
        let demand_noise_g = rng.normal() * cfg.demand_volatility * 0.3; // damped
        self.gasoline_demand = (base_gasoline_demand * (1.0 + demand_noise_g))
            .max(base_gasoline_demand * 0.7)
            .min(base_gasoline_demand * 1.3);

        let demand_noise_d = rng.normal() * cfg.demand_volatility * 0.3;
        self.diesel_demand = (base_diesel_demand * (1.0 + demand_noise_d))
            .max(base_diesel_demand * 0.7)
            .min(base_diesel_demand * 1.3);
    }
}
