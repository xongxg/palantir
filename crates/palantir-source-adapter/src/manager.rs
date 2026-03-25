use crate::adapters::SourceAdapter;
use crate::errors::Result;
use crate::mapping::Mapping;
use crate::model::{Cursor, OntologySchema};
use crate::repository::OntologyRepository;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;

pub struct OntologyManager<R: OntologyRepository> {
    adapters: Vec<Arc<dyn SourceAdapter>>,
    mappers: HashMap<String, Arc<dyn Mapping>>,
    repo: Arc<R>,
    schema: Arc<OntologySchema>,
    batch_size: usize,
}

impl<R: OntologyRepository + 'static> OntologyManager<R> {
    pub fn new(
        adapters: Vec<Arc<dyn SourceAdapter>>,
        mappers: HashMap<String, Arc<dyn Mapping>>,
        repo: Arc<R>,
        schema: OntologySchema,
    ) -> Self {
        Self {
            adapters,
            mappers,
            repo,
            schema: Arc::new(schema),
            batch_size: 512,
        }
    }
    pub fn with_batch_size(mut self, n: usize) -> Self {
        self.batch_size = n.max(1);
        self
    }

    pub async fn run(&self, since: Option<Cursor>) -> Result<()> {
        let mut tasks = Vec::new();
        for adapter in &self.adapters {
            let mapper = self
                .mappers
                .get(adapter.id())
                .cloned()
                .expect("missing mapper");
            let repo = self.repo.clone();
            let schema = self.schema.clone();
            let since_clone = since.clone();

            let adapter = adapter.clone();
            let batch = self.batch_size;
            let task = tokio::spawn(async move {
                let mut stream = adapter.stream(since_clone);
                let mut buf = Vec::with_capacity(batch);
                while let Some(item) = stream.next().await {
                    let rec = item.map_err(|e| anyhow::anyhow!(e.to_string()))?;
                    let events = mapper
                        .apply(&rec, &schema)
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                    buf.extend(events);
                    if buf.len() >= batch {
                        repo.apply(&buf)
                            .await
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                        buf.clear();
                    }
                }
                if !buf.is_empty() {
                    repo.apply(&buf)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                }
                Ok::<(), anyhow::Error>(())
            });
            tasks.push(task);
        }
        for t in tasks {
            t.await??;
        }
        Ok(())
    }
}
