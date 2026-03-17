use crate::errors::RepositoryError;
use crate::model::OntologyEvent;
use tokio::sync::Mutex;

#[async_trait::async_trait]
pub trait OntologyRepository: Send + Sync {
    async fn apply(&self, events: &[OntologyEvent]) -> Result<(), RepositoryError>;
}

pub struct InMemoryRepository {
    pub events: Mutex<Vec<OntologyEvent>>,
}

impl InMemoryRepository {
    pub fn new() -> Self { Self { events: Mutex::new(Vec::new()) } }
}

#[async_trait::async_trait]
impl OntologyRepository for InMemoryRepository {
    async fn apply(&self, events: &[OntologyEvent]) -> Result<(), RepositoryError> {
        let mut guard = self.events.lock().await;
        guard.extend_from_slice(events);
        Ok(())
    }
}
