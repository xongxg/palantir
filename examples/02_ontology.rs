//! Example 2 — Palantir Ontology: Discovery · Bounded Context · Pattern Detection · JSON Export
//!
//! Run:  cargo run --example 02_ontology
//!
//! Demonstrates the full Ontology pipeline:
//!
//!   Repos (in-memory)
//!     └─▶ Dataset (anti-corruption layer)
//!           └─▶ DiscoveryEngine (3-pass schema-free scan)
//!                 ├─▶ OntologyGraph  ──▶  derive_actions
//!                 ├─▶ BoundedContextDetector (Union-Find)
//!                 ├─▶ PatternDetector  ──▶  DomainEvent (semantic observer)
//!                 └─▶ JsonExporter  ──▶  ontology_graph.json (Published Language)

use palantir::application::{
    action::derive_actions,
    ontology::{
        bounded_context::BoundedContextDetector,
        ddd_mapping::DddMapping,
        discovery::DiscoveryEngine,
        graph::OntologyGraph,
        pattern_detector::PatternDetector,
    },
    queries::{employees_dataset, transactions_dataset},
};
use palantir::demo_setup::build_repos;
use palantir::domain::finance::TransactionRepository;
use palantir::domain::organization::EmployeeRepository;
use palantir::infrastructure::export::JsonExporter;
use palantir::interface::{
    render_bounded_context, render_event_loop, render_json_export, render_ontology,
};

fn main() {
    let mut demo = build_repos();

    // ── Build Ontology graph from in-memory repos ─────────────────────────────
    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║              PALANTIR ONTOLOGY  —  DIGITAL TWIN VIEW             ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");

    let datasets = vec![
        employees_dataset(&demo.emp_repo),
        transactions_dataset(&demo.tx_repo),
    ];
    let (objects, relationships) = DiscoveryEngine::discover(&datasets);
    println!(
        "\n  Discovered {} entities and {} relationships.",
        objects.len(),
        relationships.len()
    );

    let graph   = OntologyGraph::build(objects, relationships);
    let actions = derive_actions(&graph);
    render_ontology(&graph, &actions);

    // ── Bounded Context detection ─────────────────────────────────────────────
    let context_map = BoundedContextDetector::detect(&graph);
    render_bounded_context(&context_map);

    // ── Pattern detection → Domain Event loop ────────────────────────────────
    let patterns = PatternDetector::scan(&graph, &mut demo.event_bus, 2_000.0);
    render_event_loop(&patterns, &demo.event_bus);

    // ── JSON export — Published Language ─────────────────────────────────────
    let ddd_mapping = DddMapping::from_graph(&graph);
    let json        = JsonExporter::export(&graph, &ddd_mapping, &context_map);
    JsonExporter::write(&json, "ontology_graph.json").expect("JSON export failed");
    render_json_export(
        "ontology_graph.json",
        graph.objects.len(),
        graph.relationships.len(),
        context_map.contexts.len(),
        &context_map.shared_kernel.dimensions,
    );
}
