//! Shared demo data — used by all examples.
//!
//! Provides a single `build_repos()` that loads employees + transactions from
//! CSV files in `data/`, fires the corresponding Commands, and returns
//! populated repositories + EventBus ready for any example to query or visualise.
//!
//! Data files:
//!   data/employees.csv     — id, name, department, salary, level
//!   data/transactions.csv  — id, employee_id, amount, category

use crate::application::commands::{
    FileTransactionCommand, FlagHighValueCommand, HireEmployeeCommand,
    file_transaction, flag_high_value_transactions, hire_employee,
};
use crate::domain::events::EventBus;
use crate::infrastructure::persistence::in_memory::{
    InMemoryEmployeeRepo, InMemoryTransactionRepo,
};

const EMPLOYEES_CSV:    &str = "data/employees.csv";
const TRANSACTIONS_CSV: &str = "data/transactions.csv";

pub struct DemoRepos {
    pub emp_repo:  InMemoryEmployeeRepo,
    pub tx_repo:   InMemoryTransactionRepo,
    pub event_bus: EventBus,
}

/// Build and return populated in-memory repositories loaded from CSV files.
///
/// Fires commands silently (no stdout), so each example can render output
/// in whatever style it prefers.
pub fn build_repos() -> DemoRepos {
    let mut emp_repo  = InMemoryEmployeeRepo::default();
    let mut tx_repo   = InMemoryTransactionRepo::default();
    let mut event_bus = EventBus::default();

    // ── Employees ────────────────────────────────────────────────────────────
    let emp_csv = std::fs::read_to_string(EMPLOYEES_CSV)
        .unwrap_or_else(|e| panic!("Cannot read {EMPLOYEES_CSV}: {e}"));

    for line in emp_csv.lines().skip(1) {
        let c: Vec<&str> = line.splitn(5, ',').collect();
        if c.len() < 5 { continue; }
        hire_employee(
            HireEmployeeCommand {
                id:         c[0].trim().into(),
                name:       c[1].trim().into(),
                department: c[2].trim().into(),
                salary:     c[3].trim().parse().unwrap_or(0.0),
                level:      c[4].trim().into(),
            },
            &mut emp_repo,
            &mut event_bus,
        ).expect("hire_employee failed");
    }

    // ── Transactions ─────────────────────────────────────────────────────────
    let tx_csv = std::fs::read_to_string(TRANSACTIONS_CSV)
        .unwrap_or_else(|e| panic!("Cannot read {TRANSACTIONS_CSV}: {e}"));

    for line in tx_csv.lines().skip(1) {
        let c: Vec<&str> = line.splitn(4, ',').collect();
        if c.len() < 4 { continue; }
        file_transaction(
            FileTransactionCommand {
                id:          c[0].trim().into(),
                employee_id: c[1].trim().into(),
                amount:      c[2].trim().parse().unwrap_or(0.0),
                category:    c[3].trim().into(),
            },
            &mut tx_repo,
            &mut event_bus,
        ).expect("file_transaction failed");
    }

    flag_high_value_transactions(
        FlagHighValueCommand { threshold: 500.0 },
        &mut tx_repo,
        &mut event_bus,
    );

    DemoRepos { emp_repo, tx_repo, event_bus }
}
