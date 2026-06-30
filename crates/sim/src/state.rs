//! Complete game state. This is the single source of truth — serializable,
//! deterministic, and entirely UI-independent.

use crate::config::GameConfig;
use crate::equipment::UnitState;
use crate::market::MarketState;
use crate::rng::RngStreams;
use crate::projects::ProjectState;
use refinery_lp::model::Refinery;
use refinery_lp::solve::SolveResult;
use serde::{Deserialize, Serialize};

/// Overall game status.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum GameStatus {
    Running,
    Won { week: u32 },
    Lost { week: u32, reason: String },
}

/// Player's current control settings (sliders and toggles).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSettings {
    /// Severity bias: 0.0 = all low-severity, 1.0 = all high-severity.
    /// Implemented as an upper-bound on high-severity feed share.
    pub severity_target: f64,
    /// Product tilt: 0.0 = neutral, positive = favour diesel, negative = favour gasoline.
    /// Implemented as an objective bonus/penalty.
    pub product_tilt: f64,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            severity_target: 0.5,
            product_tilt: 0.0,
        }
    }
}

/// Snapshot of one week's results, stored for charting and valuation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeekSnapshot {
    pub week: u32,
    /// Operating margin (EBITDA proxy): banked to cash before interest.
    pub margin: f64,
    pub interest: f64,
    pub debt: f64,
    pub cash: f64,
    pub valuation: f64,
    pub crude_price: f64,
    pub gasoline_price: f64,
    pub diesel_price: f64,
    pub crude_charge: f64,
    pub gasoline_volume: f64,
    pub diesel_volume: f64,
}

/// An event that happened during a tick, for the event log.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameEvent {
    pub week: u32,
    pub message: String,
    pub severity: EventSeverity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EventSeverity {
    Info,
    Warning,
    Critical,
}

/// Actions the player can take between ticks.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PlayerAction {
    /// Set severity target (0.0–1.0).
    SetSeverity(f64),
    /// Set product tilt (-1.0 to 1.0).
    SetProductTilt(f64),
    /// Schedule a turnaround for a unit (by name).
    ScheduleTurnaround(String),
    /// Approve a capital project (by config index).
    ApproveProject(usize),
    /// Draw new debt (£), clamped to the borrowing base.
    Borrow(f64),
    /// Repay debt principal (£), clamped to what is owed and what cash allows.
    Repay(f64),
}

/// The full game state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameState {
    pub week: u32,
    pub cash: f64,
    /// Outstanding debt principal (£).
    pub debt: f64,
    /// Enterprise value (£) — the win metric. Operating value = EBITDA × multiple.
    pub valuation: f64,
    pub status: GameStatus,
    pub player: PlayerSettings,
    pub market: MarketState,
    pub units: Vec<UnitState>,
    pub projects: Vec<ProjectState>,
    pub history: Vec<WeekSnapshot>,
    pub events: Vec<GameEvent>,
    pub rng: RngStreams,
    /// Realized execution efficiency last tick (LP plan actually achieved, 0–1).
    pub last_execution_efficiency: f64,

    // --- Internal: the refinery configs ---
    /// The current refinery config (prices/demands updated each tick).
    pub refinery: Refinery,
    /// The base refinery config (pristine capacities, for reference).
    pub base_refinery: Refinery,

    /// Last LP solve result (not serialized to JSON for the UI; recomputed each tick).
    #[serde(skip)]
    pub last_solve: Option<SolveResult>,
}

/// A view of the game state suitable for sending to the UI. Contains everything
/// the frontend needs to render, without internal engine details.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameView {
    pub week: u32,
    pub cash: f64,
    pub valuation: f64,
    pub victory_target: f64,
    pub status: GameStatus,
    pub player: PlayerSettings,
    pub market: MarketView,
    pub units: Vec<UnitView>,
    pub products: Vec<ProductView>,
    pub active_projects: Vec<ProjectView>,
    pub available_projects: Vec<AvailableProject>,
    pub history: Vec<WeekSnapshot>,
    pub events: Vec<GameEvent>,
    pub shadow_prices: Vec<(String, f64)>,
    /// Net weekly margin banked to cash = revenue − crude − variable_opex − fixed_opex.
    pub weekly_margin: f64,
    /// Realized execution efficiency this week (LP plan actually achieved, 0–1).
    pub execution_efficiency: f64,
    pub crude_charge: f64,
    pub revenue: f64,
    pub crude_cost: f64,
    pub variable_opex: f64,
    pub fixed_opex: f64,
    // --- Balance sheet ---
    pub debt: f64,
    pub interest: f64,
    pub borrowing_capacity: f64,
    /// Operating value of the business (EBITDA × multiple) — the win metric, == valuation.
    pub enterprise_value: f64,
    /// Net worth = enterprise value + cash − debt. Shown for awareness, not the win.
    pub equity: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketView {
    pub crude_price: f64,
    pub gasoline_price: f64,
    pub diesel_price: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnitView {
    pub name: String,
    pub health: f64,
    pub throughput: f64,
    pub capacity: f64,
    pub effective_capacity: f64,
    pub utilisation: f64,
    pub maintenance_status: String,
    pub maintenance_weeks_remaining: Option<u32>,
    pub realised_severity: Option<f64>,
    /// Weekly unplanned-outage probability (0–1) — the visible reliability gamble.
    pub outage_risk: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductView {
    pub name: String,
    pub volume: f64,
    pub price: f64,
    pub blend: Vec<(String, f64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectView {
    pub name: String,
    pub unit_name: String,
    pub capacity_gain: f64,
    pub weeks_remaining: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AvailableProject {
    pub config_index: usize,
    pub name: String,
    pub description: String,
    pub unit_name: String,
    pub capacity_gain: f64,
    pub cost: f64,
    pub duration_weeks: u32,
}

impl GameState {
    /// Create a GameView for the UI from the current state.
    pub fn view(&self, cfg: &GameConfig) -> GameView {
        let solve = self.last_solve.as_ref();

        let crude_charge = solve.map(|s| s.crude_charge).unwrap_or(0.0);

        // Weekly P&L straight from the LP's truthful daily breakdown (× 7 days). Every
        // line is a real flow at a real price — nothing is approximated or back-solved.
        let fin = solve.map(|s| s.finances.clone()).unwrap_or_default();
        let revenue = fin.revenue() * 7.0;
        let crude_cost = fin.crude_cost * 7.0;
        let var_opex = fin.opex * 7.0;
        let fixed_opex = cfg.fixed_opex_per_week;
        // Net weekly margin — this is exactly what tick() banks to cash.
        let weekly_margin = revenue - crude_cost - var_opex - fixed_opex;

        let units = self
            .units
            .iter()
            .enumerate()
            .map(|(i, u)| {
                let (throughput, sev) = if i == 0 {
                    // ADU
                    (
                        solve.map(|s| s.crude_charge).unwrap_or(0.0),
                        None,
                    )
                } else if let Some(conv) = solve.and_then(|s| s.conversions.get(i - 1)) {
                    (
                        conv.throughput,
                        if conv.realised_severity.is_nan() {
                            None
                        } else {
                            Some(conv.realised_severity)
                        },
                    )
                } else {
                    (0.0, None)
                };

                let base_cap = if i == 0 {
                    self.base_refinery.adu.capacity
                } else {
                    self.base_refinery
                        .conversions
                        .get(i - 1)
                        .map(|c| c.capacity)
                        .unwrap_or(0.0)
                };
                let eff_cap = base_cap * u.capacity_factor();

                let (maint_str, maint_weeks) = match &u.maintenance {
                    crate::equipment::MaintenanceStatus::Running => ("Running".to_string(), None),
                    crate::equipment::MaintenanceStatus::InTurnaround { weeks_remaining } => {
                        ("Turnaround".to_string(), Some(*weeks_remaining))
                    }
                    crate::equipment::MaintenanceStatus::Tripped { weeks_remaining } => {
                        ("Tripped!".to_string(), Some(*weeks_remaining))
                    }
                };

                UnitView {
                    name: u.unit_name.clone(),
                    health: u.health,
                    throughput,
                    capacity: base_cap,
                    effective_capacity: eff_cap,
                    utilisation: if eff_cap > 0.0 {
                        throughput / eff_cap
                    } else {
                        0.0
                    },
                    maintenance_status: maint_str,
                    maintenance_weeks_remaining: maint_weeks,
                    realised_severity: sev,
                    outage_risk: u.last_hazard,
                }
            })
            .collect();

        let products = solve
            .map(|s| {
                s.products
                    .iter()
                    .zip(self.refinery.products.iter())
                    .map(|(pr, p)| ProductView {
                        name: pr.name.clone(),
                        volume: pr.volume,
                        price: p.price,
                        blend: pr.blend.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let active_projects = self
            .projects
            .iter()
            .map(|p| ProjectView {
                name: p.name.clone(),
                unit_name: p.unit_name.clone(),
                capacity_gain: p.capacity_gain,
                weeks_remaining: p.weeks_remaining,
            })
            .collect();

        let active_indices: Vec<usize> = self.projects.iter().map(|p| p.config_index).collect();
        let available_projects = cfg
            .projects
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                p.available_after_week <= self.week && !active_indices.contains(i)
            })
            .map(|(i, p)| AvailableProject {
                config_index: i,
                name: p.name.clone(),
                description: p.description.clone(),
                unit_name: p.unit_name.clone(),
                capacity_gain: p.capacity_gain,
                cost: p.cost,
                duration_weeks: p.duration_weeks,
            })
            .collect();

        let shadow_prices = solve
            .map(|_| refinery_lp::solve::capacity_shadow_prices(&self.refinery, 100.0))
            .unwrap_or_default();

        GameView {
            week: self.week,
            cash: self.cash,
            valuation: self.valuation,
            victory_target: cfg.victory_valuation,
            status: self.status.clone(),
            player: self.player.clone(),
            market: MarketView {
                crude_price: self.market.crude_price,
                gasoline_price: self.market.gasoline_price,
                diesel_price: self.market.diesel_price,
            },
            units,
            products,
            active_projects,
            available_projects,
            history: self.history.clone(),
            events: self.events.clone(),
            shadow_prices,
            weekly_margin,
            execution_efficiency: self.last_execution_efficiency,
            crude_charge,
            revenue,
            crude_cost,
            variable_opex: var_opex,
            fixed_opex,
            debt: self.debt,
            interest: crate::finance::weekly_interest(self.debt, &cfg.finance),
            borrowing_capacity: crate::finance::borrowing_capacity(self.debt, &cfg.finance),
            enterprise_value: self.valuation, // valuation IS enterprise value (the win metric)
            equity: crate::finance::equity_value(self.valuation, self.cash, self.debt),
        }
    }
}
