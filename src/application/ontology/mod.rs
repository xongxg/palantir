//! Application: Ontology Services
//!
//! DDD Layer: Application — orchestrates infrastructure (pipeline) to produce
//! a semantic graph of the domain. Does NOT enforce domain invariants;
//! that is the domain layer's responsibility.
//!
//! This is the Palantir Ontology as an Application Service:
//!   discovery      — scans datasets, extracts entities + relationships
//!   graph          — semantic graph API (query nodes/edges)
//!   entity         — generic OntologyObject (wraps a Record with a type)
//!   relationship   — typed edges: BelongsTo, Has, LinkedTo, SimilarTo
//!   bounded_context— detects Bounded Context boundaries from graph topology
//!   pattern_detector — scans graph for business patterns, emits DomainEvents
//!   ddd_mapping    — maps ontology concepts to DDD building blocks

pub mod bounded_context;
pub mod ddd_mapping;
pub mod discovery;
pub mod entity;
pub mod graph;
pub mod pattern_detector;
pub mod relationship;
