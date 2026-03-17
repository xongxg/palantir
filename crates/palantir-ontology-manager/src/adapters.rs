use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;

#[derive(Debug, Clone)]
pub struct SourceDescriptor {
    pub id: String,
    pub has_cursor: bool,
    pub partitions: Option<u32>,
}

#[async_trait]
pub trait SourceAdapter: Send + Sync {
    fn id(&self) -> &str;
    async fn describe(&self) -> SourceDescriptor;
    fn stream(
        &self,
        since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send>;
}
