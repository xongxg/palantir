//! Dataset and Value types used by analytics and ontology discovery.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Json(serde_json::Value),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub fn get(&self, key: &str) -> Option<&Value> { self.fields.get(key) }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub name: String,
    pub object_type: String,
    pub records: Vec<Record>,
}

impl Dataset {
    pub fn new(name: &str, object_type: &str) -> Self {
        Self { name: name.into(), object_type: object_type.into(), records: Vec::new() }
    }
    pub fn push(&mut self, record: Record) { self.records.push(record); }
    pub fn len(&self) -> usize { self.records.len() }
    pub fn is_empty(&self) -> bool { self.records.is_empty() }
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self { Value::Float(f) => Some(*f), Value::Int(n) => Some(*n as f64), _ => None }
    }
    pub fn as_str(&self) -> Option<&str> { match self { Value::String(s) => Some(s), _ => None } }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(x) => write!(f, "{}", x),
            Value::Bool(b) => write!(f, "{}", b),
            Value::String(s) => write!(f, "{}", s),
            Value::Json(j) => write!(f, "{}", j.to_string()),
            Value::Null => write!(f, ""),
        }
    }
}
