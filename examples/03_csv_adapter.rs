//! Example 3 — Infrastructure Adapter: CSV Loader
//!
//! Run:  cargo run --example 03_csv_adapter
//!
//! Demonstrates Hexagonal Architecture / Ports & Adapters:
//!
//!   CsvLoader (Infrastructure)  ←→  same interface as InMemoryRepo
//!     └─▶ Dataset
//!           └─▶ DiscoveryEngine
//!                 └─▶ OntologyGraph
//!
//! The domain and ontology layers are completely unaware of the data source.
//! Swap CsvLoader → DatabaseLoader → KafkaLoader with zero changes to anything above.

use palantir::application::{
    action::derive_actions,
    ontology::{discovery::DiscoveryEngine, graph::OntologyGraph},
    queries::{employees_dataset, transactions_dataset},
};
use palantir::demo_setup::build_repos;
use palantir::domain::finance::TransactionRepository;
use palantir::domain::organization::EmployeeRepository;
use palantir::infrastructure::datasource::CsvLoader;
use palantir::interface::{render_csv_concept, render_ontology};

fn main() {
    // ── In-memory baseline (from Commands) ───────────────────────────────────
    let demo = build_repos();
    let mem_datasets = vec![
        employees_dataset(&demo.emp_repo),
        transactions_dataset(&demo.tx_repo),
    ];
    let (mem_obj, mem_rel) = DiscoveryEngine::discover(&mem_datasets);
    let mem_graph = OntologyGraph::build(mem_obj, mem_rel);

    // ── CSV adapter (Infrastructure) ─────────────────────────────────────────
    let csv_emp = CsvLoader::load("data/employees.csv", "Employee")
        .expect("data/employees.csv not found — run from project root");
    let csv_tx = CsvLoader::load("data/transactions.csv", "Transaction")
        .expect("data/transactions.csv not found — run from project root");

    render_csv_concept(
        "data/employees.csv",
        csv_emp.len(),
        "data/transactions.csv",
        csv_tx.len(),
    );

    // ── Discovery from CSV (same engine, different adapter) ──────────────────
    let csv_datasets = vec![csv_emp, csv_tx];
    let (csv_obj, csv_rel) = DiscoveryEngine::discover(&csv_datasets);

    let matches = csv_obj.len() == mem_graph.objects.len()
        && csv_rel.len() == mem_graph.relationships.len();
    println!(
        "  Verification — CSV discovery: {} entities, {} relationships  (matches in-memory: {})\n",
        csv_obj.len(),
        csv_rel.len(),
        matches
    );

    // ── Render ontology from CSV-sourced graph ────────────────────────────────
    let csv_graph   = OntologyGraph::build(csv_obj, csv_rel);
    let csv_actions = derive_actions(&csv_graph);
    render_ontology(&csv_graph, &csv_actions);
}
