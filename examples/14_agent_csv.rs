use palantir_agent::{agent::{Agent, CsvIngestIntent}, tools::LocalIngestTools};

fn main() -> anyhow::Result<()> {
    let agent = Agent::new(LocalIngestTools);
    let mapping = r#"
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
[[links]]
rel = "HAS"
from_key = "employee_id"
to_key = "id"
"#;
    let intent = CsvIngestIntent {
        path: "data/transactions.csv".into(),
        ns: "csv.transactions".into(),
        schema: "transactions_v1".into(),
        mapping_toml: mapping.into(),
        preview_limit: Some(3),
    };
    agent.handle_csv_ingest(&intent)?;
    Ok(())
}

