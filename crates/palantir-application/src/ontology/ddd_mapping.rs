//! Maps discovered Ontology concepts onto their DDD equivalents.
//!
//! DDD defines HOW to structure code (layers, aggregates, events).
//! The Ontology defines WHAT the domain means (objects, relationships, actions).
//! This module makes that bridge explicit.

use std::collections::HashSet;

use super::{graph::OntologyGraph, relationship::RelationshipKind};

// ─── DDD concept taxonomy ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DddLayer {
    Domain,
    Application,
    Infrastructure,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DddConcept {
    // Domain layer
    AggregateRoot,  // owns child entities; no parent owner
    Entity,         // has identity; owned by an aggregate
    ValueObject,    // grouping dimension; identity-free, value-based

    // Application layer
    DomainService,      // Logic  — cross-entity computation within domain
    ApplicationService, // Workflow — command handler, orchestrates domain ops
    QueryHandler,       // Search  — CQRS read side

    // Infrastructure layer
    Repository,             // Integration — data access port
    AntiCorruptionLayer,    // Dataset adapter — keeps domain model pure
}

impl DddConcept {
    pub fn label(&self) -> &str {
        match self {
            Self::AggregateRoot         => "Aggregate Root",
            Self::Entity                => "Entity",
            Self::ValueObject           => "Value Object",
            Self::DomainService         => "Domain Service",
            Self::ApplicationService    => "Application Service",
            Self::QueryHandler          => "Query Handler",
            Self::Repository            => "Repository",
            Self::AntiCorruptionLayer   => "Anti-Corruption Layer",
        }
    }

    pub fn layer(&self) -> DddLayer {
        match self {
            Self::AggregateRoot | Self::Entity | Self::ValueObject
            | Self::DomainService => DddLayer::Domain,

            Self::ApplicationService | Self::QueryHandler => DddLayer::Application,

            Self::Repository | Self::AntiCorruptionLayer => DddLayer::Infrastructure,
        }
    }
}

// ─── Classification result ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ObjectClassification {
    pub object_type: String,
    pub concept:     DddConcept,
    pub reason:      &'static str,
}

#[derive(Debug, Clone)]
pub struct ActionClassification {
    pub relationship_kind: String, // "BELONGS_TO" | "HAS" | "LINKED_TO" | "SIMILAR_TO"
    pub concept:           DddConcept,
    pub palantir_action:   &'static str,
    pub ddd_pattern:       &'static str,
}

#[derive(Debug)]
pub struct DddMapping {
    pub objects: Vec<ObjectClassification>,
    pub actions: Vec<ActionClassification>,
}

// ─── Classification logic ─────────────────────────────────────────────────────

impl DddMapping {
    pub fn from_graph(graph: &OntologyGraph) -> Self {
        let objects = classify_objects(graph);
        let actions = classify_actions();
        Self { objects, actions }
    }
}

fn classify_objects(graph: &OntologyGraph) -> Vec<ObjectClassification> {
    // Which object types appear as the "from" side of HAS (they own children)
    let mut owns_children: HashSet<String> = HashSet::new();
    // Which object types appear as the "to" side of HAS (they are owned)
    let mut is_owned: HashSet<String> = HashSet::new();
    // Which object types appear only as BELONGS_TO targets (grouping dimensions)
    let mut grouping_dims: HashSet<String> = HashSet::new();

    for rel in &graph.relationships {
        match rel.kind {
            RelationshipKind::Has => {
                owns_children.insert(rel.from_type.0.clone());
                is_owned.insert(rel.to_type.0.clone());
            }
            RelationshipKind::BelongsTo => {
                grouping_dims.insert(rel.to_type.0.clone());
            }
            _ => {}
        }
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<ObjectClassification> = Vec::new();

    // Classify concrete entity types
    for obj in &graph.objects {
        let t = &obj.object_type.0;
        if !seen.insert(t.clone()) { continue; }

        let (concept, reason) = match (owns_children.contains(t), is_owned.contains(t)) {
            (true, false) => (
                DddConcept::AggregateRoot,
                "owns child entities via HAS; no parent — enforce invariants here",
            ),
            (false, true) => (
                DddConcept::Entity,
                "has stable identity; lifecycle controlled by its Aggregate Root",
            ),
            (true, true) => (
                DddConcept::Entity,
                "has identity; participates as both owner and owned (nested aggregate)",
            ),
            (false, false) => (
                DddConcept::Entity,
                "has identity; standalone entity with no ownership edges",
            ),
        };
        out.push(ObjectClassification { object_type: t.clone(), concept, reason });
    }

    // Classify grouping dimensions (Value Objects)
    for dim in &grouping_dims {
        if seen.insert(dim.clone()) {
            out.push(ObjectClassification {
                object_type: dim.clone(),
                concept: DddConcept::ValueObject,
                reason: "no independent identity — equality defined by value, not ID",
            });
        }
    }

    out.sort_by_key(|c| match c.concept.layer() {
        DddLayer::Domain         => 0u8,
        DddLayer::Application    => 1,
        DddLayer::Infrastructure => 2,
    });
    out
}

fn classify_actions() -> Vec<ActionClassification> {
    vec![
        ActionClassification {
            relationship_kind: "BELONGS_TO".into(),
            concept:           DddConcept::DomainService,
            palantir_action:   "Logic",
            ddd_pattern:       "Domain Service — stateless cross-entity computation",
        },
        ActionClassification {
            relationship_kind: "HAS".into(),
            concept:           DddConcept::Repository,
            palantir_action:   "Integration",
            ddd_pattern:       "Repository + ACL — join across bounded contexts",
        },
        ActionClassification {
            relationship_kind: "LINKED_TO".into(),
            concept:           DddConcept::ApplicationService,
            palantir_action:   "Workflow",
            ddd_pattern:       "Application Service — command handler, dispatches domain ops",
        },
        ActionClassification {
            relationship_kind: "SIMILAR_TO".into(),
            concept:           DddConcept::QueryHandler,
            palantir_action:   "Search",
            ddd_pattern:       "Query Handler — CQRS read side, no domain state mutation",
        },
    ]
}
