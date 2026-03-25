use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("adapter error: {0}")]
    Message(String),
}

#[derive(Debug, Error)]
pub enum MappingError {
    #[error("mapping error: {0}")]
    Message(String),
}

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("repository error: {0}")]
    Message(String),
}

pub type Result<T, E = anyhow::Error> = std::result::Result<T, E>;
