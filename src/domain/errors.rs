#[derive(Debug)]
pub enum DomainError {
    InvalidSalary(String),
    NegativeAmount(String),
    UnknownLevel(String),
    InvalidOperation(String),
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSalary(msg)    => write!(f, "invalid salary: {msg}"),
            Self::NegativeAmount(msg)   => write!(f, "negative amount: {msg}"),
            Self::UnknownLevel(l)       => write!(f, "unknown employee level: {l}"),
            Self::InvalidOperation(msg) => write!(f, "invalid operation: {msg}"),
        }
    }
}

impl std::error::Error for DomainError {}
