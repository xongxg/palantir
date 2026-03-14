//! Infrastructure: Data Pipeline Engine
//!
//! DDD Layer: Infrastructure — technical capability shared across layers.
//! Provides the ETL/transform framework (Dataset, Record, Value, Pipeline).
//! No domain knowledge; purely a data-processing tool.

pub mod dataset;
pub mod transforms;
