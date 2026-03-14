use super::{money::Money, organization::EmployeeId, finance::TransactionId};

// ─── Domain Events ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EmployeeHired {
    pub employee_id: EmployeeId,
    pub name: String,
    pub department: String,
    pub salary: Money,
}

#[derive(Debug, Clone)]
pub struct TransactionFiled {
    pub transaction_id: TransactionId,
    pub employee_id: EmployeeId,
    pub amount: Money,
    pub category: String,
}

#[derive(Debug, Clone)]
pub struct TransactionFlagged {
    pub transaction_id: TransactionId,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub enum DomainEvent {
    // ── Standard DDD domain events ─────────────────────────────────────────
    EmployeeHired(EmployeeHired),
    TransactionFiled(TransactionFiled),
    TransactionFlagged(TransactionFlagged),

    // ── Ontology-triggered events (C: Pattern Detector → Event Loop) ───────
    //
    // These are emitted by the Ontology PatternDetector when it finds a
    // semantic pattern in the entity graph.  Application Services listen
    // and dispatch the appropriate Commands back into the Domain layer.
    // This closes the loop:  Ontology → DomainEvent → ApplicationService → Command.

    /// Employee total spend exceeds threshold → triggers FlagHighValueCommand.
    HighSpendPatternDetected {
        employee_id:  String,
        total_spend:  f64,
        department:   String,
    },

    /// Employee's spend is concentrated (>60%) in one category → policy review.
    CategoryConcentrationDetected {
        employee_id: String,
        category:    String,
        percent:     f64,
    },
}

impl DomainEvent {
    pub fn name(&self) -> &str {
        match self {
            Self::EmployeeHired(_)                   => "EmployeeHired",
            Self::TransactionFiled(_)                => "TransactionFiled",
            Self::TransactionFlagged(_)              => "TransactionFlagged",
            Self::HighSpendPatternDetected { .. }    => "HighSpendPatternDetected",
            Self::CategoryConcentrationDetected { .. } => "CategoryConcentrationDetected",
        }
    }

    pub fn is_ontology_triggered(&self) -> bool {
        matches!(
            self,
            Self::HighSpendPatternDetected { .. } | Self::CategoryConcentrationDetected { .. }
        )
    }
}

// ─── Event Bus ────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct EventBus {
    events: Vec<DomainEvent>,
}

impl EventBus {
    pub fn publish(&mut self, event: DomainEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[DomainEvent] { &self.events }
}
