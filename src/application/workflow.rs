//! Application: Workflow Actions (Orchestrated multi-step processes)
//!
//! DDD Layer: Application Service — a Workflow is a Command-driven, multi-step
//! process that coordinates domain operations in response to an ontology-detected
//! pattern.
//!
//! In Palantir Foundry, a **Workflow action** is triggered by a condition detected
//! in the ontology (e.g. high spend, policy violation) and executes an ordered
//! sequence of steps — validation → notification → state change → audit.
//!
//! Each step produces a concrete output (what actually happened), making the
//! workflow auditable and observable.
//!
//! Flow:
//!   PatternDetector detects anomaly
//!     └─▶ DomainEvent published to EventBus
//!           └─▶ Application Service calls workflow::run_*()
//!                 └─▶ WorkflowRun { steps with real outputs }

// ─── Core types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    /// Step completed and produced `output`.
    Completed(String),
    /// Step was skipped (condition not met), reason in message.
    Skipped(String),
    /// Step failed (and workflow should halt), reason in message.
    Failed(String),
}

impl StepOutcome {
    pub fn label(&self) -> &str {
        match self {
            Self::Completed(_) => "✓",
            Self::Skipped(_)   => "○",
            Self::Failed(_)    => "✗",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Completed(m) | Self::Skipped(m) | Self::Failed(m) => m,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowStep {
    pub name:    &'static str,
    pub outcome: StepOutcome,
}

#[derive(Debug, Clone)]
pub struct WorkflowRun {
    pub workflow_name: &'static str,
    /// Human-readable trigger description.
    pub trigger:       String,
    pub steps:         Vec<WorkflowStep>,
}

impl WorkflowRun {
    pub fn succeeded(&self) -> bool {
        self.steps.iter().all(|s| {
            matches!(s.outcome, StepOutcome::Completed(_) | StepOutcome::Skipped(_))
        })
    }
}

// ─── Workflow: High-Spend Approval ────────────────────────────────────────────

/// Workflow action: multi-step approval process for an employee whose total
/// spend exceeds the policy threshold.
///
/// Steps:
///   1. Validate threshold — confirm spend actually breaches policy
///   2. Notify department head — simulate sending a manager alert
///   3. Create approval hold — freeze further transactions pending review
///   4. Archive audit record — persist the decision trail
pub fn run_high_spend_approval(
    employee_id: &str,
    employee_name: &str,
    department: &str,
    total_spend: f64,
    threshold: f64,
) -> WorkflowRun {
    let mut steps = Vec::new();

    // Step 1: Validate
    let over_by = total_spend - threshold;
    steps.push(WorkflowStep {
        name: "Validate threshold breach",
        outcome: if over_by > 0.0 {
            StepOutcome::Completed(format!(
                "${:.0} spend exceeds ${:.0} threshold by ${:.0}  → proceed",
                total_spend, threshold, over_by
            ))
        } else {
            StepOutcome::Skipped("Spend within policy — no action required".into())
        },
    });

    // Step 2: Notify manager (only if validation passed)
    if over_by > 0.0 {
        steps.push(WorkflowStep {
            name: "Notify department head",
            outcome: StepOutcome::Completed(format!(
                "Alert email → {} department head  [subj: \"Expense review: {}\"]",
                department, employee_name
            )),
        });

        // Step 3: Create approval hold
        let hold_id = format!(
            "HOLD-{}-{}",
            employee_id.to_uppercase(),
            (total_spend as u64) % 10_000
        );
        steps.push(WorkflowStep {
            name: "Create approval hold",
            outcome: StepOutcome::Completed(format!(
                "{} created — further transactions frozen pending approval",
                hold_id
            )),
        });

        // Step 4: Archive audit record
        let audit_id = format!(
            "AR-{:04X}",
            (total_spend as u64).wrapping_add(employee_id.len() as u64)
        );
        steps.push(WorkflowStep {
            name: "Archive audit record",
            outcome: StepOutcome::Completed(format!(
                "{} written to audit log  (employee={}, spend=${:.0}, threshold=${:.0})",
                audit_id, employee_id, total_spend, threshold
            )),
        });
    }

    WorkflowRun {
        workflow_name: "HighSpendApprovalWorkflow",
        trigger: format!(
            "{} ({}) — total spend ${:.0} exceeds threshold ${:.0}",
            employee_name, employee_id, total_spend, threshold
        ),
        steps,
    }
}

// ─── Workflow: Spend Policy Review ────────────────────────────────────────────

/// Workflow action: triggered when an employee's spending is concentrated
/// (>60%) in a single category, suggesting a potential policy violation.
///
/// Steps:
///   1. Analyse spend pattern — confirm concentration and classify severity
///   2. Generate policy report — produce a summary document reference
///   3. Schedule review meeting — book a calendar slot for policy review
///   4. Track in policy dashboard — register in the ongoing policy tracker
pub fn run_spend_policy_review(
    employee_id: &str,
    employee_name: &str,
    department: &str,
    top_category: &str,
    concentration_pct: f64,
    total_spend: f64,
) -> WorkflowRun {
    let mut steps = Vec::new();

    // Step 1: Analyse
    let severity = if concentration_pct >= 80.0 { "Critical" }
                   else if concentration_pct >= 60.0 { "High" }
                   else { "Medium" };
    steps.push(WorkflowStep {
        name: "Analyse spend pattern",
        outcome: StepOutcome::Completed(format!(
            "{:.0}% concentrated in \"{}\"  (${:.0} of ${:.0} total)  → severity: {}",
            concentration_pct, top_category,
            total_spend * concentration_pct / 100.0, total_spend,
            severity
        )),
    });

    // Step 2: Generate report
    let report_id = format!(
        "POL-{}-{:04}",
        &employee_id.to_uppercase(),
        (concentration_pct as u64 * 137) % 10_000   // deterministic mock ID
    );
    steps.push(WorkflowStep {
        name: "Generate policy report",
        outcome: StepOutcome::Completed(format!(
            "{} generated  — \"Spend concentration: {} / {} dept\"",
            report_id, employee_name, department
        )),
    });

    // Step 3: Schedule review
    steps.push(WorkflowStep {
        name: "Schedule review meeting",
        outcome: StepOutcome::Completed(format!(
            "Calendar invite sent → {} + {} department compliance officer  [topic: {}]",
            employee_name, department, report_id
        )),
    });

    // Step 4: Track in dashboard
    steps.push(WorkflowStep {
        name: "Track in policy dashboard",
        outcome: StepOutcome::Completed(format!(
            "Entry added to spend-policy tracker  (id={}, status=Under Review, severity={})",
            report_id, severity
        )),
    });

    WorkflowRun {
        workflow_name: "SpendPolicyReviewWorkflow",
        trigger: format!(
            "{} ({}) — {:.0}% spend in \"{}\" exceeds 60% concentration threshold",
            employee_name, employee_id, concentration_pct, top_category
        ),
        steps,
    }
}
