//! Equipment degradation and maintenance model. Each unit has a health value
//! (0.0–1.0) that degrades with throughput and can be restored via turnarounds.

use crate::config::EquipmentConfig;
use serde::{Deserialize, Serialize};

/// Current state of one process unit's equipment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnitState {
    pub unit_name: String,
    /// Health in [0.0, 1.0]. Multiplies effective capacity.
    pub health: f64,
    /// Maintenance status.
    pub maintenance: MaintenanceStatus,
    /// Cumulative barrels processed (lifetime, for stats).
    pub lifetime_throughput: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MaintenanceStatus {
    /// Unit is running normally.
    Running,
    /// Planned turnaround in progress. `weeks_remaining` counts down.
    InTurnaround { weeks_remaining: u32 },
    /// Unplanned trip/outage. Same structure but longer/costlier.
    Tripped { weeks_remaining: u32 },
}

impl UnitState {
    pub fn new(unit_name: String) -> Self {
        Self {
            unit_name,
            health: 1.0,
            maintenance: MaintenanceStatus::Running,
            lifetime_throughput: 0.0,
        }
    }

    /// Effective capacity multiplier based on current health and maintenance status.
    pub fn capacity_factor(&self) -> f64 {
        match self.maintenance {
            MaintenanceStatus::Running => self.health,
            MaintenanceStatus::InTurnaround { .. } | MaintenanceStatus::Tripped { .. } => 0.0,
        }
    }

    /// Is this unit available for production?
    pub fn is_running(&self) -> bool {
        self.maintenance == MaintenanceStatus::Running
    }

    /// Apply throughput-driven degradation for one week.
    /// `throughput_bbl_day` is the daily rate from the LP; we assume 7 days/week.
    /// `severity_factor` is 1.0 for normal ops, higher for high-severity modes.
    pub fn degrade(
        &mut self,
        throughput_bbl_day: f64,
        severity_factor: f64,
        cfg: &EquipmentConfig,
    ) {
        if !self.is_running() {
            return;
        }
        let weekly_bbl = throughput_bbl_day * 7.0;
        self.lifetime_throughput += weekly_bbl;
        let health_loss =
            (weekly_bbl / 1000.0) * cfg.degradation_per_kbbl * severity_factor;
        self.health = (self.health - health_loss).max(0.0);

        // Auto-trip if health drops below threshold.
        if self.health <= cfg.trip_threshold {
            self.maintenance = MaintenanceStatus::Tripped {
                weeks_remaining: cfg.trip_outage_weeks,
            };
        }
    }

    /// Start a planned turnaround. Returns the cost, or None if unit is already
    /// in maintenance.
    pub fn start_turnaround(&mut self, cfg: &EquipmentConfig) -> Option<f64> {
        if !self.is_running() {
            return None;
        }
        self.maintenance = MaintenanceStatus::InTurnaround {
            weeks_remaining: cfg.turnaround_weeks,
        };
        Some(cfg.turnaround_cost)
    }

    /// Advance maintenance timers by one week. Returns true if the unit just
    /// came back online.
    pub fn tick_maintenance(&mut self) -> bool {
        match &mut self.maintenance {
            MaintenanceStatus::Running => false,
            MaintenanceStatus::InTurnaround { weeks_remaining }
            | MaintenanceStatus::Tripped { weeks_remaining } => {
                if *weeks_remaining <= 1 {
                    self.maintenance = MaintenanceStatus::Running;
                    self.health = 1.0; // turnaround restores to full health
                    true
                } else {
                    *weeks_remaining -= 1;
                    false
                }
            }
        }
    }
}
