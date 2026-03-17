//! Order aggregate — event-sourced.
//!
//! DDD principle: the aggregate has NO mutable repository.
//! Current state is derived exclusively by folding domain events:
//!
//!   OrderState::draft("o01")
//!     .apply(OrderPlaced { … })   → Placed
//!     .apply(PaymentReceived { … }) → Paid
//!     .apply(ItemShipped { … })   → Shipped
//!     .apply(OrderDelivered { … }) → Delivered
//!
//! This makes the event stream the single source of truth.

// ─── Status ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderStatus {
    Draft,
    Placed,
    Paid,
    Shipped,
    Delivered,
    Cancelled,
}

impl OrderStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Placed => "Placed",
            Self::Paid => "Paid",
            Self::Shipped => "Shipped",
            Self::Delivered => "Delivered",
            Self::Cancelled => "Cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Delivered | Self::Cancelled)
    }
}

// ─── Domain Events ────────────────────────────────────────────────────────────

/// Every business fact about an Order is captured as one of these variants.
/// Events are immutable value objects — never changed after creation.
#[derive(Debug, Clone)]
pub enum OrderEvent {
    OrderPlaced {
        order_id: String,
        customer_id: String,
        amount: f64,
    },
    PaymentReceived {
        order_id: String,
        amount: f64,
    },
    ItemShipped {
        order_id: String,
    },
    OrderDelivered {
        order_id: String,
    },
    OrderCancelled {
        order_id: String,
        reason: String,
    },
}

impl OrderEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::OrderPlaced { .. } => "OrderPlaced",
            Self::PaymentReceived { .. } => "PaymentReceived",
            Self::ItemShipped { .. } => "ItemShipped",
            Self::OrderDelivered { .. } => "OrderDelivered",
            Self::OrderCancelled { .. } => "OrderCancelled",
        }
    }

    pub fn order_id(&self) -> &str {
        match self {
            Self::OrderPlaced { order_id, .. } => order_id,
            Self::PaymentReceived { order_id, .. } => order_id,
            Self::ItemShipped { order_id } => order_id,
            Self::OrderDelivered { order_id } => order_id,
            Self::OrderCancelled { order_id, .. } => order_id,
        }
    }

    /// Amount relevant to revenue accounting (only when payment event).
    pub fn revenue_amount(&self) -> f64 {
        match self {
            Self::OrderPlaced { amount, .. } => *amount,
            Self::PaymentReceived { amount, .. } => *amount,
            _ => 0.0,
        }
    }

    /// Parse a CSV event_type string into an OrderEvent.
    pub fn from_csv_row(
        order_id: &str,
        customer_id: &str,
        event_type: &str,
        amount: f64,
    ) -> Option<Self> {
        match event_type {
            "OrderPlaced" => Some(Self::OrderPlaced {
                order_id: order_id.to_string(),
                customer_id: customer_id.to_string(),
                amount,
            }),
            "PaymentReceived" => Some(Self::PaymentReceived {
                order_id: order_id.to_string(),
                amount,
            }),
            "ItemShipped" => Some(Self::ItemShipped {
                order_id: order_id.to_string(),
            }),
            "OrderDelivered" => Some(Self::OrderDelivered {
                order_id: order_id.to_string(),
            }),
            "OrderCancelled" => Some(Self::OrderCancelled {
                order_id: order_id.to_string(),
                reason: "customer request".to_string(),
            }),
            _ => None,
        }
    }
}

// ─── Aggregate state ──────────────────────────────────────────────────────────

/// The current observable state of an Order, fully derived from its event history.
/// This struct is produced by replaying events — it is never stored as-is.
#[derive(Debug, Clone)]
pub struct OrderState {
    pub id: String,
    pub customer_id: String,
    pub amount: f64,
    pub status: OrderStatus,
    /// How many events have been applied (= position in event stream).
    pub version: u32,
}

impl OrderState {
    /// Empty initial state before any events.
    pub fn draft(id: &str) -> Self {
        Self {
            id: id.to_string(),
            customer_id: String::new(),
            amount: 0.0,
            status: OrderStatus::Draft,
            version: 0,
        }
    }

    /// Transition function — mutates self in place.
    /// Returns Ok(()) on a valid transition or Err(message) for a state-machine violation.
    /// This is the heart of event sourcing: each event is a fact; apply() enforces the rules.
    pub fn apply(&mut self, event: &OrderEvent) -> Result<(), String> {
        match (&self.status, event) {
            (
                OrderStatus::Draft,
                OrderEvent::OrderPlaced {
                    customer_id,
                    amount,
                    ..
                },
            ) => {
                self.customer_id = customer_id.clone();
                self.amount = *amount;
                self.status = OrderStatus::Placed;
            }
            (OrderStatus::Placed, OrderEvent::PaymentReceived { .. }) => {
                self.status = OrderStatus::Paid;
            }
            (OrderStatus::Paid, OrderEvent::ItemShipped { .. }) => {
                self.status = OrderStatus::Shipped;
            }
            (OrderStatus::Shipped, OrderEvent::OrderDelivered { .. }) => {
                self.status = OrderStatus::Delivered;
            }
            // Cancellation allowed from any non-terminal state
            (s, OrderEvent::OrderCancelled { .. }) if !s.is_terminal() => {
                self.status = OrderStatus::Cancelled;
            }
            (s, e) => {
                return Err(format!(
                    "[{}]  {} → {}  ILLEGAL  (at version {})",
                    self.id,
                    s.label(),
                    e.event_type(),
                    self.version
                ));
            }
        }
        self.version += 1;
        Ok(())
    }

    /// Rebuild state by folding a slice of events.
    /// Returns (final_state, violations).  Violations are recorded but do not stop replay.
    pub fn from_events(id: &str, events: &[&OrderEvent]) -> (Self, Vec<String>) {
        let mut state = Self::draft(id);
        let mut violations = Vec::new();
        for event in events {
            if let Err(msg) = state.apply(event) {
                violations.push(msg);
            }
        }
        (state, violations)
    }
}
