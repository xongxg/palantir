//! Domain Service: pure calculation rules.
//!
//! These functions encode business calculation logic with no I/O, no graph dependency,
//! no state — just input → output.  This is where the "Logic" action category lives
//! at the domain level.  The application layer calls these over collections of entities.

/// Classify an annual salary into a band label.
pub fn salary_band(salary: f64) -> &'static str {
    match salary as u64 {
        0..=69_999        => "Junior Band   ($0–$70k)",
        70_000..=99_999   => "Mid Band      ($70k–$100k)",
        100_000..=129_999 => "Senior Band   ($100k–$130k)",
        _                 => "Staff Band    ($130k+)",
    }
}

/// Expense ratio: what percentage of annual salary is spent on transactions.
/// Palantir Logic action: "compute spend ratio per employee".
pub fn spend_ratio_pct(total_spend: f64, annual_salary: f64) -> f64 {
    if annual_salary == 0.0 {
        0.0
    } else {
        (total_spend / annual_salary) * 100.0
    }
}

/// Concentration ratio: what fraction of total spend is in the top category.
/// Palantir Logic action: "detect category concentration".
pub fn concentration_ratio(top_category_amount: f64, total_spend: f64) -> f64 {
    if total_spend == 0.0 {
        0.0
    } else {
        top_category_amount / total_spend
    }
}

/// Derive a qualitative risk level from spend ratio and category concentration.
/// Palantir Logic action: "compute employee expense risk score".
pub fn expense_risk_level(spend_ratio: f64, concentration: f64) -> &'static str {
    match (spend_ratio > 3.0, concentration > 0.6) {
        (true,  true)  => "High",
        (true,  false) | (false, true) => "Medium",
        (false, false) => "Low",
    }
}
