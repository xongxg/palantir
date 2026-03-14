//! Example 1 — DDD Core: Commands · Domain Events · CQRS Queries
//!
//! Run:  cargo run --example 01_ddd_core
//!
//! Demonstrates the standard four DDD building blocks wired together:
//!
//!   HireEmployeeCommand / FileTransactionCommand
//!     └─▶ Domain (Employee Aggregate, Transaction Entity)
//!           └─▶ DomainEvent published to EventBus
//!                 └─▶ CQRS Query handlers read from repos

use palantir::application::queries::{
    query_dept_spend_summary, query_high_value_transactions, query_top_earners,
};
use palantir::demo_setup::build_repos;
use palantir::domain::finance::TransactionRepository;
use palantir::domain::organization::EmployeeRepository;

fn main() {
    let mut demo = build_repos();
    let emp_repo  = &demo.emp_repo;
    let tx_repo   = &demo.tx_repo;
    let event_bus = &mut demo.event_bus;

    // ── Summary ──────────────────────────────────────────────────────────────
    println!("═══ Commands ════════════════════════════════════════════");
    println!("  {} employees hired", emp_repo.find_all().len());
    println!("  {} transactions filed", tx_repo.find_all().len());
    println!("  (8 employees × 15 transactions, high-value threshold $500)");
    println!();

    // ── Domain Events ────────────────────────────────────────────────────────
    println!("═══ Domain Events ════════════════════════════════════════");
    for event in event_bus.events() {
        println!("  [{}]", event.name());
    }
    println!("  ({} events total)", event_bus.events().len());
    println!();

    // ── CQRS Queries ─────────────────────────────────────────────────────────
    println!("═══ Query: Department Spend Summary ═════════════════════");
    print_header(&["department", "total_spend", "tx_count", "avg_tx", "largest_tx"]);
    for r in &query_dept_spend_summary(emp_repo, tx_repo) {
        println!(
            "  {:<14} {:<12.2} {:<9} {:<10.2} {:.2}",
            r.department, r.total_spend, r.tx_count, r.avg_tx, r.largest_tx
        );
    }
    println!();

    println!("═══ Query: Top Earners ══════════════════════════════════");
    print_header(&["name", "department", "level", "salary"]);
    for r in &query_top_earners(emp_repo) {
        println!(
            "  {:<15} {:<14} {:<8} {:.2}",
            r.name, r.department, r.level, r.salary
        );
    }
    println!();

    println!("═══ Query: High-Value Transactions (>$500) ══════════════");
    print_header(&["employee", "department", "category", "amount"]);
    for r in &query_high_value_transactions(500.0, emp_repo, tx_repo) {
        println!(
            "  {:<15} {:<14} {:<22} {:.2}",
            r.employee, r.department, r.category, r.amount
        );
    }
    println!();
}

fn print_header(cols: &[&str]) {
    println!("  {}", cols.join("  "));
    println!(
        "  {}",
        cols.iter()
            .map(|c| "─".repeat(c.len()))
            .collect::<Vec<_>>()
            .join("  ")
    );
}
