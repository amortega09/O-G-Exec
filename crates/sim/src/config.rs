//! Game configuration loaded from JSON scenario files. Contains all tuneable
//! parameters that define a scenario: starting conditions, market behaviour,
//! degradation rates, available capital projects, and victory conditions.

use serde::{Deserialize, Serialize};

/// Top-level scenario configuration. Loaded from `data/scenarios/<name>.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameConfig {
    pub name: String,
    pub description: String,
    /// Starting cash (£).
    pub starting_cash: f64,
    /// Fixed weekly overhead not tied to throughput (£/week). Covers admin,
    /// insurance, site costs, etc.
    pub fixed_opex_per_week: f64,
    /// Target valuation (£) to win the game.
    pub victory_valuation: f64,
    /// Number of trailing weeks used for valuation calculation.
    pub valuation_lookback_weeks: u32,
    /// EV/EBITDA-style multiple applied to annualised trailing margin.
    pub valuation_multiple: f64,
    /// Market model parameters.
    pub market: MarketConfig,
    /// Per-unit equipment parameters (aligned to refinery units by name).
    pub equipment: Vec<EquipmentConfig>,
    /// Capital projects available to the player.
    pub projects: Vec<ProjectConfig>,
}

/// Market price model configuration. Prices follow an Ornstein-Uhlenbeck
/// (mean-reverting) process: dP = θ(μ - P)dt + σ·dW.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketConfig {
    /// Base crude price (£/bbl) — the long-run mean.
    pub crude_mean: f64,
    /// Mean-reversion speed for crude (higher = faster reversion). θ in the OU process.
    pub crude_reversion: f64,
    /// Weekly volatility for crude price (σ).
    pub crude_volatility: f64,
    /// Base gasoline crack spread (£/bbl over crude).
    pub gasoline_spread_mean: f64,
    /// Base diesel crack spread (£/bbl over crude).
    pub diesel_spread_mean: f64,
    /// Crack spread volatility (applied to both products).
    pub spread_volatility: f64,
    /// Crack spread mean-reversion speed.
    pub spread_reversion: f64,
    /// Seasonal amplitude for gasoline spread (£/bbl). Peaks in "summer" (weeks 20-32).
    pub gasoline_seasonal_amplitude: f64,
    /// Demand ceiling fluctuation (fraction, e.g. 0.10 = ±10%).
    pub demand_volatility: f64,
}

/// Per-unit equipment degradation and maintenance parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EquipmentConfig {
    /// Unit name — must match the name in the Refinery model.
    pub unit_name: String,
    /// Health lost per 1000 bbl processed at base severity.
    pub degradation_per_kbbl: f64,
    /// Multiplier on degradation rate when running high-severity modes.
    pub high_severity_degradation_mult: f64,
    /// Health threshold below which the unit trips automatically.
    pub trip_threshold: f64,
    /// Duration of a planned turnaround (weeks).
    pub turnaround_weeks: u32,
    /// Cost of a planned turnaround (£).
    pub turnaround_cost: f64,
    /// Duration of an unplanned (trip) outage (weeks) — longer than planned.
    pub trip_outage_weeks: u32,
    /// Cost of an unplanned outage (£) — more expensive than planned.
    pub trip_outage_cost: f64,
}

/// A capital project the player can approve.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub description: String,
    /// Which unit this upgrades (by name).
    pub unit_name: String,
    /// Base capacity added (bbl/day) when project completes.
    pub capacity_gain: f64,
    /// Total cost (£).
    pub cost: f64,
    /// Construction duration (weeks).
    pub duration_weeks: u32,
    /// Earliest game week this project becomes available.
    pub available_after_week: u32,
}

impl GameConfig {
    /// Load a GameConfig from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Find equipment config for a unit by name.
    pub fn equipment_for(&self, unit_name: &str) -> Option<&EquipmentConfig> {
        self.equipment.iter().find(|e| e.unit_name == unit_name)
    }
}
