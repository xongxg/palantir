//! Example 12 — OntologyManager ingest from CSV via TOML mapping
//! Run: cargo run --example 12_ingest_csv_ontology

use palantir_ontology_manager::adapters::SourceAdapter;
use palantir_ontology_manager::adapters_csv::CsvAdapter;
use palantir_ontology_manager::manager::OntologyManager;
use palantir_ontology_manager::mapping::Mapping;
use palantir_ontology_manager::mapping_toml::TomlMapping;
use palantir_ontology_manager::model::OntologySchema;
use palantir_ontology_manager::repository::InMemoryRepository;

use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build OntologyManager with CSV adapters
    let emp_adapter: Arc<dyn SourceAdapter> = Arc::new(CsvAdapter::new("csv.employees", "data/employees.csv", "csv.employees", "employees_v1"));
    let tx_adapter: Arc<dyn SourceAdapter> = Arc::new(CsvAdapter::new("csv.transactions", "data/transactions.csv", "csv.transactions", "transactions_v1"));

    let emp_toml = r#"
version = "v1"
entity = "Employee"
[from]
ns = "csv.employees"
[id]
field = "id"
[map]
name = "name|str"
department = "department|str"
level = "level|str"
salary = "salary|float"
"#;
    let tx_toml = r#"
version = "v1"
entity = "Transaction"
[from]
ns = "csv.transactions"
[id]
field = "id"
[map]
employee_id = "employee_id|str"
amount = "amount|float"
category = "category|str"
"#;

    let emp_map: Arc<dyn Mapping> = Arc::new(TomlMapping::from_str(emp_toml)?);
    let tx_map: Arc<dyn Mapping> = Arc::new(TomlMapping::from_str(tx_toml)?);

    let mut mappers: HashMap<String, Arc<dyn Mapping>> = HashMap::new();
    mappers.insert(emp_adapter.id().to_string(), emp_map);
    mappers.insert(tx_adapter.id().to_string(), tx_map);

    let repo = Arc::new(InMemoryRepository::new());
    let schema = OntologySchema { version: "v1".into(), entities: Default::default() };

    let mgr = OntologyManager::new(vec![emp_adapter, tx_adapter], mappers, repo.clone(), schema);
    mgr.run(None).await?;

    println!("Ingested events: {}", repo.events.lock().await.len());
    Ok(())
}
