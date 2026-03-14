//! Infrastructure Layer
//!
//! DDD Layer: Infrastructure — implements ports defined in domain/application,
//! provides technical capabilities (persistence, pipeline, I/O adapters).
//! Everything here is swappable without touching domain or application logic.
//!
//! Sub-modules:
//!   persistence/  — repository implementations (in-memory, database, …)
//!   pipeline/     — ETL pipeline engine (Dataset, Record, transforms)
//!   datasource    — inbound adapters: CSV, database, stream, …
//!   export        — outbound adapters: JSON (Published Language), …

pub mod datasource;
pub mod event_store;
pub mod export;
pub mod persistence;
pub mod pipeline;
