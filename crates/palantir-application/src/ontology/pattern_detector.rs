//! Ontology Pattern Detector — semantic observer that closes the DDD event loop.
//!
//! Scans the OntologyGraph for business patterns, emits DomainEvents.
//!
//! Full loop:
//!   Data → DiscoveryEngine → OntologyGraph → PatternDetector → DomainEvent
//!   → EventBus → ApplicationService → Command → Domain

use crate::infrastructure::pipeline::dataset::Value;
use crate::domain::events::{DomainEvent, EventBus};

use super::{graph::OntologyGraph, relationship::RelationshipKind};

#[derive(Debug, Clone)]
pub struct DetectedPattern {
    pub kind:         PatternKind,
    pub entity_id:    String,
    pub entity_label: String,
    pub detail:       String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    HighSpend,
    CategoryConcentration,
}

impl PatternKind {
    pub fn label(&self) -> &str {
        match self {
            Self::HighSpend             => "HIGH_SPEND_EMPLOYEE",
            Self::CategoryConcentration => "CATEGORY_CONCENTRATION",
        }
    }

    pub fn ddd_trigger(&self) -> &str {
        match self {
            Self::HighSpend             => "FlagHighValueCommand",
            Self::CategoryConcentration => "ReviewSpendPolicyCommand",
        }
    }
}

pub struct PatternDetector;

impl PatternDetector {
    pub fn scan(
        graph:     &OntologyGraph,
        event_bus: &mut EventBus,
        high_spend_threshold: f64,
    ) -> Vec<DetectedPattern> {
        let mut patterns = Vec::new();

        for emp in graph.objects_by_type("Employee") {
            let emp_id    = emp.id.0.clone();
            let emp_label = emp.label();
            let dept      = emp.get("department").and_then(Value::as_str)
                              .unwrap_or("?").to_string();

            let tx_amounts: Vec<(f64, String)> = graph
                .outgoing(&emp_id, Some(&RelationshipKind::Has))
                .into_iter()
                .filter(|r| r.to_type.0 == "Transaction")
                .filter_map(|r| graph.find_object(&r.to_id.0))
                .filter_map(|tx| {
                    let amount   = tx.get("amount").and_then(Value::as_f64)?;
                    let category = tx.get("category").and_then(Value::as_str)?.to_string();
                    Some((amount, category))
                })
                .collect();

            let total_spend: f64 = tx_amounts.iter().map(|(a, _)| a).sum();

            // Pattern 1: High spend
            if total_spend > high_spend_threshold {
                let detail = format!(
                    "{} — total ${:.0} (threshold ${:.0}) dept: {}",
                    emp_label, total_spend, high_spend_threshold, dept
                );
                patterns.push(DetectedPattern {
                    kind:         PatternKind::HighSpend,
                    entity_id:    emp_id.clone(),
                    entity_label: emp_label.clone(),
                    detail,
                });
                event_bus.publish(DomainEvent::HighSpendPatternDetected {
                    employee_id: emp_id.clone(),
                    total_spend,
                    department:  dept.clone(),
                });
            }

            // Pattern 2: Category concentration (>60% in one category)
            if total_spend > 0.0 {
                let mut by_cat: std::collections::HashMap<String, f64> =
                    std::collections::HashMap::new();
                for (amount, category) in &tx_amounts {
                    *by_cat.entry(category.clone()).or_default() += amount;
                }
                if let Some((top_cat, top_amt)) = by_cat
                    .iter()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                {
                    let pct = top_amt / total_spend;
                    if pct > 0.60 {
                        let detail = format!(
                            "{} — {:.0}% spend in '{}' (${:.0} of ${:.0})",
                            emp_label, pct * 100.0, top_cat, top_amt, total_spend
                        );
                        patterns.push(DetectedPattern {
                            kind:         PatternKind::CategoryConcentration,
                            entity_id:    emp_id.clone(),
                            entity_label: emp_label.clone(),
                            detail,
                        });
                        event_bus.publish(DomainEvent::CategoryConcentrationDetected {
                            employee_id: emp_id.clone(),
                            category:    top_cat.clone(),
                            percent:     pct,
                        });
                    }
                }
            }
        }

        patterns.sort_by_key(|p| if p.kind == PatternKind::HighSpend { 0u8 } else { 1 });
        patterns
    }
}
