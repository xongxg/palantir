use std::collections::HashMap;
use std::fmt;

/// A dynamically-typed value stored in a record field.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "{s}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(v) => write!(f, "{v:.2}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Null => write!(f, "null"),
        }
    }
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
            (Value::String(a), Value::String(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

/// A single row in a dataset.
#[derive(Debug, Clone)]
pub struct Record {
    pub id: String,
    pub fields: HashMap<String, Value>,
}

impl Record {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), fields: HashMap::new() }
    }

    pub fn set(mut self, key: &str, value: Value) -> Self {
        self.fields.insert(key.to_string(), value);
        self
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }
}

/// A named, typed collection of records — analogous to a Foundry dataset.
#[derive(Debug, Clone)]
pub struct Dataset {
    pub name: String,
    pub object_type: String,
    pub records: Vec<Record>,
}

impl Dataset {
    pub fn new(name: &str, object_type: &str) -> Self {
        Self {
            name: name.to_string(),
            object_type: object_type.to_string(),
            records: Vec::new(),
        }
    }

    pub fn push(&mut self, record: Record) {
        self.records.push(record);
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
