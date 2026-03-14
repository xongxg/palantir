use std::collections::HashMap;

use super::dataset::{Dataset, Record, Value};

// ─── Transform trait ─────────────────────────────────────────────────────────

pub trait Transform: std::fmt::Debug {
    fn name(&self) -> &str;
    fn apply(&self, input: Dataset) -> Dataset;
}

// ─── Filter ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
}

#[derive(Debug)]
pub struct Filter {
    pub field: String,
    pub op: FilterOp,
    pub value: Value,
}

impl Transform for Filter {
    fn name(&self) -> &str { "Filter" }

    fn apply(&self, input: Dataset) -> Dataset {
        let records = input.records.into_iter().filter(|r| {
            let Some(field_val) = r.get(&self.field) else { return false };
            match &self.op {
                FilterOp::Eq => field_val == &self.value,
                FilterOp::Ne => field_val != &self.value,
                FilterOp::Gt  => field_val.partial_cmp(&self.value).is_some_and(|o| o.is_gt()),
                FilterOp::Gte => field_val.partial_cmp(&self.value).is_some_and(|o| o.is_ge()),
                FilterOp::Lt  => field_val.partial_cmp(&self.value).is_some_and(|o| o.is_lt()),
                FilterOp::Lte => field_val.partial_cmp(&self.value).is_some_and(|o| o.is_le()),
                FilterOp::Contains => {
                    matches!((field_val, &self.value),
                        (Value::String(s), Value::String(pat)) if s.contains(pat.as_str()))
                }
            }
        }).collect();
        Dataset { records, ..input }
    }
}

// ─── Select (projection) ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Select {
    pub fields: Vec<String>,
}

impl Transform for Select {
    fn name(&self) -> &str { "Select" }

    fn apply(&self, input: Dataset) -> Dataset {
        let records = input.records.into_iter().map(|r| {
            let fields = self.fields.iter()
                .filter_map(|f| r.fields.get(f).map(|v| (f.clone(), v.clone())))
                .collect();
            Record { id: r.id, fields }
        }).collect();
        Dataset { records, ..input }
    }
}

// ─── Derive (compute new field) ──────────────────────────────────────────────

#[derive(Debug)]
pub enum DeriveFunc {
    Multiply,        // from_fields[0] * from_fields[1]
    Add,             // from_fields[0] + from_fields[1]
    Concat(String),  // join string fields with separator
    Const(Value),    // constant value for every row
}

#[derive(Debug)]
pub struct Derive {
    pub new_field: String,
    pub from_fields: Vec<String>,
    pub func: DeriveFunc,
}

impl Transform for Derive {
    fn name(&self) -> &str { "Derive" }

    fn apply(&self, input: Dataset) -> Dataset {
        let records = input.records.into_iter().map(|mut r| {
            let new_val = match &self.func {
                DeriveFunc::Multiply => {
                    let a = r.get(&self.from_fields[0]).and_then(Value::as_f64);
                    let b = r.get(&self.from_fields[1]).and_then(Value::as_f64);
                    match (a, b) {
                        (Some(a), Some(b)) => Value::Float(a * b),
                        _ => Value::Null,
                    }
                }
                DeriveFunc::Add => {
                    let a = r.get(&self.from_fields[0]).and_then(Value::as_f64);
                    let b = r.get(&self.from_fields[1]).and_then(Value::as_f64);
                    match (a, b) {
                        (Some(a), Some(b)) => Value::Float(a + b),
                        _ => Value::Null,
                    }
                }
                DeriveFunc::Concat(sep) => {
                    let parts: Vec<String> = self.from_fields.iter()
                        .filter_map(|f| r.get(f).and_then(Value::as_str).map(str::to_string))
                        .collect();
                    Value::String(parts.join(sep))
                }
                DeriveFunc::Const(v) => v.clone(),
            };
            r.fields.insert(self.new_field.clone(), new_val);
            r
        }).collect();
        Dataset { records, ..input }
    }
}

// ─── Aggregate ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AggFunc {
    Count,
    Sum(String),
    Avg(String),
    Min(String),
    Max(String),
}

#[derive(Debug)]
pub struct Aggregate {
    pub group_by: Vec<String>,
    pub aggregations: Vec<(String, AggFunc)>, // (output_field_name, func)
}

impl Transform for Aggregate {
    fn name(&self) -> &str { "Aggregate" }

    fn apply(&self, input: Dataset) -> Dataset {
        // Group record indices by key
        let mut groups: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
        for (i, record) in input.records.iter().enumerate() {
            let key: Vec<String> = self.group_by.iter()
                .map(|f| record.get(f).map(|v| v.to_string()).unwrap_or_default())
                .collect();
            groups.entry(key).or_default().push(i);
        }

        let mut out_records = Vec::new();
        for (key_vals, indices) in &groups {
            let group: Vec<&Record> = indices.iter().map(|&i| &input.records[i]).collect();

            let mut out = Record::new(key_vals.join("_"));

            // Carry group-by fields into output
            for (field, val) in self.group_by.iter().zip(key_vals.iter()) {
                out.fields.insert(field.clone(), Value::String(val.clone()));
            }

            // Compute aggregations
            for (out_field, agg) in &self.aggregations {
                let val = match agg {
                    AggFunc::Count => Value::Int(group.len() as i64),
                    AggFunc::Sum(f) => {
                        let sum: f64 = group.iter()
                            .filter_map(|r| r.get(f).and_then(Value::as_f64))
                            .sum();
                        Value::Float(sum)
                    }
                    AggFunc::Avg(f) => {
                        let vals: Vec<f64> = group.iter()
                            .filter_map(|r| r.get(f).and_then(Value::as_f64))
                            .collect();
                        if vals.is_empty() { Value::Null }
                        else { Value::Float(vals.iter().sum::<f64>() / vals.len() as f64) }
                    }
                    AggFunc::Min(f) => {
                        group.iter()
                            .filter_map(|r| r.get(f))
                            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                            .cloned()
                            .unwrap_or(Value::Null)
                    }
                    AggFunc::Max(f) => {
                        group.iter()
                            .filter_map(|r| r.get(f))
                            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                            .cloned()
                            .unwrap_or(Value::Null)
                    }
                };
                out.fields.insert(out_field.clone(), val);
            }

            out_records.push(out);
        }

        Dataset { name: input.name, object_type: input.object_type, records: out_records }
    }
}

// ─── Join ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum JoinType { Inner, Left }

#[derive(Debug)]
pub struct Join {
    pub right: Dataset,
    pub left_key: String,
    pub right_key: String,
    pub join_type: JoinType,
}

impl Transform for Join {
    fn name(&self) -> &str { "Join" }

    fn apply(&self, input: Dataset) -> Dataset {
        // Build a lookup from right side
        let right_index: HashMap<String, &Record> = self.right.records.iter()
            .filter_map(|r| r.get(&self.right_key).map(|v| (v.to_string(), r)))
            .collect();

        let mut out_records = Vec::new();
        for left in &input.records {
            let key = left.get(&self.left_key).map(|v| v.to_string()).unwrap_or_default();
            match right_index.get(&key) {
                Some(right) => {
                    let mut merged = left.clone();
                    for (k, v) in &right.fields {
                        if k != &self.right_key {
                            // Don't overwrite existing left-side fields
                            merged.fields.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                    out_records.push(merged);
                }
                None => {
                    if matches!(self.join_type, JoinType::Left) {
                        out_records.push(left.clone());
                    }
                }
            }
        }

        Dataset { name: input.name, object_type: input.object_type, records: out_records }
    }
}

// ─── Sort ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Sort {
    pub field: String,
    pub descending: bool,
}

impl Transform for Sort {
    fn name(&self) -> &str { "Sort" }

    fn apply(&self, mut input: Dataset) -> Dataset {
        input.records.sort_by(|a, b| {
            let ord = match (a.get(&self.field), b.get(&self.field)) {
                (Some(va), Some(vb)) => va.partial_cmp(vb).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => std::cmp::Ordering::Equal,
            };
            if self.descending { ord.reverse() } else { ord }
        });
        input
    }
}

// ─── Pipeline ────────────────────────────────────────────────────────────────

pub struct Pipeline {
    pub name: String,
    steps: Vec<Box<dyn Transform>>,
}

impl Pipeline {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), steps: Vec::new() }
    }

    pub fn step(mut self, transform: impl Transform + 'static) -> Self {
        self.steps.push(Box::new(transform));
        self
    }

    pub fn run(&self, input: Dataset) -> Dataset {
        println!("Running pipeline: \"{}\"", self.name);
        let mut current = input;
        for step in &self.steps {
            let before = current.len();
            current = step.apply(current);
            println!("  [{}] {} → {} records", step.name(), before, current.len());
        }
        current
    }
}
