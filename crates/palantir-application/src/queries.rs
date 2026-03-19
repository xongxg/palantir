use crate::domain::{finance::TransactionRepository, organization::EmployeeRepository};
use crate::infrastructure::pipeline::{
    dataset::{Dataset, Record, Value},
    transforms::{AggFunc, Aggregate, Filter, FilterOp, Join, JoinType, Pipeline, Select, Sort},
};

// ─── Read models ──────────────────────────────────────────────────────────────

pub struct DeptSpendSummary {
    pub department: String,
    pub total_spend: f64,
    pub tx_count: i64,
    pub avg_tx: f64,
    pub largest_tx: f64,
}

pub struct TopEarner {
    pub name: String,
    pub department: String,
    pub level: String,
    pub salary: f64,
}

pub struct HighValueTx {
    pub employee: String,
    pub department: String,
    pub category: String,
    pub amount: f64,
}

// ─── Dataset adapters (anti-corruption layer) ─────────────────────────────────

pub fn employees_dataset(repo: &dyn EmployeeRepository) -> Dataset {
    let mut ds = Dataset::new("employees", "Employee");
    for e in repo.find_all() {
        ds.push(
            Record::new(e.id.as_str())
                .set("id", Value::String(e.id.to_string()))
                .set("name", Value::String(e.name.clone()))
                .set("department", Value::String(e.department.to_string()))
                .set("salary", Value::Float(e.salary.amount()))
                .set("level", Value::String(e.level.as_str().to_string())),
        );
    }
    ds
}

pub fn transactions_dataset(repo: &dyn TransactionRepository) -> Dataset {
    let mut ds = Dataset::new("transactions", "Transaction");
    for t in repo.find_all() {
        ds.push(
            Record::new(t.id.as_str())
                .set("id", Value::String(t.id.to_string()))
                .set("employee_id", Value::String(t.employee_id.to_string()))
                .set("amount", Value::Float(t.amount.amount()))
                .set("category", Value::String(t.category.to_string())),
        );
    }
    ds
}

// ─── Queries ──────────────────────────────────────────────────────────────────

pub fn query_dept_spend_summary(
    emp_repo: &dyn EmployeeRepository,
    tx_repo: &dyn TransactionRepository,
) -> Vec<DeptSpendSummary> {
    let result = Pipeline::new("dept_spend_summary")
        .step(Join {
            right: employees_dataset(emp_repo),
            left_key: "employee_id".into(),
            right_key: "id".into(),
            join_type: JoinType::Inner,
        })
        .step(Aggregate {
            group_by: vec!["department".into()],
            aggregations: vec![
                ("total_spend".into(), AggFunc::Sum("amount".into())),
                ("tx_count".into(), AggFunc::Count),
                ("avg_tx".into(), AggFunc::Avg("amount".into())),
                ("largest_tx".into(), AggFunc::Max("amount".into())),
            ],
        })
        .step(Sort {
            field: "total_spend".into(),
            descending: true,
        })
        .run(transactions_dataset(tx_repo));

    result
        .records
        .iter()
        .map(|r| {
            let f = |k: &str| r.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            let i = |k: &str| match r.get(k) {
                Some(Value::Int(n)) => *n,
                _ => 0,
            };
            let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            DeptSpendSummary {
                department: s("department"),
                total_spend: f("total_spend"),
                tx_count: i("tx_count"),
                avg_tx: f("avg_tx"),
                largest_tx: f("largest_tx"),
            }
        })
        .collect()
}

pub fn query_top_earners(emp_repo: &dyn EmployeeRepository) -> Vec<TopEarner> {
    let result = Pipeline::new("top_earners")
        .step(Select {
            fields: vec![
                "name".into(),
                "department".into(),
                "level".into(),
                "salary".into(),
            ],
        })
        .step(Sort {
            field: "salary".into(),
            descending: true,
        })
        .run(employees_dataset(emp_repo));

    result
        .records
        .iter()
        .map(|r| {
            let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            TopEarner {
                name: s("name"),
                department: s("department"),
                level: s("level"),
                salary: r.get("salary").and_then(Value::as_f64).unwrap_or(0.0),
            }
        })
        .collect()
}

pub fn query_high_value_transactions(
    threshold: f64,
    emp_repo: &dyn EmployeeRepository,
    tx_repo: &dyn TransactionRepository,
) -> Vec<HighValueTx> {
    let result = Pipeline::new("high_value_txns")
        .step(Filter {
            field: "amount".into(),
            op: FilterOp::Gt,
            value: Value::Float(threshold),
        })
        .step(Join {
            right: employees_dataset(emp_repo),
            left_key: "employee_id".into(),
            right_key: "id".into(),
            join_type: JoinType::Inner,
        })
        .step(Sort {
            field: "amount".into(),
            descending: true,
        })
        .run(transactions_dataset(tx_repo));

    result
        .records
        .iter()
        .map(|r| {
            let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            HighValueTx {
                employee: s("name"),
                department: s("department"),
                category: s("category"),
                amount: r.get("amount").and_then(Value::as_f64).unwrap_or(0.0),
            }
        })
        .collect()
}
