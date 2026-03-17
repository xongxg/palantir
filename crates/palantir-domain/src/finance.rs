use super::{
    errors::DomainError,
    events::{TransactionFiled, TransactionFlagged},
    money::Money,
    organization::EmployeeId,
};

// ─── Value Objects ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransactionId(pub String);

impl TransactionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category(pub String);

impl Category {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ─── Entity ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Approved,
    Flagged,
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: TransactionId,
    pub employee_id: EmployeeId,
    pub amount: Money,
    pub category: Category,
    pub status: TransactionStatus,
}

impl Transaction {
    /// Factory method — creates a Pending transaction and emits domain event.
    pub fn file(
        id: TransactionId,
        employee_id: EmployeeId,
        amount: Money,
        category: Category,
    ) -> (Self, TransactionFiled) {
        let tx = Self {
            id: id.clone(),
            employee_id: employee_id.clone(),
            amount: amount.clone(),
            category: category.clone(),
            status: TransactionStatus::Pending,
        };
        let event = TransactionFiled {
            transaction_id: id,
            employee_id,
            amount,
            category: category.to_string(),
        };
        (tx, event)
    }

    /// Business rule: only Pending transactions can be flagged.
    pub fn flag(&mut self, reason: String) -> Result<TransactionFlagged, DomainError> {
        if self.status != TransactionStatus::Pending {
            return Err(DomainError::InvalidOperation(format!(
                "transaction {} is not Pending",
                self.id
            )));
        }
        self.status = TransactionStatus::Flagged;
        Ok(TransactionFlagged {
            transaction_id: self.id.clone(),
            reason,
        })
    }

    pub fn is_high_value(&self, threshold: f64) -> bool {
        self.amount.amount() > threshold
    }
}

// ─── Repository trait (port) ──────────────────────────────────────────────────

pub trait TransactionRepository {
    fn save(&mut self, tx: Transaction);
    fn find_by_id(&self, id: &TransactionId) -> Option<&Transaction>;
    fn find_all(&self) -> Vec<&Transaction>;
    fn find_by_employee(&self, employee_id: &EmployeeId) -> Vec<&Transaction>;
}
