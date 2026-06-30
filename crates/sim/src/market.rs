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

        // Diesel/middle-distillate cracks peak in winter (opposite phase to gasoline).
        let diesel_seasonal = cfg.diesel_seasonal_amplitude
            * (2.0 * std::f64::consts::PI * (week as f64 - 46.0) / 52.0).sin();
        let diesel_target = cfg.diesel_spread_mean + diesel_seasonal;
        let diesel_noise = rng.normal() * cfg.spread_volatility;
        self.diesel_spread +=
            cfg.spread_reversion * (diesel_target - self.diesel_spread) + diesel_noise;
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

    /// Roll the discrete fat-tail shocks (supply/demand/OPEC/refining). Each that fires
    /// applies a multiplicative jump to the price level / cracks; the weak mean-reversion
    /// then decays it over months. Uses its own RNG stream so it stays independent of the
    /// continuous price noise. Returns human-readable messages for the event log.
    pub fn roll_shocks(&mut self, cfg: &MarketConfig, rng: &mut Rng) -> Vec<String> {
        let mut msgs = Vec::new();
        for sh in &cfg.shocks {
            if !rng.chance(sh.weekly_probability) {
                continue;
            }
            let cm = sh.crude_mult_min + rng.next_f64() * (sh.crude_mult_max - sh.crude_mult_min);
            let km = sh.crack_mult_min + rng.next_f64() * (sh.crack_mult_max - sh.crack_mult_min);
            self.crude_price = (self.crude_price * cm).max(20.0);
            self.gasoline_spread = (self.gasoline_spread * km).max(2.0);
            self.diesel_spread = (self.diesel_spread * km).max(2.0);
            self.gasoline_price = self.crude_price + self.gasoline_spread;
            self.diesel_price = self.crude_price + self.diesel_spread;

            let mut parts = Vec::new();
            let cpct = (cm - 1.0) * 100.0;
            let kpct = (km - 1.0) * 100.0;
            if cpct.abs() > 1.0 {
                parts.push(format!("crude {cpct:+.0}%"));
            }
            if kpct.abs() > 1.0 {
                parts.push(format!("cracks {kpct:+.0}%"));
            }
            msgs.push(format!("{} — {}", sh.name, parts.join(", ")));
        }
        msgs
    }
}
