use super::errors::DomainError;

/// Money is a value object — immutable, validated at construction.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Money(f64);

impl Money {
    pub fn new(amount: f64) -> Result<Self, DomainError> {
        if amount < 0.0 {
            Err(DomainError::NegativeAmount(format!("{amount}")))
        } else {
            Ok(Self(amount))
        }
    }

    pub fn amount(&self) -> f64 {
        self.0
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${:.2}", self.0)
    }
}
