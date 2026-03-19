use crate::dataset::{Dataset, Record, Value};
use std::collections::HashMap;

pub trait Transform: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &str;
    fn run(&self, input: Dataset) -> Dataset;
}

#[derive(Debug)]
pub struct Pipeline {
    #[allow(dead_code)]
    name: String,
    steps: Vec<Box<dyn Transform>>,
}

impl Pipeline {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }
    pub fn step<T: Transform + 'static>(mut self, t: T) -> Self {
        self.steps.push(Box::new(t));
        self
    }
    pub fn run(&self, input: Dataset) -> Dataset {
        self.steps.iter().fold(input, |acc, t| t.run(acc))
    }
}

#[derive(Debug, Clone)]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Contains,
}

#[derive(Debug, Clone)]
pub struct Filter {
    pub field: String,
    pub op: FilterOp,
    pub value: Value,
}

impl Transform for Filter {
    fn name(&self) -> &str {
        "filter"
    }
    fn run(&self, mut input: Dataset) -> Dataset {
        input.records.retain(|r| match r.get(&self.field) {
            Some(v) => compare(v, &self.value, &self.op),
            None => false,
        });
        input
    }
}

#[derive(Debug, Clone)]
pub struct Select {
    pub fields: Vec<String>,
}

impl Transform for Select {
    fn name(&self) -> &str {
        "select"
    }
    fn run(&self, input: Dataset) -> Dataset {
        let mut out = Dataset::new(&input.name, &input.object_type);
        for rec in input.records {
            let mut m = HashMap::new();
            for f in &self.fields {
                if let Some(v) = rec.get(f) {
                    m.insert(f.clone(), v.clone());
                }
            }
            out.push(Record {
                id: rec.id,
                fields: m,
            });
        }
        out
    }
}

#[derive(Debug, Clone)]
pub enum AggFunc {
    Sum(String),
    Count,
    Avg(String),
    Max(String),
    Min(String),
}

#[derive(Debug, Clone)]
pub struct Aggregate {
    pub group_by: Vec<String>,
    pub aggregations: Vec<(String, AggFunc)>,
}

impl Transform for Aggregate {
    fn name(&self) -> &str {
        "aggregate"
    }
    fn run(&self, input: Dataset) -> Dataset {
        #[derive(Default)]
        struct Acc {
            count: i64,
            sums: HashMap<String, f64>,
            max: HashMap<String, f64>,
            min: HashMap<String, f64>,
        }

        let mut groups: HashMap<Vec<String>, Acc> = HashMap::new();
        for r in &input.records {
            let key: Vec<String> = self
                .group_by
                .iter()
                .map(|f| r.get(f).map(to_string).unwrap_or_default())
                .collect();
            let ent = groups.entry(key).or_default();
            ent.count += 1;
            for (_, func) in &self.aggregations {
                match func {
                    AggFunc::Sum(field) | AggFunc::Avg(field) => {
                        let v = r.get(field).and_then(Value::as_f64).unwrap_or(0.0);
                        *ent.sums.entry(field.clone()).or_default() += v;
                    }
                    AggFunc::Max(field) => {
                        let v = r
                            .get(field)
                            .and_then(Value::as_f64)
                            .unwrap_or(f64::NEG_INFINITY);
                        let e = ent.max.entry(field.clone()).or_insert(f64::NEG_INFINITY);
                        if v > *e {
                            *e = v;
                        }
                    }
                    AggFunc::Min(field) => {
                        let v = r
                            .get(field)
                            .and_then(Value::as_f64)
                            .unwrap_or(f64::INFINITY);
                        let e = ent.min.entry(field.clone()).or_insert(f64::INFINITY);
                        if v < *e {
                            *e = v;
                        }
                    }
                    AggFunc::Count => {}
                }
            }
        }
        let mut out = Dataset::new(&input.name, &input.object_type);
        for (key, acc) in groups.into_iter() {
            let mut fields = HashMap::new();
            for (i, gb) in self.group_by.iter().enumerate() {
                fields.insert(gb.clone(), Value::String(key[i].clone()));
            }
            for (alias, func) in &self.aggregations {
                let v = match func {
                    AggFunc::Count => Value::Int(acc.count),
                    AggFunc::Sum(field) => Value::Float(*acc.sums.get(field).unwrap_or(&0.0)),
                    AggFunc::Avg(field) => {
                        let s = *acc.sums.get(field).unwrap_or(&0.0);
                        Value::Float(if acc.count == 0 {
                            0.0
                        } else {
                            s / acc.count as f64
                        })
                    }
                    AggFunc::Max(field) => {
                        Value::Float(*acc.max.get(field).unwrap_or(&f64::NEG_INFINITY))
                    }
                    AggFunc::Min(field) => {
                        Value::Float(*acc.min.get(field).unwrap_or(&f64::INFINITY))
                    }
                };
                fields.insert(alias.clone(), v);
            }
            out.push(Record {
                id: uuid_like(),
                fields,
            });
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct Sort {
    pub field: String,
    pub descending: bool,
}

impl Transform for Sort {
    fn name(&self) -> &str {
        "sort"
    }
    fn run(&self, mut input: Dataset) -> Dataset {
        let field = self.field.clone();
        if self.descending {
            input.records.sort_by(|a, b| cmp_record(b, a, &field));
        } else {
            input.records.sort_by(|a, b| cmp_record(a, b, &field));
        }
        input
    }
}

#[derive(Debug, Clone)]
pub enum JoinType {
    Inner,
}

#[derive(Debug, Clone)]
pub struct Join {
    pub right: Dataset,
    pub left_key: String,
    pub right_key: String,
    pub join_type: JoinType,
}

impl Transform for Join {
    fn name(&self) -> &str {
        "join"
    }
    fn run(&self, input: Dataset) -> Dataset {
        // Build right index: key → record
        let mut idx: HashMap<String, &Record> = HashMap::new();
        for r in &self.right.records {
            if let Some(v) = r.get(&self.right_key) {
                idx.insert(to_string(v), r);
            }
        }
        let mut out = Dataset::new(&input.name, &input.object_type);
        for l in &input.records {
            if let Some(lk) = l.get(&self.left_key) {
                if let Some(rr) = idx.get(&to_string(lk)) {
                    // merge fields (right wins on collisions with suffix)
                    let mut merged = l.fields.clone();
                    for (k, v) in rr.fields.iter() {
                        let key = if merged.contains_key(k) {
                            format!("{}_r", k)
                        } else {
                            k.clone()
                        };
                        merged.insert(key, v.clone());
                    }
                    out.push(Record {
                        id: l.id.clone(),
                        fields: merged,
                    });
                }
            }
        }
        out
    }
}

fn to_string(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::String(s) => s.clone(),
        Value::Json(j) => j.to_string(),
        Value::Null => String::new(),
    }
}

fn compare(a: &Value, b: &Value, op: &FilterOp) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => cmp_f(*x as f64, *y as f64, op),
        (Value::Float(x), Value::Float(y)) => cmp_f(*x, *y, op),
        (Value::Int(x), Value::Float(y)) => cmp_f(*x as f64, *y, op),
        (Value::Float(x), Value::Int(y)) => cmp_f(*x, *y as f64, op),
        (Value::String(x), Value::String(y)) => cmp_s(x, y, op),
        _ => false,
    }
}

fn cmp_f(a: f64, b: f64, op: &FilterOp) -> bool {
    match op {
        FilterOp::Eq => a == b,
        FilterOp::Ne => a != b,
        FilterOp::Gt => a > b,
        FilterOp::Lt => a < b,
        FilterOp::Ge => a >= b,
        FilterOp::Le => a <= b,
        FilterOp::Contains => false,
    }
}
fn cmp_s(a: &str, b: &str, op: &FilterOp) -> bool {
    match op {
        FilterOp::Eq => a == b,
        FilterOp::Ne => a != b,
        FilterOp::Gt => a > b,
        FilterOp::Lt => a < b,
        FilterOp::Ge => a >= b,
        FilterOp::Le => a <= b,
        FilterOp::Contains => a.contains(b),
    }
}

fn cmp_record(a: &Record, b: &Record, field: &str) -> std::cmp::Ordering {
    match (a.get(field), b.get(field)) {
        (Some(Value::Float(x)), Some(Value::Float(y))) => {
            x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Some(Value::Int(x)), Some(Value::Int(y))) => x.cmp(y),
        (Some(Value::String(x)), Some(Value::String(y))) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    }
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("r{:x}", ns)
}
