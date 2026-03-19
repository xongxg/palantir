use crate::domain::{
    errors::DomainError,
    events::{DomainEvent, EventBus},
    finance::{Category, Transaction, TransactionId, TransactionRepository},
    money::Money,
    organization::{DepartmentName, Employee, EmployeeId, EmployeeLevel, EmployeeRepository},
};

// ─── Hire Employee ────────────────────────────────────────────────────────────

pub struct HireEmployeeCommand {
    pub id: String,
    pub name: String,
    pub department: String,
    pub salary: f64,
    pub level: String,
}

pub fn hire_employee(
    cmd: HireEmployeeCommand,
    repo: &mut dyn EmployeeRepository,
    bus: &mut EventBus,
) -> Result<(), DomainError> {
    let (employee, event) = Employee::hire(
        EmployeeId::new(cmd.id),
        cmd.name,
        DepartmentName::new(cmd.department),
        Money::new(cmd.salary)?,
        EmployeeLevel::from_str(&cmd.level)?,
    )?;
    repo.save(employee);
    bus.publish(DomainEvent::EmployeeHired(event));
    Ok(())
}

// ─── File Transaction ─────────────────────────────────────────────────────────

pub struct FileTransactionCommand {
    pub id: String,
    pub employee_id: String,
    pub amount: f64,
    pub category: String,
}

pub fn file_transaction(
    cmd: FileTransactionCommand,
    repo: &mut dyn TransactionRepository,
    bus: &mut EventBus,
) -> Result<(), DomainError> {
    let (tx, event) = Transaction::file(
        TransactionId::new(cmd.id),
        EmployeeId::new(cmd.employee_id),
        Money::new(cmd.amount)?,
        Category::new(cmd.category),
    );
    repo.save(tx);
    bus.publish(DomainEvent::TransactionFiled(event));
    Ok(())
}

// ─── Flag High-Value Transactions (domain service) ───────────────────────────

pub struct FlagHighValueCommand {
    pub threshold: f64,
}

pub fn flag_high_value_transactions(
    cmd: FlagHighValueCommand,
    repo: &mut dyn TransactionRepository,
    bus: &mut EventBus,
) -> usize {
    // Collect IDs first (drops the immutable borrow on repo)
    let ids_to_flag: Vec<String> = repo
        .find_all()
        .iter()
        .filter(|t| t.is_high_value(cmd.threshold))
        .map(|t| t.id.as_str().to_string())
        .collect();

    let count = ids_to_flag.len();

    for id_str in ids_to_flag {
        let tx_id = TransactionId::new(&id_str);
        // Collect the clone before the mutable borrow
        let updated = repo
            .find_by_id(&tx_id)
            .map(|t| {
                let mut clone = t.clone();
                clone
                    .flag(format!("amount exceeds ${:.0} threshold", cmd.threshold))
                    .ok()
                    .map(|evt| (clone, evt))
            })
            .flatten();

        if let Some((tx, event)) = updated {
            repo.save(tx);
            bus.publish(DomainEvent::TransactionFlagged(event));
        }
    }

    count
}
