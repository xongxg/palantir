//! Example 4 — Logic (Calculations) + Workflow (Actions)
//!
//! Run:  cargo run --example 04_logic_and_workflow
//!
//! Demonstrates the two Palantir action types that were previously only labels:
//!
//!   Logic (Calculations)
//!   ────────────────────
//!   Stateless computations over ontology entities.
//!   Domain rules (salary_band, spend_ratio, concentration_ratio) applied
//!   across all Employee + Transaction objects in the graph.
//!
//!     OntologyGraph
//!       └─▶ application::logic::calc_*()       ← Application Service
//!             └─▶ domain::calculations::*()     ← Domain Service (pure rules)
//!                   └─▶ derived metrics
//!
//!   Workflow (Actions)
//!   ──────────────────
//!   Multi-step orchestrated processes triggered by ontology-detected patterns.
//!   Each step produces a concrete, auditable output.
//!
//!     PatternDetector → DomainEvent → EventBus
//!       └─▶ application::workflow::run_*()     ← Application Service
//!             └─▶ WorkflowRun { ordered steps with real outputs }

use palantir::application::{
    logic::{calc_category_concentration, calc_salary_bands, calc_spend_metrics},
    ontology::{discovery::DiscoveryEngine, graph::OntologyGraph},
    queries::{employees_dataset, transactions_dataset},
    workflow::{run_high_spend_approval, run_spend_policy_review},
};
use palantir::demo_setup::build_repos;
use palantir::domain::finance::TransactionRepository;
use palantir::domain::organization::EmployeeRepository;

fn main() {
    let demo = build_repos();

    // Build OntologyGraph (same as example 02)
    let datasets = vec![
        employees_dataset(&demo.emp_repo),
        transactions_dataset(&demo.tx_repo),
    ];
    let (objects, relationships) = DiscoveryEngine::discover(&datasets);
    let graph = OntologyGraph::build(objects, relationships);

    // ═══════════════════════════════════════════════════════════════
    //  LOGIC ACTIONS  —  Calculations over the ontology graph
    // ═══════════════════════════════════════════════════════════════

    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║         LOGIC ACTIONS  —  Palantir Calculations                  ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  DDD: Application Service orchestrates Domain Service calculation rules.");
    println!("  Input: OntologyGraph entities.  Output: derived metrics.");
    println!("  Pure functions — same input always produces the same output.");
    println!();

    // ── Logic 1: Salary Bands ────────────────────────────────────────────────
    println!("── [L1] Salary Band Classification ─────────────────────────────────");
    println!("   domain rule: calculations::salary_band(salary) → band label");
    println!();
    for group in calc_salary_bands(&graph) {
        println!("   {}", group.band);
        for name in &group.members {
            println!("     · {}", name);
        }
    }
    println!();

    // ── Logic 2: Spend Metrics ───────────────────────────────────────────────
    println!("── [L2] Per-Employee Spend Metrics ──────────────────────────────────");
    println!("   domain rule: calculations::spend_ratio_pct(spend, salary)");
    println!("                calculations::expense_risk_level(ratio, concentration)");
    println!();
    println!("   {:<15} {:<13} {:<10} {:<8} {:<8} {:<6}",
        "name", "department", "salary", "spend", "ratio%", "risk");
    println!("   {}", "─".repeat(66));
    for m in calc_spend_metrics(&graph) {
        println!(
            "   {:<15} {:<13} ${:<9.0} ${:<7.0} {:<8.2} {}",
            trunc(&m.name, 15), trunc(&m.department, 13),
            m.annual_salary, m.total_spend, m.spend_ratio_pct, m.risk_level
        );
    }
    println!();

    // ── Logic 3: Category Concentration ─────────────────────────────────────
    println!("── [L3] Category Concentration Analysis ─────────────────────────────");
    println!("   domain rule: calculations::concentration_ratio(top_amt, total)");
    println!();
    println!("   {:<15} {:<22} {:<8} {:<10} {:<6}",
        "name", "top_category", "top_amt", "total", "conc%");
    println!("   {}", "─".repeat(66));
    for c in calc_category_concentration(&graph) {
        println!(
            "   {:<15} {:<22} ${:<7.0} ${:<9.0} {:.1}%",
            trunc(&c.name, 15), trunc(&c.top_category, 22),
            c.top_amount, c.total_spend, c.concentration_pct
        );
    }
    println!();

    // ═══════════════════════════════════════════════════════════════
    //  WORKFLOW ACTIONS  —  Orchestrated multi-step processes
    // ═══════════════════════════════════════════════════════════════

    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║         WORKFLOW ACTIONS  —  Palantir Orchestration              ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  DDD: Application Service executes ordered commands in response to");
    println!("  ontology-detected patterns.  Each step is concrete and auditable.");
    println!();

    // Trigger workflows for employees whose metrics breach thresholds
    let spend_threshold = 2_000.0_f64;
    let conc_threshold  = 60.0_f64;

    let metrics      = calc_spend_metrics(&graph);
    let high_spenders: Vec<_> = metrics.iter()
        .filter(|m| m.total_spend > spend_threshold)
        .collect();

    let concentrations = calc_category_concentration(&graph);
    let concentrated: Vec<_> = concentrations.iter()
        .filter(|c| c.concentration_pct > conc_threshold)
        .collect();

    println!("── [W1] HighSpendApprovalWorkflow ───────────────────────────────────");
    println!("   Triggered for {} employee(s) with spend > ${:.0}", high_spenders.len(), spend_threshold);
    println!();

    for m in &high_spenders {
        let run = run_high_spend_approval(
            &m.employee_id, &m.name, &m.department, m.total_spend, spend_threshold,
        );
        render_workflow(&run);
    }

    println!("── [W2] SpendPolicyReviewWorkflow ───────────────────────────────────");
    println!("   Triggered for {} employee(s) with category concentration > {:.0}%",
        concentrated.len(), conc_threshold);
    println!();

    for c in &concentrated {
        let run = run_spend_policy_review(
            &c.employee_id, &c.name, "—",
            &c.top_category, c.concentration_pct, c.total_spend,
        );
        render_workflow(&run);
    }
}

// ─── Renderer ─────────────────────────────────────────────────────────────────

fn render_workflow(run: &palantir::application::workflow::WorkflowRun) {
    let status = if run.succeeded() { "COMPLETED" } else { "FAILED" };
    println!("   ┌─ {} [{}]", run.workflow_name, status);
    println!("   │  Trigger: {}", run.trigger);
    println!("   │");
    for step in &run.steps {
        println!("   │  {} {}",  step.outcome.label(), step.name);
        println!("   │      → {}", step.outcome.message());
    }
    println!("   └─");
    println!();
}

fn trunc(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}
