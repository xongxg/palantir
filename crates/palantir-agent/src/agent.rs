use crate::tools::IngestTools;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct CsvIngestIntent {
    pub path: String,
    pub ns: String,
    pub schema: String,
    pub mapping_toml: String,
    pub preview_limit: Option<usize>,
}

pub struct Agent<T: IngestTools> {
    tools: T,
}

impl<T: IngestTools> Agent<T> {
    pub fn new(tools: T) -> Self {
        Self { tools }
    }

    pub fn handle_csv_ingest(&self, intent: &CsvIngestIntent) -> anyhow::Result<()> {
        let limit = intent.preview_limit.unwrap_or(5);
        let preview = self.tools.preview_csv(
            &intent.path,
            &intent.ns,
            &intent.schema,
            &intent.mapping_toml,
            limit,
        )?;
        println!("Preview events: {} (limit {})", preview, limit);
        let applied = self.tools.apply_csv(
            &intent.path,
            &intent.ns,
            &intent.schema,
            &intent.mapping_toml,
        )?;
        println!("Applied events: {}", applied);
        Ok(())
    }
}
