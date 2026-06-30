//! Debt financing. A single revolving-balance model: the player draws debt up to a
//! borrowing limit, pays weekly interest on the outstanding balance, and repays
//! principal when flush. Kept deliberately simple — the strategic point is *leverage
//! under uncertainty*, not an amortisation schedule.
//!
//! The balance sheet feeds valuation: equity = enterprise value + cash − debt. So
//! borrowing to fund capex is equity-neutral the moment you draw it; it only pays off
//! if the capacity it buys grows EBITDA faster than the interest bleeds.

use crate::config::FinanceConfig;

/// Remaining headroom under the borrowing base (£).
pub fn borrowing_capacity(debt: f64, cfg: &FinanceConfig) -> f64 {
    (cfg.max_debt - debt).max(0.0)
}

/// Draw `amount` of new debt, clamped to remaining capacity. Returns the amount
/// actually drawn (added to both cash and debt by the caller).
pub fn draw(debt: f64, amount: f64, cfg: &FinanceConfig) -> f64 {
    amount.max(0.0).min(borrowing_capacity(debt, cfg))
}

/// Repay `amount` of principal, clamped to what is owed and what cash is available.
/// Returns the amount actually repaid (subtracted from both cash and debt).
pub fn repay(debt: f64, cash: f64, amount: f64) -> f64 {
    amount.max(0.0).min(debt).min(cash.max(0.0))
}

/// Weekly interest expense on the current balance.
pub fn weekly_interest(debt: f64, cfg: &FinanceConfig) -> f64 {
    debt * cfg.weekly_rate()
}

/// Equity (the player's net worth, and the win metric):
///   enterprise value (operations) + cash − debt.
pub fn equity_value(enterprise_value: f64, cash: f64, debt: f64) -> f64 {
    enterprise_value + cash - debt
}
