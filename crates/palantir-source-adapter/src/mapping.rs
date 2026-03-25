use crate::errors::MappingError;
use crate::model::{CanonicalRecord, OntologyEvent, OntologySchema};

pub trait Mapping: Send + Sync {
    fn version(&self) -> &str;
    fn apply(
        &self,
        rec: &CanonicalRecord,
        schema: &OntologySchema,
    ) -> Result<Vec<OntologyEvent>, MappingError>;
}
