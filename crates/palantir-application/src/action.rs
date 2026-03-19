//! Application: Action Derivation Service
//!
//! DDD Layer: Application — derives actionable operations from the ontology
//! graph's relationship patterns. Maps Palantir action categories to DDD patterns.
//!
//! Relationship kind → Palantir action → DDD pattern:
//!   BelongsTo → Logic       → Domain Service
//!   Has       → Integration → Repository + ACL
//!   LinkedTo  → Workflow    → Application Service (Command)
//!   SimilarTo → Search      → Query Handler (CQRS read side)

use super::ontology::graph::OntologyGraph;

#[derive(Debug, Clone)]
pub struct ActionSummary {
    pub category: &'static str,
    pub description: String,
}

pub fn derive_actions(graph: &OntologyGraph) -> Vec<ActionSummary> {
    let mut actions: Vec<ActionSummary> = Vec::new();

    for (from_type, kind, to_type, count) in graph.relationship_patterns() {
        let (category, description) = match kind.as_str() {
            "BELONGS_TO" => (
                "Logic",
                format!(
                    "Aggregate {} by [{}] → compute group metrics  ({} links)",
                    from_type, to_type, count
                ),
            ),
            "HAS" => (
                "Integration",
                format!(
                    "Join {} ──▶ {} via foreign key  ({} links)",
                    from_type, to_type, count
                ),
            ),
            "LINKED_TO" => (
                "Workflow",
                format!(
                    "Trigger review: {} ──▶ {}  ({} triggers)",
                    from_type, to_type, count
                ),
            ),
            "SIMILAR_TO" => (
                "Search",
                format!(
                    "Find similar {} by shared {}  ({} pairs)",
                    from_type, to_type, count
                ),
            ),
            _ => continue,
        };
        actions.push(ActionSummary {
            category,
            description,
        });
    }

    let n_emp = graph.objects_by_type("Employee").len();
    let n_tx = graph.objects_by_type("Transaction").len();

    if n_emp > 0 {
        actions.push(ActionSummary {
            category: "Logic",
            description: format!(
                "Compute salary bands for {} employees across department groups",
                n_emp
            ),
        });
        actions.push(ActionSummary {
            category: "Workflow",
            description: format!(
                "Flag high-value transactions (>$500) for manager approval  ({} candidates)",
                n_tx
            ),
        });
        actions.push(ActionSummary {
            category: "Search",
            description: "Find employees with similar spend patterns within the same department"
                .to_string(),
        });
    }

    actions.sort_by_key(|a| match a.category {
        "Logic" => 0u8,
        "Integration" => 1,
        "Workflow" => 2,
        "Search" => 3,
        _ => 4,
    });
    actions
}
