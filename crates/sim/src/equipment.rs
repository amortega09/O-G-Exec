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
    /// Outage probability rolled this week — surfaced to the player so the gamble is
    /// visible and manageable. Recomputed every tick.
    pub last_hazard: f64,
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
            last_hazard: 0.0,
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

    /// Apply throughput-driven degradation for one week. Pure health change — tripping
    /// is now a separate stochastic roll (`outage_hazard` + `trip`).
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
    }

    /// Probability of an unplanned outage this week. Rises non-linearly as health falls
    /// (so reliability is a *managed gamble*, not a countdown), is certain below the
    /// hard floor, and is amplified by running hard (`severity_factor`).
    pub fn outage_hazard(&self, severity_factor: f64, cfg: &EquipmentConfig) -> f64 {
        if !self.is_running() {
            return 0.0;
        }
        if self.health <= cfg.trip_threshold {
            return 1.0;
        }
        let wear = (1.0 - self.health).clamp(0.0, 1.0).powf(cfg.hazard_exponent);
        let base = cfg.base_outage_hazard + (cfg.max_outage_hazard - cfg.base_outage_hazard) * wear;
        (base * severity_factor).clamp(0.0, 1.0)
    }

    /// Force the unit into an unplanned outage.
    pub fn trip(&mut self, cfg: &EquipmentConfig) {
        self.maintenance = MaintenanceStatus::Tripped {
            weeks_remaining: cfg.trip_outage_weeks,
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EquipmentConfig;

    fn cfg() -> EquipmentConfig {
        EquipmentConfig {
            unit_name: "T".into(),
            degradation_per_kbbl: 1e-5,
            high_severity_degradation_mult: 1.0,
            trip_threshold: 0.1,
            base_outage_hazard: 0.001,
            max_outage_hazard: 0.4,
            hazard_exponent: 3.0,
            turnaround_weeks: 4,
            turnaround_cost: 1.0,
            trip_outage_weeks: 6,
            trip_outage_cost: 1.0,
        }
    }

    fn unit_at(health: f64) -> UnitState {
        let mut u = UnitState::new("T".into());
        u.health = health;
        u
    }

    #[test]
    fn hazard_rises_with_wear_and_is_certain_below_floor() {
        let c = cfg();
        let healthy = unit_at(1.0).outage_hazard(1.0, &c);
        let worn = unit_at(0.4).outage_hazard(1.0, &c);
        let critical = unit_at(0.05).outage_hazard(1.0, &c); // below trip_threshold

        assert!((healthy - c.base_outage_hazard).abs() < 1e-9, "full health ≈ base hazard");
        assert!(healthy < worn, "wear must raise hazard");
        assert!((critical - 1.0).abs() < 1e-9, "below the floor, failure is certain");
        // Running harder amplifies the gamble.
        assert!(unit_at(0.4).outage_hazard(2.0, &c) > worn);
    }

    #[test]
    fn tripped_or_in_maintenance_has_no_hazard() {
        let c = cfg();
        let mut u = unit_at(0.5);
        u.trip(&c);
        assert_eq!(u.outage_hazard(1.0, &c), 0.0);
    }
}
