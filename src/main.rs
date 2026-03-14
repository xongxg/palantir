//! Palantir — Palantir Ontology + DDD architecture demo
//!
//! All demonstrations have been moved to the `examples/` directory.
//! Run them individually:
//!
//!   cargo run --example 01_ddd_core          # Commands · Domain Events · CQRS Queries
//!   cargo run --example 02_ontology          # Ontology discovery · BC detection · Pattern → Event · JSON export
//!   cargo run --example 03_csv_adapter       # Infrastructure adapter (CSV ↔ in-memory swap)
//!   cargo run --example 04_logic_and_workflow # Logic calculations + Workflow orchestration

mod application;
mod domain;
mod infrastructure;
mod interface;

fn main() {
    println!("Palantir — DDD + Ontology demo");
    println!();
    println!("Run an example:");
    println!("  cargo run --example 01_ddd_core           — Commands · Events · CQRS Queries");
    println!("  cargo run --example 02_ontology           — Ontology · BC · Pattern detection · JSON");
    println!("  cargo run --example 03_csv_adapter        — Infrastructure adapter (CSV)");
    println!("  cargo run --example 04_logic_and_workflow — Logic calculations + Workflow orchestration");
    println!("  cargo run --example 05_complex_discovery  — 10 entity types, 77 records, 251 relationships");
}
