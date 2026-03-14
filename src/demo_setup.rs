//! Shared demo data — used by all examples.
//!
//! Provides a single `build_repos()` that fires the standard set of Commands
//! (8 employees, 15 transactions, high-value flag) and returns populated
//! repositories + EventBus, ready for any example to query or visualise.

use crate::application::commands::{
    FileTransactionCommand, FlagHighValueCommand, HireEmployeeCommand,
    file_transaction, flag_high_value_transactions, hire_employee,
};
use crate::domain::events::EventBus;
use crate::infrastructure::persistence::in_memory::{
    InMemoryEmployeeRepo, InMemoryTransactionRepo,
};

pub struct DemoRepos {
    pub emp_repo:  InMemoryEmployeeRepo,
    pub tx_repo:   InMemoryTransactionRepo,
    pub event_bus: EventBus,
}

/// Build and return populated in-memory repositories.
///
/// Fires commands silently (no stdout), so each example can render output
/// in whatever style it prefers.
pub fn build_repos() -> DemoRepos {
    let mut emp_repo  = InMemoryEmployeeRepo::default();
    let mut tx_repo   = InMemoryTransactionRepo::default();
    let mut event_bus = EventBus::default();

    for (id, name, dept, salary, level) in [
        ("e1", "Alice Chen",   "Engineering",  120_000.0, "Senior"),
        ("e2", "Bob Martinez", "Engineering",   95_000.0, "Mid"),
        ("e3", "Carol White",  "Sales",          85_000.0, "Senior"),
        ("e4", "David Kim",    "Sales",           72_000.0, "Mid"),
        ("e5", "Eva Patel",    "Marketing",      90_000.0, "Senior"),
        ("e6", "Frank Lee",    "Marketing",      68_000.0, "Junior"),
        ("e7", "Grace Nguyen", "Engineering",   145_000.0, "Staff"),
        ("e8", "Henry Brown",  "Operations",     78_000.0, "Mid"),
    ] {
        hire_employee(
            HireEmployeeCommand {
                id: id.into(), name: name.into(),
                department: dept.into(), salary,
                level: level.into(),
            },
            &mut emp_repo,
            &mut event_bus,
        ).expect("hire_employee failed");
    }

    for (id, emp_id, amount, category) in [
        ("t01", "e1", 1_200.0, "Software"),
        ("t02", "e1",   350.0, "Travel"),
        ("t03", "e2",   800.0, "Hardware"),
        ("t04", "e3", 2_500.0, "Client Entertainment"),
        ("t05", "e3",   450.0, "Office Supplies"),
        ("t06", "e4",   600.0, "Travel"),
        ("t07", "e5", 3_100.0, "Marketing Campaign"),
        ("t08", "e5",   200.0, "Office Supplies"),
        ("t09", "e6",   950.0, "Marketing Campaign"),
        ("t10", "e7", 5_000.0, "Software"),
        ("t11", "e7",   120.0, "Office Supplies"),
        ("t12", "e8",   700.0, "Equipment"),
        ("t13", "e8",   430.0, "Travel"),
        ("t14", "e2",   525.0, "Training"),
        ("t15", "e4",   180.0, "Travel"),
    ] {
        file_transaction(
            FileTransactionCommand {
                id: id.into(), employee_id: emp_id.into(),
                amount, category: category.into(),
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
