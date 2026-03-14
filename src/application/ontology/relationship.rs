use super::entity::{EntityId, ObjectType};

/// Semantic type of a relationship — maps to a Palantir action category.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelationshipKind {
    /// entity → group dimension  (e.g. Employee → Department)   → Logic
    BelongsTo,
    /// owner → child entity      (e.g. Employee → Transaction)   → Integration
    Has,
    /// cross-type non-FK link                                     → Workflow
    LinkedTo,
    /// same-type similarity                                       → Search
    SimilarTo,
}

impl RelationshipKind {
    pub fn label(&self) -> &str {
        match self {
            Self::BelongsTo => "BELONGS_TO",
            Self::Has       => "HAS",
            Self::LinkedTo  => "LINKED_TO",
            Self::SimilarTo => "SIMILAR_TO",
        }
    }

    pub fn action_category(&self) -> &str {
        match self {
            Self::BelongsTo => "Logic",
            Self::Has       => "Integration",
            Self::LinkedTo  => "Workflow",
            Self::SimilarTo => "Search",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Relationship {
    pub from_id:   EntityId,
    pub from_type: ObjectType,
    pub to_id:     EntityId,
    pub to_type:   ObjectType,
    pub kind:      RelationshipKind,
    pub via_field: String,
}
