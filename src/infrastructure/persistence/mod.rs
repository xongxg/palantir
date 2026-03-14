//! Infrastructure: Persistence Adapters
//!
//! DDD Layer: Infrastructure — implements repository ports defined in domain.
//! Swappable: replace InMemory with Postgres, Redis, etc. without touching domain.

pub mod in_memory;
