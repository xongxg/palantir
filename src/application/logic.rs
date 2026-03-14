//! Application: Logic Actions (Calculations)
//!
//! DDD Layer: Application Service — orchestrates domain calculation rules
//! (`domain::calculations`) over entities retrieved from the OntologyGraph.
//!
//! In Palantir Foundry, a **Logic action** is a calculation that runs on
//! ontology objects and produces derived metrics — salary bands, spend ratios,
//! concentration scores.  These are stateless transforms: same input → same output.
//!
//! Flow:
//!   OntologyGraph  (entity data)
//!     └─▶ logic::calc_*()  (application orchestration)
//!           └─▶ domain::calculations::*()  (pure business rules)
//!                 └─▶ derived metrics  (new calculated fields)

use std::collections::HashMap;

use crate::application::ontology::{graph::OntologyGraph, relationship::RelationshipKind};
use crate::domain::calculations;
use crate::infrastructure::pipeline::dataset::Value;

// ─── Output types ─────────────────────────────────────────────────────────────

/// Result of the "salary band" Logic action.
pub struct SalaryBandGroup {
    pub band:    &'static str,
    /// Employee names in this band, sorted alphabetically.
    pub members: Vec<String>,
}

/// Result of the "spend metrics" Logic action — one row per employee.
pub struct SpendMetrics {
    pub employee_id:     String,
    pub name:            String,
    pub department:      String,
    pub annual_salary:   f64,
    pub total_spend:     f64,
    /// spend / salary * 100
    pub spend_ratio_pct: f64,
    pub risk_level:      &'static str,
}

/// Result of the "category concentration" Logic action — one row per employee.
pub struct CategoryConcentration {
    pub employee_id:      String,
    pub name:             String,
    pub top_category:     String,
    pub top_amount:       f64,
    pub total_spend:      f64,
    /// top_category_amount / total_spend
    pub concentration_pct: f64,
}

// ─── Logic Actions ────────────────────────────────────────────────────────────

/// Logic action: group employees into salary bands.
///
/// Traverses Employee objects in the graph, calls `domain::calculations::salary_band`
/// on each salary field, and returns groups sorted from highest to lowest band.
pub fn calc_salary_bands(graph: &OntologyGraph) -> Vec<SalaryBandGroup> {
    let mut groups: HashMap<&'static str, Vec<String>> = HashMap::new();

    for emp in graph.objects_by_type("Employee") {
        let salary = emp.get("salary").and_then(Value::as_f64).unwrap_or(0.0);
        let name   = emp.label();
        let band   = calculations::salary_band(salary);
        groups.entry(band).or_default().push(name);
    }

    // Sort band keys by minimum salary threshold (highest first)
    let band_order = ["Staff Band    ($130k+)", "Senior Band   ($100k–$130k)",
                      "Mid Band      ($70k–$100k)", "Junior Band   ($0–$70k)"];
    let mut result: Vec<SalaryBandGroup> = band_order.iter()
        .filter_map(|&band| {
            groups.remove(band).map(|mut members| {
                members.sort();
                SalaryBandGroup { band, members }
            })
        })
        .collect();

    // Any unexpected bands appended at the end
    for (band, mut members) in groups {
        members.sort();
        result.push(SalaryBandGroup { band, members });
    }

    result
}

/// Logic action: compute per-employee spend metrics.
///
/// Joins Employee → Transaction via HAS edges, then calls domain calculation rules
/// to produce spend ratio and risk level for each employee.
pub fn calc_spend_metrics(graph: &OntologyGraph) -> Vec<SpendMetrics> {
    let mut rows: Vec<SpendMetrics> = graph.objects_by_type("Employee")
        .into_iter()
        .map(|emp| {
            let salary = emp.get("salary").and_then(Value::as_f64).unwrap_or(0.0);

            // Traverse HAS edges to sum transaction amounts
            let total_spend: f64 = graph
                .outgoing(&emp.id.0, Some(&RelationshipKind::Has))
                .into_iter()
                .filter(|r| r.to_type.0 == "Transaction")
                .filter_map(|r| graph.find_object(&r.to_id.0))
                .filter_map(|tx| tx.get("amount").and_then(Value::as_f64))
                .sum();

            let spend_ratio_pct = calculations::spend_ratio_pct(total_spend, salary);

            // Compute concentration for risk level
            let concentration = {
                let mut by_cat: HashMap<String, f64> = HashMap::new();
                for rel in graph.outgoing(&emp.id.0, Some(&RelationshipKind::Has)) {
                    if let Some(tx) = graph.find_object(&rel.to_id.0) {
                        let cat = tx.get("category")
                            .and_then(Value::as_str)
                            .unwrap_or("Other")
                            .to_string();
                        let amt = tx.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
                        *by_cat.entry(cat).or_default() += amt;
                    }
                }
                let top = by_cat.values().cloned().fold(0.0_f64, f64::max);
                calculations::concentration_ratio(top, total_spend)
            };

            let department = graph
                .outgoing(&emp.id.0, Some(&RelationshipKind::BelongsTo))
                .into_iter()
                .find(|r| r.to_type.0 == "department")
                .map(|r| r.to_id.0.trim_start_matches("department:").to_string())
                .unwrap_or_default();

            SpendMetrics {
                employee_id:     emp.id.0.clone(),
                name:            emp.label(),
                department,
                annual_salary:   salary,
                total_spend,
                spend_ratio_pct,
                risk_level: calculations::expense_risk_level(spend_ratio_pct, concentration),
            }
        })
        .collect();

    rows.sort_by(|a, b| b.spend_ratio_pct.partial_cmp(&a.spend_ratio_pct).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

/// Logic action: compute category concentration per employee.
///
/// For each employee, finds the top spend category and computes the fraction
/// of total spend it represents (using `domain::calculations::concentration_ratio`).
pub fn calc_category_concentration(graph: &OntologyGraph) -> Vec<CategoryConcentration> {
    let mut rows: Vec<CategoryConcentration> = graph.objects_by_type("Employee")
        .into_iter()
        .filter_map(|emp| {
            let mut by_cat: HashMap<String, f64> = HashMap::new();
            for rel in graph.outgoing(&emp.id.0, Some(&RelationshipKind::Has)) {
                if let Some(tx) = graph.find_object(&rel.to_id.0) {
                    let cat = tx.get("category")
                        .and_then(Value::as_str)
                        .unwrap_or("Other")
                        .to_string();
                    let amt = tx.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
                    *by_cat.entry(cat).or_default() += amt;
                }
            }
            if by_cat.is_empty() {
                return None;
            }
            let total_spend: f64 = by_cat.values().sum();
            let (top_category, &top_amount) = by_cat.iter()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(k, v)| (k, v))
                .unwrap();

            let concentration_pct =
                calculations::concentration_ratio(top_amount, total_spend) * 100.0;

            Some(CategoryConcentration {
                employee_id:      emp.id.0.clone(),
                name:             emp.label(),
                top_category:     top_category.clone(),
                top_amount,
                total_spend,
                concentration_pct,
            })
        })
        .collect();

    rows.sort_by(|a, b| b.concentration_pct.partial_cmp(&a.concentration_pct).unwrap_or(std::cmp::Ordering::Equal));
    rows
}
