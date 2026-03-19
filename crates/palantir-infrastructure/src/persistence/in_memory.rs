use std::collections::HashMap;

use crate::domain::{
    finance::{Transaction, TransactionId, TransactionRepository},
    organization::{Employee, EmployeeId, EmployeeRepository},
};

// ─── Employee repository ──────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct InMemoryEmployeeRepo {
    store: HashMap<String, Employee>,
}

impl EmployeeRepository for InMemoryEmployeeRepo {
    fn save(&mut self, employee: Employee) {
        self.store
            .insert(employee.id.as_str().to_string(), employee);
    }

    fn find_by_id(&self, id: &EmployeeId) -> Option<&Employee> {
        self.store.get(id.as_str())
    }

    fn find_all(&self) -> Vec<&Employee> {
        self.store.values().collect()
    }
}

// ─── Transaction repository ───────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct InMemoryTransactionRepo {
    store: HashMap<String, Transaction>,
}

impl TransactionRepository for InMemoryTransactionRepo {
    fn save(&mut self, tx: Transaction) {
        self.store.insert(tx.id.as_str().to_string(), tx);
    }

    fn find_by_id(&self, id: &TransactionId) -> Option<&Transaction> {
        self.store.get(id.as_str())
    }

    fn find_all(&self) -> Vec<&Transaction> {
        self.store.values().collect()
    }

    fn find_by_employee(&self, employee_id: &EmployeeId) -> Vec<&Transaction> {
        self.store
            .values()
            .filter(|t| &t.employee_id == employee_id)
            .collect()
    }
}
