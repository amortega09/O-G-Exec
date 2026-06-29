//! Capital project system. Players can approve pre-defined projects that cost
//! money, take time, and increase unit capacity on completion.

use crate::config::ProjectConfig;
use serde::{Deserialize, Serialize};

/// Runtime state of an active capital project.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectState {
    /// Index into `GameConfig::projects` for the blueprint.
    pub config_index: usize,
    /// Name (copied from config for convenience).
    pub name: String,
    /// Unit this project upgrades.
    pub unit_name: String,
    /// Capacity gain when complete (bbl/day).
    pub capacity_gain: f64,
    /// Total cost (£) — paid up front when approved.
    pub total_cost: f64,
    /// Weeks remaining until completion.
    pub weeks_remaining: u32,
}

impl ProjectState {
    /// Create a new active project from a config blueprint.
    pub fn from_config(cfg: &ProjectConfig, config_index: usize) -> Self {
        Self {
            config_index,
            name: cfg.name.clone(),
            unit_name: cfg.unit_name.clone(),
            capacity_gain: cfg.capacity_gain,
            total_cost: cfg.cost,
            weeks_remaining: cfg.duration_weeks,
        }
    }

    /// Advance by one week. Returns `Some(unit_name, capacity_gain)` if the
    /// project completed this tick.
    pub fn tick(&mut self) -> Option<(String, f64)> {
        if self.weeks_remaining <= 1 {
            Some((self.unit_name.clone(), self.capacity_gain))
        } else {
            self.weeks_remaining -= 1;
            None
        }
    }
}
