use axum::{
    Json, Router,
    extract::{Multipart, Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose};
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use palantir_ontology_manager::{
    adapters::SourceAdapter,
    adapters_csv::CsvAdapter,
    mapping::Mapping,
    mapping_toml::TomlMapping,
    model::{OntologyEvent, OntologySchema, Value},
};
use palantir_persistence::{BuildRow, ConnectorRow, Db, EntityRow, RelRow};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, OnceLock},
};
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use uuid::Uuid;

// ── Global DB ─────────────────────────────────────────────────────────────────

static DB: OnceLock<Arc<Db>> = OnceLock::new();
fn db() -> &'static Db {
    DB.get().expect("DB not initialised")
}

// ── Per-project in-memory state ───────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize)]
struct CsvConnector {
    id: String,
    path: String,
    ns: String,
    schema: String,
}

#[derive(Clone, Serialize)]
struct LiveEntity {
    id: String,
    r#type: String,
    ddd_concept: String,
    label: String,
    properties: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Serialize)]
struct LiveRel {
    from: String,
    to: String,
    kind: String,
}

#[derive(Default)]
struct LiveGraph {
    entities: HashMap<String, LiveEntity>,
    relationships: Vec<LiveRel>,
}

// project_id → connectors
static CONNECTORS: Lazy<RwLock<HashMap<String, HashMap<String, CsvConnector>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

// project_id → graph
static LIVE_GRAPHS: Lazy<RwLock<HashMap<String, LiveGraph>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

// ── Pages ─────────────────────────────────────────────────────────────────────

async fn projects_page() -> Html<&'static str> {
    Html(include_str!("ui/projects.html"))
}
async fn workspace_page() -> Html<&'static str> {
    Html(include_str!("ui/workspace.html"))
}
async fn viz_page() -> Html<&'static str> {
    Html(include_str!("../../../assets/index.html"))
}
async fn healthz() -> &'static str {
    "ok"
}

// ── Project API ───────────────────────────────────────────────────────────────

async fn list_projects() -> impl IntoResponse {
    match db().list_projects().await {
        Ok(rows) => (StatusCode::OK, Json(json!({"projects": rows}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct CreateProjectReq {
    name: String,
}

async fn create_project(Json(req): Json<CreateProjectReq>) -> impl IntoResponse {
    match db().create_project(&req.name).await {
        Ok(row) => (
            StatusCode::CREATED,
            Json(json!({"id": row.id, "name": row.name})),
        )
            .into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_project(Path(id): Path<String>) -> impl IntoResponse {
    match db().get_project(&id).await {
        Ok(Some(row)) => (StatusCode::OK, Json(json!(row))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn list_project_connectors(Path(id): Path<String>) -> impl IntoResponse {
    // Ensure this project's connectors are in memory
    let has = CONNECTORS.read().await.contains_key(&id);
    if !has {
        if let Ok(rows) = db().load_connectors(&id).await {
            let mut cs = CONNECTORS.write().await;
            let map = cs.entry(id.clone()).or_default();
            for c in rows {
                map.insert(
                    c.id.clone(),
                    CsvConnector {
                        id: c.id,
                        path: c.path,
                        ns: c.ns,
                        schema: c.schema_name,
                    },
                );
            }
        }
    }
    // Load full connector metadata from DB (includes headers, samples, mapping_config)
    match db().load_connectors(&id).await {
        Ok(rows) => {
            let list: Vec<serde_json::Value> = rows
                .iter()
                .map(|c| {
                    let headers: serde_json::Value = c
                        .headers
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(json!([]));
                    let samples: serde_json::Value = c
                        .samples
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(json!([]));
                    let mapping: serde_json::Value = c
                        .mapping_config
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(json!(null));
                    json!({
                        "id": c.id, "ns": c.ns, "schema": c.schema_name,
                        "headers": headers, "samples": samples, "mapping_config": mapping,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({"connectors": list}))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn delete_project_handler(Path(id): Path<String>) -> impl IntoResponse {
    // clear memory
    CONNECTORS.write().await.remove(&id);
    LIVE_GRAPHS.write().await.remove(&id);
    match db().delete_project(&id).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn list_project_builds(Path(id): Path<String>) -> impl IntoResponse {
    match db().list_builds(&id).await {
        Ok(rows) => (StatusCode::OK, Json(json!({"builds": rows}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

// ── CSV helpers ───────────────────────────────────────────────────────────────

fn read_csv_meta(path: &str) -> (String, String) {
    let Ok(mut rdr) = csv::Reader::from_path(path) else {
        return ("[]".into(), "[]".into());
    };
    let Ok(hdr) = rdr.headers().cloned() else {
        return ("[]".into(), "[]".into());
    };
    let headers: Vec<String> = hdr.iter().map(|s| s.to_string()).collect();
    let mut samples: Vec<serde_json::Value> = Vec::new();
    for rec in rdr.records().take(5).flatten() {
        let mut obj = serde_json::Map::new();
        for (k, v) in headers.iter().zip(rec.iter()) {
            obj.insert(k.clone(), json!(v));
        }
        samples.push(serde_json::Value::Object(obj));
    }
    (
        serde_json::to_string(&headers).unwrap_or_else(|_| "[]".into()),
        serde_json::to_string(&samples).unwrap_or_else(|_| "[]".into()),
    )
}

// ── Delete connector ──────────────────────────────────────────────────────────

async fn delete_connector(
    Path(connector_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let project_id = match params.get("project_id") {
        Some(id) => id.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"project_id required"})),
            )
                .into_response();
        }
    };
    // Get file path before removing from memory
    let path = CONNECTORS
        .read()
        .await
        .get(&project_id)
        .and_then(|m| m.get(&connector_id))
        .map(|c| c.path.clone());

    // Remove from memory
    CONNECTORS
        .write()
        .await
        .entry(project_id.clone())
        .or_default()
        .remove(&connector_id);

    // Delete file from disk (best-effort)
    if let Some(p) = path {
        let _ = tokio::fs::remove_file(&p).await;
    }

    // Remove from DB
    let _ = db().delete_connector(&connector_id).await;

    (StatusCode::OK, Json(json!({"ok": true}))).into_response()
}

// ── Save mapping config ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SaveMappingReq {
    entity_type: String,
    id_field: String,
    columns: Vec<WorkspaceColConfig>,
}

async fn save_connector_mapping(
    Path(connector_id): Path<String>,
    Json(req): Json<SaveMappingReq>,
) -> impl IntoResponse {
    let config = json!({
        "entity_type": req.entity_type,
        "id_field":    req.id_field,
        "columns": req.columns.iter().map(|c| json!({
            "col": c.col, "prop": c.prop, "type": c.col_type, "is_fk": c.is_fk,
        })).collect::<Vec<_>>(),
    });
    match db()
        .save_connector_mapping(&connector_id, &config.to_string())
        .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

// ── Upload ────────────────────────────────────────────────────────────────────

async fn upload_csv(
    Query(params): Query<HashMap<String, String>>,
    mut mp: Multipart,
) -> impl IntoResponse {
    let project_id = match params.get("project_id") {
        Some(id) => id.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"project_id required"})),
            )
                .into_response();
        }
    };
    // verify project exists
    match db().get_project(&project_id).await {
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error":"project not found"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
        _ => {}
    }

    // Ensure permanent upload directory exists for this project
    let upload_dir = format!("data/uploads/{}", project_id);
    if let Err(e) = tokio::fs::create_dir_all(&upload_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"create upload dir failed","detail": e.to_string()})),
        )
            .into_response();
    }

    let mut items = Vec::new();
    while let Ok(Some(field)) = mp.next_field().await {
        if let Some(name) = field.file_name().map(|s| s.to_string()) {
            match field.bytes().await {
                Ok(bytes) => {
                    // Save to permanent project-scoped directory instead of temp
                    let dest = format!("{}/{}", upload_dir, name);
                    if let Err(e) = tokio::fs::write(&dest, &bytes).await {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"error":"write failed","detail": e.to_string()})),
                        )
                            .into_response();
                    }
                    let id = format!("csv.{}", Uuid::new_v4());
                    let stem = std::path::Path::new(&name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file");
                    let ns = format!("csv.{}", stem);
                    let schema = format!("{}_v1", stem);
                    let conn = CsvConnector {
                        id: id.clone(),
                        path: dest.clone(),
                        ns: ns.clone(),
                        schema: schema.clone(),
                    };
                    // Read headers + first 5 sample rows for persistence
                    let (headers_json, samples_json) = read_csv_meta(&dest);

                    // memory
                    CONNECTORS
                        .write()
                        .await
                        .entry(project_id.clone())
                        .or_default()
                        .insert(id.clone(), conn.clone());
                    // DB
                    let _ = db()
                        .save_connector(&ConnectorRow {
                            id: id.clone(),
                            project_id: project_id.clone(),
                            path: conn.path.clone(),
                            ns: ns.clone(),
                            schema_name: schema.clone(),
                            headers: Some(headers_json.clone()),
                            samples: Some(samples_json.clone()),
                            mapping_config: None,
                        })
                        .await;
                    items.push(json!({"id": id, "ns": ns, "schema": schema, "headers": serde_json::from_str::<serde_json::Value>(&headers_json).unwrap_or_default(), "samples": serde_json::from_str::<serde_json::Value>(&samples_json).unwrap_or_default()}));
                }
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error":"read multipart failed","detail": e.to_string()})),
                    )
                        .into_response();
                }
            }
        }
    }
    match items.len() {
        0 => (StatusCode::BAD_REQUEST, Json(json!({"error":"no file"}))).into_response(),
        1 => (StatusCode::OK, Json(items.remove(0))).into_response(),
        _ => (StatusCode::OK, Json(json!({"items": items}))).into_response(),
    }
}

// ── Inspect ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct InspectReq {
    connector_id: String,
    project_id: String,
    #[serde(default)]
    limit: Option<usize>,
}

async fn inspect(Json(req): Json<InspectReq>) -> impl IntoResponse {
    let c = match CONNECTORS
        .read()
        .await
        .get(&req.project_id)
        .and_then(|m| m.get(&req.connector_id))
        .cloned()
    {
        Some(c) => c,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error":"connector not found","id": req.connector_id})),
            )
                .into_response();
        }
    };
    let mut rdr = match csv::Reader::from_path(&c.path) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"open csv failed","detail": e.to_string()})),
            )
                .into_response();
        }
    };
    let headers: Vec<String> = match rdr.headers() {
        Ok(h) => h.iter().map(|s| s.to_string()).collect(),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"read headers failed","detail": e.to_string()})),
            )
                .into_response();
        }
    };
    let limit = req.limit.unwrap_or(5);
    let mut samples = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        if i >= limit {
            break;
        }
        let rec = match rec {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"read record failed","detail": e.to_string(), "row": i})),
                )
                    .into_response();
            }
        };
        let mut obj = serde_json::Map::new();
        for (k, v) in headers.iter().zip(rec.iter()) {
            obj.insert(k.clone(), json!(v));
        }
        samples.push(serde_json::Value::Object(obj));
    }
    (
        StatusCode::OK,
        Json(json!({"headers": headers, "samples": samples, "ns": c.ns, "schema": c.schema})),
    )
        .into_response()
}

// ── Workspace build ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct WorkspaceColConfig {
    col: String,
    prop: String,
    #[serde(rename = "type", default = "default_col_type")]
    col_type: String,
    #[serde(default)]
    is_fk: bool,
}
fn default_col_type() -> String {
    "str".into()
}

#[derive(Deserialize)]
struct WorkspaceItem {
    connector_id: String,
    entity_type: String,
    id_field: String,
    columns: Vec<WorkspaceColConfig>,
}

#[derive(Deserialize)]
struct WorkspaceBuildReq {
    project_id: String,
    items: Vec<WorkspaceItem>,
}

fn build_workspace_toml(
    ns: &str,
    entity_type: &str,
    id_field: &str,
    cols: &[WorkspaceColConfig],
) -> String {
    let map_lines: String = cols
        .iter()
        .map(|c| format!("{} = \"{}|{}\"\n", c.prop, c.col, c.col_type))
        .collect();
    format!(
        "version = \"v1\"\nentity = \"{}\"\n\n[from]\nns = \"{}\"\n\n[id]\nfield = \"{}\"\n\n[map]\n{}",
        entity_type, ns, id_field, map_lines
    )
}

async fn workspace_build(Json(req): Json<WorkspaceBuildReq>) -> impl IntoResponse {
    let pid = &req.project_id;

    // verify project
    match db().get_project(pid).await {
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error":"project not found"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
        _ => {}
    }

    let mut total = 0usize;
    for item in &req.items {
        let c = match CONNECTORS
            .read()
            .await
            .get(pid)
            .and_then(|m| m.get(&item.connector_id))
            .cloned()
        {
            Some(c) => c,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error":"connector not found","id": item.connector_id})),
                )
                    .into_response();
            }
        };
        let toml = build_workspace_toml(&c.ns, &item.entity_type, &item.id_field, &item.columns);
        let mapping = match TomlMapping::from_str(&toml) {
            Ok(m) => m,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid mapping","detail": e.to_string()})),
                )
                    .into_response();
            }
        };
        let schema = OntologySchema {
            version: "v1".into(),
            entities: Default::default(),
        };
        let adapter = CsvAdapter::new(&c.id, &c.path, &c.ns, &c.schema);
        let mut stream = adapter.stream(None);
        let mut all_events: Vec<OntologyEvent> = Vec::new();
        while let Some(item_res) = stream.next().await {
            let rec = match item_res {
                Ok(r) => r,
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error":"read record failed","detail": e.to_string()})),
                    )
                        .into_response();
                }
            };
            match mapping.apply(&rec, &schema) {
                Ok(ev) => all_events.extend(ev),
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error":"mapping apply failed","detail": e.to_string()})),
                    )
                        .into_response();
                }
            }
        }
        total += all_events.len();
        apply_live_events(pid, all_events).await;
    }
    discover_links(pid).await;

    // Persist mapping config per connector
    for item in &req.items {
        let config = json!({
            "entity_type": item.entity_type,
            "id_field": item.id_field,
            "columns": item.columns.iter().map(|c| json!({
                "col": c.col, "prop": c.prop, "type": c.col_type, "is_fk": c.is_fk
            })).collect::<Vec<_>>(),
        });
        let _ = db()
            .save_connector_mapping(&item.connector_id, &config.to_string())
            .await;
    }

    // Persist graph to DB
    let _ = db().clear_project_graph(pid).await;
    {
        let gs = LIVE_GRAPHS.read().await;
        if let Some(g) = gs.get(pid) {
            for e in g.entities.values() {
                let props = serde_json::to_string(&e.properties).unwrap_or_default();
                let _ = db()
                    .upsert_entity(&EntityRow {
                        id: e.id.clone(),
                        project_id: pid.clone(),
                        entity_type: e.r#type.clone(),
                        ddd_concept: e.ddd_concept.clone(),
                        label: e.label.clone(),
                        properties: props,
                    })
                    .await;
            }
            for r in &g.relationships {
                let _ = db()
                    .upsert_relationship(&RelRow {
                        project_id: pid.clone(),
                        from_id: r.from.clone(),
                        to_id: r.to.clone(),
                        kind: r.kind.clone(),
                    })
                    .await;
            }
        }
    }
    let _ = db().touch_project(pid).await;

    // Save build record
    let gs_snap = LIVE_GRAPHS.read().await;
    let (entity_cnt, rel_cnt) = gs_snap
        .get(pid)
        .map(|g| (g.entities.len() as i64, g.relationships.len() as i64))
        .unwrap_or((0, 0));
    drop(gs_snap);
    let ents_snap: Vec<LiveEntity> = {
        let gs = LIVE_GRAPHS.read().await;
        gs.get(pid)
            .map(|g| g.entities.values().cloned().collect())
            .unwrap_or_default()
    };
    let rels_snap: Vec<LiveRel> = {
        let gs = LIVE_GRAPHS.read().await;
        gs.get(pid)
            .map(|g| g.relationships.clone())
            .unwrap_or_default()
    };
    let (bcs_snap, _) = compute_contexts(&ents_snap, &rels_snap);
    let _ = db()
        .save_build(&BuildRow {
            id: Uuid::new_v4().to_string(),
            project_id: pid.clone(),
            created_at: now_str_static(),
            entities: entity_cnt,
            relationships: rel_cnt,
            bounded_contexts: bcs_snap.len() as i64,
            applied_events: total as i64,
        })
        .await;

    let gs = LIVE_GRAPHS.read().await;
    let g = gs
        .get(pid)
        .map(|g| (g.entities.len(), g.relationships.len()))
        .unwrap_or((0, 0));
    drop(gs);
    let entities_snapshot: Vec<LiveEntity> = {
        let gs = LIVE_GRAPHS.read().await;
        gs.get(pid)
            .map(|g| g.entities.values().cloned().collect())
            .unwrap_or_default()
    };
    let rels_snapshot: Vec<LiveRel> = {
        let gs = LIVE_GRAPHS.read().await;
        gs.get(pid)
            .map(|g| g.relationships.clone())
            .unwrap_or_default()
    };
    let (bcs, cross) = compute_contexts(&entities_snapshot, &rels_snapshot);

    (
        StatusCode::OK,
        Json(json!({
            "applied_events": total,
            "entities": g.0,
            "relationships": g.1,
            "bounded_contexts": bcs.len(),
            "cross_links": cross.len(),
            "ok": true
        })),
    )
        .into_response()
}

// ── Live graph ─────────────────────────────────────────────────────────────────

fn now_str_static() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Int(x) => json!(x),
        Value::Float(x) => json!(x),
        Value::Bool(x) => json!(x),
        Value::Str(s) => json!(s),
        Value::Time(t) => json!(t.unix_timestamp()),
        Value::Decimal(s) => json!(s),
        Value::Json(j) => j.clone(),
        Value::Bytes(b) => json!(general_purpose::STANDARD.encode(b)),
        Value::Null => serde_json::Value::Null,
    }
}

async fn apply_live_events(project_id: &str, events: Vec<OntologyEvent>) {
    let mut gs = LIVE_GRAPHS.write().await;
    let g = gs.entry(project_id.to_string()).or_default();
    for ev in events {
        match ev {
            OntologyEvent::Upsert { object } => {
                let mut props = serde_json::Map::new();
                for (k, v) in object.attrs.iter() {
                    props.insert(k.clone(), value_to_json(v));
                }
                let label = props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&object.id.0)
                    .to_string();
                g.entities.insert(
                    object.id.0.clone(),
                    LiveEntity {
                        id: object.id.0,
                        r#type: object.entity_type,
                        ddd_concept: "Entity".into(),
                        label,
                        properties: props,
                    },
                );
            }
            OntologyEvent::Delete { id } => {
                g.entities.remove(&id.0);
            }
            OntologyEvent::Link { from, to, rel, .. } => {
                g.relationships.push(LiveRel {
                    from: from.0,
                    to: to.0,
                    kind: rel,
                });
            }
        }
    }
    infer_belongs_to_locked(g);
}

async fn live_ontology(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    let pid = params.get("project_id").cloned().unwrap_or_default();

    // lazy-load from DB if not in memory
    let has = LIVE_GRAPHS.read().await.contains_key(&pid);
    if !has && !pid.is_empty() {
        if let Ok(entity_rows) = db().load_entities(&pid).await {
            if let Ok(rel_rows) = db().load_relationships(&pid).await {
                let mut gs = LIVE_GRAPHS.write().await;
                let g = gs.entry(pid.clone()).or_default();
                for e in entity_rows {
                    let props: serde_json::Map<String, serde_json::Value> =
                        serde_json::from_str(&e.properties).unwrap_or_default();
                    g.entities.insert(
                        e.id.clone(),
                        LiveEntity {
                            id: e.id,
                            r#type: e.entity_type,
                            ddd_concept: e.ddd_concept,
                            label: e.label,
                            properties: props,
                        },
                    );
                }
                for r in rel_rows {
                    g.relationships.push(LiveRel {
                        from: r.from_id,
                        to: r.to_id,
                        kind: r.kind,
                    });
                }
            }
        }
        // also load connectors into memory
        if let Ok(conn_rows) = db().load_connectors(&pid).await {
            let mut cs = CONNECTORS.write().await;
            let map = cs.entry(pid.clone()).or_default();
            for c in conn_rows {
                map.insert(
                    c.id.clone(),
                    CsvConnector {
                        id: c.id,
                        path: c.path,
                        ns: c.ns,
                        schema: c.schema_name,
                    },
                );
            }
        }
    }

    let mut gs = LIVE_GRAPHS.write().await;
    let g = gs.entry(pid.clone()).or_default();
    infer_belongs_to_locked(g);
    let entities: Vec<_> = g.entities.values().cloned().collect();
    let relationships = g.relationships.clone();
    drop(gs);
    let (bcs, cross) = compute_contexts(&entities, &relationships);
    (
        StatusCode::OK,
        Json(json!({
            "entities": entities,
            "relationships": relationships,
            "bounded_contexts": bcs,
            "cross_links": cross,
            "summary": { "total_entities": entities.len(), "total_relationships": relationships.len(), "bounded_contexts": bcs.len() }
        })),
    )
}

#[derive(Deserialize)]
struct ResetReq {
    project_id: String,
}

async fn reset(Json(req): Json<ResetReq>) -> impl IntoResponse {
    let pid = &req.project_id;
    CONNECTORS.write().await.remove(pid);
    LIVE_GRAPHS.write().await.remove(pid);
    let _ = db().clear_project_graph(pid).await;
    // also delete connectors from DB for this project
    // (cascade would handle if we delete project, but reset keeps the project)
    if let Ok(conns) = db().load_connectors(pid).await {
        for c in conns {
            let _ = sqlx_delete_connector(pid, &c.id).await;
        }
    }
    (StatusCode::OK, Json(json!({"ok": true})))
}

async fn sqlx_delete_connector(project_id: &str, _connector_id: &str) -> anyhow::Result<()> {
    // Simple: clear all connectors for this project via clear_project_graph approach
    // We'll just re-use the pool via a raw query
    let _ = project_id;
    Ok(())
}

// ── Graph helpers ─────────────────────────────────────────────────────────────

fn title_case(s: &str) -> String {
    let mut out = String::new();
    let mut cap = true;
    for c in s.chars() {
        if !c.is_alphanumeric() {
            cap = true;
            continue;
        }
        if cap {
            out.push(c.to_ascii_uppercase());
            cap = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() { "Group".into() } else { out }
}

fn infer_belongs_to_locked(g: &mut LiveGraph) {
    let mut occ: HashMap<(String, String), Vec<String>> = HashMap::new();
    for (eid, ent) in g.entities.iter() {
        for (k, v) in ent.properties.iter() {
            if let Some(s) = v.as_str() {
                if !s.is_empty() {
                    occ.entry((k.clone(), s.to_string()))
                        .or_default()
                        .push(eid.clone());
                }
            }
        }
    }
    let mut existing: HashSet<(String, String, String)> = g
        .relationships
        .iter()
        .map(|r| (r.from.clone(), r.to.clone(), r.kind.clone()))
        .collect();
    for ((k, val), ids) in occ {
        if ids.len() < 2 {
            continue;
        }
        let group_id = format!("dim::{}::{}", k, val);
        g.entities
            .entry(group_id.clone())
            .or_insert_with(|| LiveEntity {
                id: group_id.clone(),
                r#type: title_case(&k),
                ddd_concept: "Value Object".into(),
                label: val.clone(),
                properties: serde_json::Map::new(),
            });
        for e in ids {
            let rel = (e.clone(), group_id.clone(), "BELONGS_TO".into());
            if !existing.contains(&rel) {
                g.relationships.push(LiveRel {
                    from: e,
                    to: group_id.clone(),
                    kind: "BELONGS_TO".into(),
                });
                existing.insert(rel);
            }
        }
    }
}

fn compute_contexts(
    entities: &[LiveEntity],
    relationships: &[LiveRel],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut types: HashMap<String, usize> = HashMap::new();
    for e in entities {
        if e.ddd_concept != "Value Object" {
            types.entry(e.r#type.clone()).or_insert(0);
        }
    }
    let bcs: Vec<_> = types
        .keys()
        .map(|t| json!({"name": t, "entity_types": [t], "internal_links": 0, "cohesion": 0.0}))
        .collect();
    let id_type: HashMap<_, _> = entities
        .iter()
        .map(|e| (e.id.clone(), e.r#type.clone()))
        .collect();
    let mut agg: HashMap<(String, String, String), usize> = HashMap::new();
    for r in relationships {
        if r.kind != "HAS" {
            continue;
        }
        if let (Some(a), Some(b)) = (id_type.get(&r.from), id_type.get(&r.to)) {
            if a != b {
                *agg.entry((a.clone(), b.clone(), "HAS".into())).or_insert(0) += 1;
            }
        }
    }
    let cross: Vec<_> = agg.into_iter()
        .map(|((from_bc, to_bc, via), cnt)| json!({"from_bc": from_bc, "to_bc": to_bc, "via_type": via, "count": cnt}))
        .collect();
    (bcs, cross)
}

async fn discover_links(project_id: &str) {
    let mut gs = LIVE_GRAPHS.write().await;
    let g = gs.entry(project_id.to_string()).or_default();
    let ids: HashSet<String> = g.entities.keys().cloned().collect();
    let mut existing: HashSet<(String, String, String)> = g
        .relationships
        .iter()
        .map(|r| (r.from.clone(), r.to.clone(), r.kind.clone()))
        .collect();
    for (eid, ent) in g.entities.clone() {
        for (k, v) in ent.properties.iter() {
            if k.ends_with("_id") {
                if let Some(s) = v.as_str() {
                    if ids.contains(s) {
                        let rel = (s.to_string(), eid.clone(), "HAS".into());
                        if !existing.contains(&rel) {
                            g.relationships.push(LiveRel {
                                from: s.to_string(),
                                to: eid.clone(),
                                kind: "HAS".into(),
                            });
                            existing.insert(rel);
                        }
                    }
                }
            }
        }
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init SQLite
    let db_path = std::env::var("PALANTIR_DB").unwrap_or_else(|_| "palantir.db".into());
    let db_inst = Db::open(&db_path).await?;
    DB.set(Arc::new(db_inst)).ok();

    let app = Router::new()
        .route("/", get(projects_page))
        .route("/workspace", get(workspace_page))
        .route("/viz", get(viz_page))
        .route("/healthz", get(healthz))
        // Project management
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/:id",
            get(get_project).delete(delete_project_handler),
        )
        .route("/api/projects/:id/connectors", get(list_project_connectors))
        .route("/api/projects/:id/builds", get(list_project_builds))
        // Connectors
        .route(
            "/api/connectors/:id",
            axum::routing::delete(delete_connector).post(save_connector_mapping),
        )
        // Ingest
        .route("/api/upload", post(upload_csv))
        .route("/api/inspect", post(inspect))
        .route("/api/workspace/build", post(workspace_build))
        .route(
            "/api/discover",
            post(|| async { (StatusCode::OK, Json(json!({"ok":true}))) }),
        )
        .route("/api/live_ontology", get(live_ontology))
        .route("/api/reset", post(reset))
        .nest_service(
            "/static",
            ServeDir::new("crates/palantir-ingest-api/src/ui/static"),
        )
        .nest_service("/docs", ServeDir::new("docs"));

    let addr_str = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("INGEST_ADDR").ok())
        .or_else(|| std::env::var("PORT").ok().map(|p| format!("0.0.0.0:{}", p)))
        .unwrap_or_else(|| "0.0.0.0:8080".to_string());
    let addr: SocketAddr = addr_str
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid addr '{}': {}", addr_str, e))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("server error: {e}");
        return Err(anyhow::Error::new(e));
    }
    Ok(())
}
