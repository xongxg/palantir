//! Application Layer
//!
//! DDD Layer: Application — orchestrates the domain, coordinates infrastructure,
//! implements use cases. Contains NO business rules (those live in domain/).
//!
//! Sub-modules:
//!   commands  — write side: Command DTOs + handlers (hire, file, flag)
//!   queries   — read side: CQRS query handlers + read models (DeptSpend, TopEarner, …)
//!   action    — derives Palantir actions from the ontology graph
//!   ontology/ — ontology application services (discovery, graph, pattern detection)

pub mod action;
pub mod commands;
pub mod event_sourcing;
pub mod logic;
pub mod ontology;
pub mod queries;
pub mod timeline;
pub mod saga;
pub mod workflow;
