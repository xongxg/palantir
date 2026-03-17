use super::{errors::DomainError, events::EmployeeHired, money::Money};

// ─── Value Objects ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmployeeId(pub String);

impl EmployeeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EmployeeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepartmentName(pub String);

impl DepartmentName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DepartmentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmployeeLevel {
    Junior,
    Mid,
    Senior,
    Staff,
}

impl EmployeeLevel {
    pub fn from_str(s: &str) -> Result<Self, DomainError> {
        match s {
            "Junior" => Ok(Self::Junior),
            "Mid" => Ok(Self::Mid),
            "Senior" => Ok(Self::Senior),
            "Staff" => Ok(Self::Staff),
            other => Err(DomainError::UnknownLevel(other.to_string())),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Junior => "Junior",
            Self::Mid => "Mid",
            Self::Senior => "Senior",
            Self::Staff => "Staff",
        }
    }
}

// ─── Entity ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Employee {
    pub id: EmployeeId,
    pub name: String,
    pub department: DepartmentName,
    pub salary: Money,
    pub level: EmployeeLevel,
}

impl Employee {
    /// Factory method — enforces business invariants, returns domain event.
    pub fn hire(
        id: EmployeeId,
        name: String,
        department: DepartmentName,
        salary: Money,
        level: EmployeeLevel,
    ) -> Result<(Self, EmployeeHired), DomainError> {
        if salary.amount() == 0.0 {
            return Err(DomainError::InvalidSalary("salary cannot be zero".into()));
        }
        let employee = Self {
            id: id.clone(),
            name: name.clone(),
            department: department.clone(),
            salary: salary.clone(),
            level,
        };
        let event = EmployeeHired {
            employee_id: id,
            name,
            department: department.to_string(),
            salary,
        };
        Ok((employee, event))
    }
}

// ─── Repository trait (port) ──────────────────────────────────────────────────

pub trait EmployeeRepository {
    fn save(&mut self, employee: Employee);
    fn find_by_id(&self, id: &EmployeeId) -> Option<&Employee>;
    fn find_all(&self) -> Vec<&Employee>;
}
