pub mod action;
pub mod commands;
pub mod logic;
pub mod ontology;
pub mod queries;
pub mod timeline;
pub mod workflow;

// Compatibility re-exports so included source paths like `crate::domain::*`
// and `crate::infrastructure::pipeline::*` keep compiling in this crate.
pub use palantir_domain as domain;
pub mod infrastructure { pub use palantir_pipeline as pipeline; }
