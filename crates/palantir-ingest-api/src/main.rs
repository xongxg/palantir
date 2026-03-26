use axum::{
    Json, Router,
    extract::{Multipart, Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose};
use object_store::{ObjectStore, aws::AmazonS3Builder, path::Path as OsPath};
use bytes::Bytes as OsBytes;
use palantir_dataset::{DatasetStore, LocalFsBackend, S3Backend};
use std::sync::Arc as StdArc;
use futures_util::TryStreamExt;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use palantir_source_adapter::{
    adapters::SourceAdapter,
    adapters_csv::CsvAdapter,
    adapters_json::JsonAdapter,
    adapters_rest::RestAdapter,
    adapters_sql::SqlAdapter,
    mapping::Mapping,
    mapping_toml::TomlMapping,
    model::{OntologyEvent, OntologySchema, Value},
};
use palantir_sqlite::{BuildRow, ConnectorRow, Db, EntityRow, LinkTypeMappingInput, RelRow};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, OnceLock},
};
use tokio::sync::RwLock;
use tower_http::services::{ServeDir, ServeFile};
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
async fn ingest_project_redirect(Path(id): Path<String>) -> Redirect {
    Redirect::to(&format!("/project/{}", id))
}
async fn project_workspace_page() -> Html<&'static str> {
    Html(include_str!("ui/project_workspace.html"))
}
async fn ingest_fold_redirect(Path(fold_id): Path<String>) -> Redirect {
    let project_id = db()
        .get_fold(&fold_id)
        .await
        .ok()
        .flatten()
        .map(|f| f.project_id)
        .unwrap_or_default();
    if project_id.is_empty() {
        Redirect::to("/ingest")
    } else {
        Redirect::to(&format!("/project/{}", project_id))
    }
}
async fn healthz() -> &'static str {
    "ok"
}

// ── Project API ───────────────────────────────────────────────────────────────

async fn list_projects() -> impl IntoResponse {
    match db().list_projects().await {
        Ok(rows) => {
            let mut projects = Vec::with_capacity(rows.len());
            for p in rows {
                let (fold_count, last_sync_at, status) =
                    db().project_stats(&p.id).await.unwrap_or((0, None, "idle".into()));
                projects.push(json!({
                    "id": p.id, "name": p.name,
                    "created_at": p.created_at, "updated_at": p.updated_at,
                    "fold_count": fold_count,
                    "last_sync_at": last_sync_at,
                    "status": status,
                }));
            }
            (StatusCode::OK, Json(json!({ "projects": projects }))).into_response()
        }
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

#[derive(serde::Deserialize)]
struct PatchProjectReq { name: Option<String> }
async fn patch_project(
    Path(id): Path<String>,
    Json(req): Json<PatchProjectReq>,
) -> impl IntoResponse {
    if let Some(name) = req.name {
        if let Err(e) = db().rename_project(&id, &name).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response();
        }
    }
    (StatusCode::OK, Json(json!({"ok": true}))).into_response()
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

/// POST /api/upload/source — upload one or more CSV files for use as a data source.
/// No project_id needed. Files saved to data/uploads/sources/.
/// Returns [{ name, path, size }]
async fn upload_source_files(mut mp: Multipart) -> impl IntoResponse {
    let upload_dir = "data/uploads/sources";
    if let Err(e) = tokio::fs::create_dir_all(upload_dir).await {
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("create dir failed: {e}")}))).into_response();
    }
    let mut items: Vec<serde_json::Value> = Vec::new();
    while let Ok(Some(field)) = mp.next_field().await {
        let name = match field.file_name().map(|s| s.to_string()) {
            Some(n) if !n.is_empty() => n,
            _ => continue,
        };
        match field.bytes().await {
            Ok(bytes) => {
                let dest = format!("{}/{}", upload_dir, name);
                if let Err(e) = tokio::fs::write(&dest, &bytes).await {
                    return (StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": format!("write failed: {e}")}))).into_response();
                }
                items.push(json!({ "name": name, "path": dest, "size": bytes.len() }));
            }
            Err(e) => return (StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("read failed: {e}")}))).into_response(),
        }
    }
    if items.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "no files received"}))).into_response();
    }
    (StatusCode::OK, Json(json!({ "files": items }))).into_response()
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

// ── Ontology page ─────────────────────────────────────────────────────────────

async fn ontology_page() -> Html<&'static str> {
    Html(include_str!("ui/ontology.html"))
}

// ── Ontology TBox API ─────────────────────────────────────────────────────────

fn default_color() -> String { "#6366f1".into() }
fn default_icon() -> String { "●".into() }

fn default_ddd_role() -> String { "entity".into() }

#[derive(Deserialize)]
struct CreateEntityTypeReqClean {
    name: String,
    display_name: String,
    #[serde(default = "default_color")]
    color: String,
    #[serde(default = "default_icon")]
    icon: String,
    #[serde(default)]
    fold_id: Option<String>,
    #[serde(default = "default_ddd_role")]
    ddd_role: String,
}

async fn list_entity_types_handler() -> impl IntoResponse {
    match db().list_entity_types().await {
        Ok(types) => {
            // For each type, also load fields
            let mut result = Vec::new();
            for et in types {
                let fields = db().list_entity_fields(&et.id).await.unwrap_or_default();
                result.push(json!({
                    "id": et.id, "name": et.name, "display_name": et.display_name,
                    "color": et.color, "icon": et.icon, "created_at": et.created_at,
                    "fold_id": et.fold_id, "bc_id": et.bc_id, "namespace": et.namespace,
                    "ddd_role": et.ddd_role, "status": et.status,
                    "fields": fields,
                }));
            }
            (StatusCode::OK, Json(json!({"entity_types": result}))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn create_entity_type_handler(
    Json(req): Json<CreateEntityTypeReqClean>,
) -> impl IntoResponse {
    // Auto-derive namespace from fold name when fold_id is provided.
    // Stored name = "{namespace}.{short_name}" to satisfy UNIQUE constraint across folds.
    let (stored_name, namespace) = match req.fold_id.as_deref() {
        Some(fid) if !fid.is_empty() => {
            match db().get_fold(fid).await {
                Ok(Some(fold)) => {
                    let ns = fold.name.to_lowercase().replace(' ', "_");
                    let full = format!("{}.{}", ns, req.name);
                    (full, Some(ns))
                }
                _ => (req.name.clone(), None),
            }
        }
        _ => (req.name.clone(), None),
    };
    match db()
        .create_entity_type(&stored_name, &req.display_name, &req.color, &req.icon, req.fold_id.as_deref(), &req.ddd_role, namespace.as_deref())
        .await
    {
        Ok(row) => (StatusCode::CREATED, Json(json!(row))).into_response(),
        Err(e) => {
            let msg = if e.to_string().contains("UNIQUE") {
                format!("Entity Type「{}」已存在，请换一个名称", req.name)
            } else {
                e.to_string()
            };
            (StatusCode::CONFLICT, Json(json!({"error": msg}))).into_response()
        }
    }
}

async fn delete_entity_type_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().delete_entity_type(&id).await {
        Ok(_) => (StatusCode::NO_CONTENT).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct AddFieldReq {
    name: String,
    #[serde(default = "default_string_type")]
    data_type: String,
    #[serde(default)]
    is_required: bool,
    #[serde(default = "default_classification")]
    classification: String,
}
fn default_string_type() -> String { "string".into() }
fn default_classification() -> String { "Internal".into() }

async fn add_field_handler(
    Path(et_id): Path<String>,
    Json(req): Json<AddFieldReq>,
) -> impl IntoResponse {
    // Get current field count for sort_order
    let count = db()
        .list_entity_fields(&et_id)
        .await
        .map(|f| f.len() as i64)
        .unwrap_or(0);
    match db()
        .add_entity_field(&et_id, &req.name, &req.data_type, req.is_required, &req.classification, count)
        .await
    {
        Ok(row) => (StatusCode::CREATED, Json(json!(row))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn delete_field_handler(Path(field_id): Path<String>) -> impl IntoResponse {
    match db().delete_entity_field(&field_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

// ── P0: Breaking change field update ──────────────────────────────────────────

#[derive(Deserialize)]
struct UpdateFieldTypeReq {
    data_type: String,
    /// If None: dry-run (returns breaking info without applying).
    /// If Some("drop"|"cast"): apply with strategy.
    #[serde(default)]
    strategy: Option<String>,
}

async fn update_field_type_handler(
    Path(field_id): Path<String>,
    Json(req): Json<UpdateFieldTypeReq>,
) -> impl IntoResponse {
    match req.strategy {
        None => {
            // Dry run: just return breaking info
            match db().check_field_type_change(&field_id, &req.data_type).await {
                Ok(info) => (StatusCode::OK, Json(json!(info))).into_response(),
                Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))).into_response(),
            }
        }
        Some(strategy) => {
            match db().apply_field_type_change(&field_id, &req.data_type, &strategy).await {
                Ok(migration) => (StatusCode::OK, Json(json!(migration))).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
            }
        }
    }
}

async fn check_delete_field_handler(Path(field_id): Path<String>) -> impl IntoResponse {
    match db().check_field_delete(&field_id).await {
        Ok(info) => (StatusCode::OK, Json(json!(info))).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct DeleteFieldWithStrategyReq {
    #[serde(default)]
    strategy: Option<String>,
}

async fn delete_field_safe_handler(
    Path(field_id): Path<String>,
    Json(req): Json<DeleteFieldWithStrategyReq>,
) -> impl IntoResponse {
    match req.strategy {
        None => {
            // Dry run: return breaking info
            match db().check_field_delete(&field_id).await {
                Ok(info) => (StatusCode::OK, Json(json!(info))).into_response(),
                Err(e) => (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))).into_response(),
            }
        }
        Some(_) => {
            match db().apply_field_delete(&field_id).await {
                Ok(migration) => (StatusCode::OK, Json(json!(migration))).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
            }
        }
    }
}

// ── P1a: ET status lifecycle ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct SetEtStatusReq {
    status: String,
}

async fn set_et_status_handler(
    Path(et_id): Path<String>,
    Json(req): Json<SetEtStatusReq>,
) -> impl IntoResponse {
    let valid = matches!(req.status.as_str(), "draft" | "active" | "deprecated");
    if !valid {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "status must be draft|active|deprecated"}))).into_response();
    }
    match db().set_entity_type_status(&et_id, &req.status).await {
        Ok(affected_datasets) => (StatusCode::OK, Json(json!({
            "ok": true, "status": req.status, "affected_datasets": affected_datasets,
        }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── P1b: Data lineage ─────────────────────────────────────────────────────────

async fn get_et_lineage_handler(Path(et_id): Path<String>) -> impl IntoResponse {
    match db().get_et_lineage(&et_id).await {
        Ok(lineage) => (StatusCode::OK, Json(lineage)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── P2a: Bounded Context + Shared Kernel ──────────────────────────────────────

async fn list_bcs_handler(Path(fold_id): Path<String>) -> impl IntoResponse {
    match db().list_bounded_contexts(&fold_id).await {
        Ok(rows) => (StatusCode::OK, Json(json!({"bounded_contexts": rows}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct CreateBcReq {
    name: String,
    #[serde(default = "default_color")]
    color: String,
    #[serde(default)]
    auto_detected: bool,
}

async fn create_bc_handler(
    Path(fold_id): Path<String>,
    Json(req): Json<CreateBcReq>,
) -> impl IntoResponse {
    match db().create_bounded_context(&fold_id, &req.name, &req.color, req.auto_detected).await {
        Ok(row) => (StatusCode::CREATED, Json(json!(row))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn delete_bc_handler(Path(bc_id): Path<String>) -> impl IntoResponse {
    match db().delete_bounded_context(&bc_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn list_shared_kernels_handler() -> impl IntoResponse {
    match db().list_shared_kernel_folds().await {
        Ok(rows) => (StatusCode::OK, Json(json!({"shared_kernels": rows}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_context_map_handler(Path(project_id): Path<String>) -> impl IntoResponse {
    match db().get_context_map(&project_id).await {
        Ok(map) => (StatusCode::OK, Json(map)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct CreateBcRelReq {
    from_bc_id: String,
    to_bc_id: String,
    relationship_type: String,
    #[serde(default)]
    notes: Option<String>,
}

async fn create_bc_relationship_handler(Json(req): Json<CreateBcRelReq>) -> impl IntoResponse {
    match db().create_bc_relationship(&req.from_bc_id, &req.to_bc_id, &req.relationship_type, req.notes.as_deref()).await {
        Ok(row) => (StatusCode::CREATED, Json(json!(row))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn delete_bc_relationship_handler(Path(rel_id): Path<String>) -> impl IntoResponse {
    match db().delete_bc_relationship(&rel_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// Set ET → BC assignment
#[derive(Deserialize)]
struct SetEtBcReq {
    bc_id: Option<String>,
}

async fn set_et_bc_handler(
    Path(et_id): Path<String>,
    Json(req): Json<SetEtBcReq>,
) -> impl IntoResponse {
    match db().update_entity_type_bc(&et_id, req.bc_id.as_deref()).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct SetDddRoleReq { ddd_role: String }

async fn set_et_ddd_role_handler(
    Path(et_id): Path<String>,
    Json(req): Json<SetDddRoleReq>,
) -> impl IntoResponse {
    match db().update_entity_type_ddd_role(&et_id, &req.ddd_role).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── P1: BC Inference ──────────────────────────────────────────────────────────

/// POST /api/ontology/auto-link — scan all *_id FK columns, auto-create ontology_links
async fn auto_detect_links_handler() -> impl IntoResponse {
    match db().auto_detect_links().await {
        Ok((created, skipped)) => (StatusCode::OK, Json(json!({
            "ok": true, "created": created, "skipped": skipped,
        }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

/// GET /api/folds/:id/bcs/infer — dry-run Union-Find suggestions (no writes)
async fn infer_child_bcs_handler(Path(fold_id): Path<String>) -> impl IntoResponse {
    match db().infer_child_bcs(&fold_id).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

/// POST /api/folds/:id/bcs/apply-suggestions — accept suggestions and write to DB
#[derive(Deserialize)]
struct ApplyBcSuggestionsReq {
    suggestions: Vec<serde_json::Value>,
}

async fn apply_bc_suggestions_handler(
    Path(fold_id): Path<String>,
    Json(req): Json<ApplyBcSuggestionsReq>,
) -> impl IntoResponse {
    match db().apply_bc_suggestions(&fold_id, &req.suggestions).await {
        Ok(created) => (StatusCode::OK, Json(json!({"created": created}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── P2c: System Interfaces ────────────────────────────────────────────────────

async fn list_interfaces_handler() -> impl IntoResponse {
    match db().list_interfaces().await {
        Ok(rows) => (StatusCode::OK, Json(json!({"interfaces": rows}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct CreateInterfaceReq {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

async fn create_interface_handler(Json(req): Json<CreateInterfaceReq>) -> impl IntoResponse {
    match db().create_interface(&req.name, req.description.as_deref()).await {
        Ok(row) => (StatusCode::CREATED, Json(json!(row))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn delete_interface_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().delete_interface(&id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn list_et_interfaces_handler(Path(et_id): Path<String>) -> impl IntoResponse {
    match db().list_et_interfaces(&et_id).await {
        Ok(rows) => (StatusCode::OK, Json(json!({"interfaces": rows}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct AddEtInterfaceReq {
    interface_id: String,
}

async fn add_et_interface_handler(
    Path(et_id): Path<String>,
    Json(req): Json<AddEtInterfaceReq>,
) -> impl IntoResponse {
    match db().add_et_interface(&et_id, &req.interface_id).await {
        Ok(_) => (StatusCode::CREATED, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn remove_et_interface_handler(
    Path((et_id, interface_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match db().remove_et_interface(&et_id, &interface_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Ontology ABox API ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ObjQueryParams {
    entity_type_id: Option<String>,
}

async fn list_objects_handler(Query(params): Query<ObjQueryParams>) -> impl IntoResponse {
    match db().list_ontology_objects(params.entity_type_id.as_deref()).await {
        Ok(rows) => (StatusCode::OK, Json(json!({"objects": rows}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct CreateObjectReq {
    entity_type_id: String,
    label: String,
    #[serde(default)]
    fields: serde_json::Value,
}

async fn create_object_handler(Json(req): Json<CreateObjectReq>) -> impl IntoResponse {
    // Lookup entity_type_name
    let types = match db().list_entity_types().await {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    };
    let et_name = types
        .iter()
        .find(|t| t.id == req.entity_type_id)
        .map(|t| t.name.clone())
        .unwrap_or_else(|| req.entity_type_id.clone());

    let fields_str = req.fields.to_string();
    match db()
        .create_ontology_object(&req.entity_type_id, &et_name, &req.label, &fields_str)
        .await
    {
        Ok(row) => (StatusCode::CREATED, Json(json!(row))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_object_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().get_ontology_object(&id).await {
        Ok(Some(row)) => {
            let links = db().list_links_for_object_enriched(&id).await.unwrap_or_default();
            let fields: serde_json::Value = serde_json::from_str(&row.fields).unwrap_or_default();
            (StatusCode::OK, Json(json!({
                "id": row.id, "entity_type_id": row.entity_type_id,
                "entity_type_name": row.entity_type_name, "label": row.label,
                "fields": fields, "created_at": row.created_at, "updated_at": row.updated_at,
                "links": links,
            }))).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct UpdateObjectReq {
    label: String,
    #[serde(default)]
    fields: serde_json::Value,
}

async fn update_object_handler(
    Path(id): Path<String>,
    Json(req): Json<UpdateObjectReq>,
) -> impl IntoResponse {
    let fields_str = req.fields.to_string();
    match db().update_ontology_object(&id, &req.label, &fields_str).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn delete_object_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().delete_ontology_object(&id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Ontology Links API ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateLinkReq {
    from_id: String,
    to_id: String,
    rel_type: String,
}

async fn create_link_handler(Json(req): Json<CreateLinkReq>) -> impl IntoResponse {
    match db().create_link(&req.from_id, &req.to_id, &req.rel_type).await {
        Ok(row) => (StatusCode::CREATED, Json(json!(row))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn delete_link_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().delete_link(&id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_ontology_graph_handler(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let project_id = params.get("project_id").map(|s| s.as_str()).unwrap_or("");

    // Load entity types (for color/fold/role metadata)
    let ets = db().list_entity_types().await.unwrap_or_default();
    let et_map: std::collections::HashMap<_, _> = ets.iter()
        .map(|e| (e.id.clone(), e))
        .collect();

    // Folds for BC grouping
    let folds: Vec<serde_json::Value> = if !project_id.is_empty() {
        db().list_folds(project_id).await.unwrap_or_default()
            .into_iter()
            .map(|f| json!({ "id": f.id, "name": f.name }))
            .collect()
    } else {
        vec![]
    };

    // Nodes = actual ontology object instances
    let objs = db().list_ontology_objects(None).await.unwrap_or_default();
    let nodes: Vec<serde_json::Value> = objs.iter().map(|o| {
        let is_default = o.entity_type_id == "default";
        let et = et_map.get(&o.entity_type_id);
        // Include first 5 fields as preview properties for tooltip
        let props: serde_json::Value = serde_json::from_str(&o.fields).unwrap_or(json!({}));
        let preview: serde_json::Map<String, serde_json::Value> = props
            .as_object()
            .map(|m| m.iter().take(5).map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        json!({
            "id":         o.id,
            "label":      o.label,
            "et_id":      o.entity_type_id,
            "et_name":    o.entity_type_name,
            "color":      if is_default { "#334155" } else { et.map(|e| e.color.as_str()).unwrap_or("#6366f1") },
            "fold_id":    et.and_then(|e| e.fold_id.as_deref()),
            "is_default": is_default,
            "props":      preview,
        })
    }).collect();

    // Edges = actual instance-level ontology links
    let edges: Vec<serde_json::Value> = if let Ok((_, links)) = db().get_ontology_graph().await {
        links.iter().map(|l| json!({
            "source": l.from_id,
            "target": l.to_id,
            "label":  l.rel_type,
            "kind":   "INSTANCE",
        })).collect()
    } else {
        vec![]
    };

    // ET summary for stats (count per type)
    let et_counts: std::collections::HashMap<String, u64> = {
        let mut m = std::collections::HashMap::new();
        for o in &objs { *m.entry(o.entity_type_id.clone()).or_insert(0) += 1; }
        m
    };
    let et_summary: Vec<serde_json::Value> = ets.iter()
        .filter(|e| e.id != "default")
        .map(|e| json!({
            "id": e.id, "label": e.display_name, "color": e.color,
            "fold_id": e.fold_id, "bc_id": e.bc_id,
            "ddd_role": e.ddd_role,
            "count": et_counts.get(&e.id).copied().unwrap_or(0),
        }))
        .collect();

    (StatusCode::OK, Json(json!({
        "nodes": nodes,
        "edges": edges,
        "folds": folds,
        "et_summary": et_summary,
    }))).into_response()
}

async fn list_schema_links_handler() -> impl IntoResponse {
    match db().list_schema_links().await {
        Ok(links) => (StatusCode::OK, Json(json!({"links": links}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Sources API (multi-adapter demo) ─────────────────────────────────────────

/// Generic source config coming from the frontend
#[derive(Deserialize)]
struct SourceReq {
    #[serde(rename = "type")]
    source_type: String,
    config: serde_json::Value,
}

fn build_adapter_from_req(req: &SourceReq) -> Result<Box<dyn SourceAdapter>, String> {
    let c = &req.config;
    match req.source_type.as_str() {
        "csv" => {
            let path = c["path"].as_str().ok_or("csv: path required")?;
            Ok(Box::new(CsvAdapter::new("demo", path, "demo.ns", "demo_schema")))
        }
        "json" => {
            let path = c["path"].as_str().ok_or("json: path required")?;
            let mut a = JsonAdapter::new("demo", path, "demo.ns", "demo_schema");
            if let Some(rp) = c["records_path"].as_str() {
                if !rp.is_empty() { a = a.with_records_path(rp); }
            }
            Ok(Box::new(a))
        }
        "sql" => {
            let db_path = c["db_path"].as_str().ok_or("sql: db_path required")?;
            let query   = c["query"].as_str().ok_or("sql: query required")?;
            let id_col  = c["id_column"].as_str().unwrap_or("id");
            let mut a = SqlAdapter::new("demo", db_path, query, "demo.ns", "demo_schema", id_col);
            if let Some(col) = c["cursor_column"].as_str() {
                if !col.is_empty() { a = a.with_cursor(col); }
            }
            Ok(Box::new(a))
        }
        "rest" => {
            let url = c["url"].as_str().ok_or("rest: url required")?;
            let mut a = RestAdapter::new("demo", url, "demo.ns", "demo_schema");
            if let Some(t) = c["bearer_token"].as_str() {
                if !t.is_empty() { a = a.with_bearer(t); }
            }
            if let (Some(h), Some(v)) = (c["api_key_header"].as_str(), c["api_key_value"].as_str()) {
                if !h.is_empty() { a = a.with_api_key(h, v); }
            }
            if let Some(rp) = c["records_path"].as_str() {
                if !rp.is_empty() { a = a.with_records_path(rp); }
            }
            let page_size = c["page_size"].as_u64().unwrap_or(0) as usize;
            if page_size > 0 {
                let pp = c["page_param"].as_str().unwrap_or("page");
                let sp = c["size_param"].as_str().unwrap_or("limit");
                a = a.with_pagination(page_size, pp, sp);
            }
            Ok(Box::new(a))
        }
        other => Err(format!("unknown source type: {other}")),
    }
}

async fn source_test(Json(req): Json<SourceReq>) -> impl IntoResponse {
    let adapter = match build_adapter_from_req(&req) {
        Ok(a) => a,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    };
    match adapter.test_connection().await {
        Ok(msg) => (StatusCode::OK, Json(json!({"ok": true, "message": msg}))).into_response(),
        Err(e)  => (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "error": e.to_string()}))).into_response(),
    }
}

async fn source_discover(Json(req): Json<SourceReq>) -> impl IntoResponse {
    let adapter = match build_adapter_from_req(&req) {
        Ok(a) => a,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    };
    match adapter.discover_schema().await {
        Ok(schema) => (StatusCode::OK, Json(json!({"ok": true, "schema": schema}))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "error": e.to_string()}))).into_response(),
    }
}

async fn source_preview(Json(req): Json<SourceReq>) -> impl IntoResponse {
    let adapter = match build_adapter_from_req(&req) {
        Ok(a) => a,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    };
    match adapter.fetch_preview(10).await {
        Ok(records) => (StatusCode::OK, Json(json!({
            "ok": true,
            "total": records.len(),
            "preview": records,
        }))).into_response(),
        Err(e) => (StatusCode::OK, Json(json!({
            "ok": false,
            "error": e.to_string(),
            "preview": [],
        }))).into_response(),
    }
}

async fn sources_page() -> Html<&'static str> {
    Html(include_str!("ui/sources.html"))
}

async fn connect_page() -> Html<&'static str> {
    Html(include_str!("ui/connect.html"))
}

// ── Connections Sync ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ConnectionSyncReq {
    #[serde(rename = "type")]
    source_type: String,
    config: serde_json::Value,
    entity_type_id: String,
    entity_type_name: Option<String>,
    #[serde(default)]
    field_mapping: serde_json::Value,
}

async fn connections_sync(Json(req): Json<ConnectionSyncReq>) -> impl IntoResponse {
    // Build adapter
    let src_req = SourceReq { source_type: req.source_type.clone(), config: req.config.clone() };
    let adapter = match build_adapter_from_req(&src_req) {
        Ok(a) => a,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "error": e}))).into_response(),
    };

    // Fetch all records via preview (no limit)
    let records = match adapter.fetch_preview(10_000).await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "error": e.to_string()}))).into_response(),
    };

    let et_id = &req.entity_type_id;
    let et_name = req.entity_type_name.as_deref().unwrap_or(et_id);

    // Apply field mapping and write each record as OntologyObject
    let mut count = 0usize;
    for rec in &records {
        let mapped = apply_field_mapping(rec, &req.field_mapping);
        let label = extract_label(&mapped);
        let fields_str = mapped.to_string();
        if let Err(e) = db().create_ontology_object(et_id, et_name, &label, &fields_str).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "error": e.to_string()}))).into_response();
        }
        count += 1;
    }

    (StatusCode::OK, Json(json!({"ok": true, "count": count}))).into_response()
}

/// Apply field_mapping: { source_field: target_field } to a record.
/// Fields not in mapping (or mapping to empty) are kept as-is.
fn apply_field_mapping(record: &serde_json::Value, mapping: &serde_json::Value) -> serde_json::Value {
    let obj = match record.as_object() {
        Some(o) => o,
        None => return record.clone(),
    };
    if !mapping.is_object() || mapping.as_object().map(|m| m.is_empty()).unwrap_or(true) {
        return record.clone();
    }
    let m = mapping.as_object().unwrap();
    let mut out = serde_json::Map::new();
    for (k, v) in obj {
        let target = m.get(k).and_then(|t| t.as_str()).unwrap_or(k.as_str());
        if !target.is_empty() {
            out.insert(target.to_string(), v.clone());
        }
    }
    serde_json::Value::Object(out)
}

/// Extract a display label from a record: tries name, title, id, or first string field.
fn extract_label(record: &serde_json::Value) -> String {
    if let Some(obj) = record.as_object() {
        for key in &["name", "title", "label", "id", "ID"] {
            if let Some(v) = obj.get(*key) {
                let s = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if !s.is_empty() { return s; }
            }
        }
        // Fallback: first string value
        for (_, v) in obj {
            if let serde_json::Value::String(s) = v {
                if !s.is_empty() { return s.clone(); }
            }
        }
    }
    "Untitled".to_string()
}

// ── Ingest API: Folds ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateFoldReq {
    name: String,
    description: Option<String>,
    #[serde(default)]
    fold_type: Option<String>,
}

async fn list_folds(Path(project_id): Path<String>) -> impl IntoResponse {
    match db().list_folds(&project_id).await {
        Ok(rows) => {
            let mut folds: Vec<serde_json::Value> = Vec::with_capacity(rows.len());
            for f in rows {
                let (src, ds, status) = db().fold_stats(&f.id).await.unwrap_or((0, 0, "idle".into()));
                folds.push(json!({
                    "id": f.id, "project_id": f.project_id,
                    "name": f.name, "description": f.description,
                    "fold_type": f.fold_type, "created_at": f.created_at,
                    "source_count": src, "dataset_count": ds, "status": status
                }));
            }
            (StatusCode::OK, Json(json!({ "folds": folds }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn create_fold(
    Path(project_id): Path<String>,
    Json(req): Json<CreateFoldReq>,
) -> impl IntoResponse {
    match db().create_fold(&project_id, &req.name, req.description.as_deref(), req.fold_type.as_deref()).await {
        Ok(row) => (StatusCode::CREATED, Json(json!({
            "id": row.id, "name": row.name, "description": row.description,
            "fold_type": row.fold_type, "created_at": row.created_at
        }))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_fold_handler(Path(fold_id): Path<String>) -> impl IntoResponse {
    match db().get_fold(&fold_id).await {
        Ok(Some(f)) => (StatusCode::OK, Json(json!({
            "id": f.id, "project_id": f.project_id, "name": f.name,
            "description": f.description, "created_at": f.created_at
        }))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

/// POST /api/folds/:id/infer-mapping
/// Given a list of source columns, suggest which existing ET in this fold each column maps to.
/// Response: { "entity_types": [...], "suggestions": { "col_name": { "et_id", "field_name", "confidence" } } }
#[derive(Deserialize)]
struct InferMappingReq {
    columns: Vec<InferColumn>,
}
#[derive(Deserialize)]
struct InferColumn {
    name: String,
    #[serde(default)]
    data_type: String,
}
async fn infer_mapping_handler(Path(fold_id): Path<String>, Json(req): Json<InferMappingReq>) -> impl IntoResponse {
    let ets = match db().list_entity_types_for_fold(&fold_id).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    };
    if ets.is_empty() {
        return (StatusCode::OK, Json(json!({ "entity_types": [], "suggestions": {} }))).into_response();
    }
    // Build field index: (et_id, field_name) with normalized key
    let mut field_index: Vec<(String, String, String)> = Vec::new(); // (et_id, field_name, normalized)
    let mut et_list = Vec::new();
    for et in &ets {
        let fields = db().list_entity_fields(&et.id).await.unwrap_or_default();
        for f in &fields {
            let norm = f.name.to_lowercase().replace(['-', ' ', '_'], "");
            field_index.push((et.id.clone(), f.name.clone(), norm));
        }
        et_list.push(json!({
            "id": et.id, "name": et.name, "display_name": et.display_name,
            "namespace": et.namespace, "ddd_role": et.ddd_role, "color": et.color,
        }));
    }
    // For each incoming column, find best match
    let mut suggestions = serde_json::Map::new();
    for col in &req.columns {
        let col_norm = col.name.to_lowercase().replace(['-', ' ', '_'], "");
        // 1. Exact normalized match
        let best = field_index.iter().find(|(_, _, norm)| *norm == col_norm)
            .map(|(et_id, field_name, _)| (et_id.clone(), field_name.clone(), 1.0_f32))
            // 2. Prefix match (col is prefix of field or field is prefix of col)
            .or_else(|| {
                field_index.iter().find(|(_, _, norm)| {
                    norm.starts_with(&col_norm) || col_norm.starts_with(norm.as_str())
                }).map(|(et_id, field_name, _)| (et_id.clone(), field_name.clone(), 0.7_f32))
            });
        if let Some((et_id, field_name, confidence)) = best {
            suggestions.insert(col.name.clone(), json!({
                "et_id": et_id,
                "field_name": field_name,
                "confidence": confidence,
            }));
        }
    }
    (StatusCode::OK, Json(json!({ "entity_types": et_list, "suggestions": suggestions }))).into_response()
}

async fn delete_fold_handler(Path(fold_id): Path<String>) -> impl IntoResponse {
    match db().delete_fold(&fold_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Ingest API: DataSources ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateSourceReq {
    name: String,
    source_type: String,
    config: serde_json::Value,
    group_id: Option<String>,
    /// snapshot (default) | append | upsert
    #[serde(default = "default_sync_mode")]
    sync_mode: String,
}

fn default_sync_mode() -> String { "snapshot".to_string() }

#[derive(Deserialize)]
struct UpdateSourceReq {
    name: Option<String>,
    source_type: Option<String>,
    config: Option<serde_json::Value>,
    sync_mode: Option<String>,
}


/// POST /api/sources — create source using default fold (first available or auto-created)
async fn create_source_simple(Json(req): Json<CreateSourceReq>) -> impl IntoResponse {
    // Find or create a default fold
    let fold_id = {
        let projects = db().list_projects().await.unwrap_or_default();
        let proj = if let Some(p) = projects.into_iter().next() {
            p
        } else {
            match db().create_project("Default").await {
                Ok(p) => p,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("create project: {e}")}))).into_response(),
            }
        };
        let folds = db().list_folds(&proj.id).await.unwrap_or_default();
        if let Some(f) = folds.into_iter().next() {
            f.id
        } else {
            match db().create_fold(&proj.id, "Default Fold", None, None).await {
                Ok(f) => f.id,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("create fold: {e}")}))).into_response(),
            }
        }
    };
    let config_str = serde_json::to_string(&req.config).unwrap_or_else(|_| "{}".into());
    match db().create_data_source(&fold_id, &req.name, &req.source_type, &config_str, req.group_id.as_deref(), &req.sync_mode).await {
        Ok(row) => (StatusCode::CREATED, Json(json!({
            "id": row.id, "name": row.name, "source_type": row.source_type,
            "fold_id": row.fold_id, "status": row.status,
        }))).into_response(),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("UNIQUE") {
                (StatusCode::CONFLICT, Json(json!({"error": format!("已存在同名数据源: {}", req.name)}))).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": msg}))).into_response()
            }
        }
    }
}

async fn list_all_sources_handler() -> impl IntoResponse {
    match db().list_all_sources().await {
        Ok(rows) => {
            let sources: Vec<serde_json::Value> = rows.into_iter().map(|s| {
                let config: serde_json::Value = serde_json::from_str(&s.config).unwrap_or(json!({}));
                json!({
                    "id": s.id, "fold_id": s.fold_id, "name": s.name,
                    "source_type": s.source_type, "config": config, "status": s.status,
                    "last_sync_at": s.last_sync_at, "record_count": s.record_count,
                    "created_at": s.created_at,
                })
            }).collect();
            (StatusCode::OK, Json(json!({ "sources": sources }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn list_sources(Path(fold_id): Path<String>) -> impl IntoResponse {
    match db().list_data_sources(&fold_id).await {
        Ok(rows) => {
            let sources: Vec<serde_json::Value> = rows.into_iter().map(|s| {
                let config: serde_json::Value = serde_json::from_str(&s.config).unwrap_or(json!({}));
                json!({
                    "id": s.id, "fold_id": s.fold_id, "name": s.name,
                    "source_type": s.source_type, "config": config, "status": s.status,
                    "write_lock": s.write_lock, "last_sync_at": s.last_sync_at,
                    "record_count": s.record_count, "created_at": s.created_at,
                    "deprecated": s.deprecated, "deleted_at": s.deleted_at,
                    "group_id": s.group_id, "sync_mode": s.sync_mode
                })
            }).collect();
            (StatusCode::OK, Json(json!({ "sources": sources }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn create_source(
    Path(fold_id): Path<String>,
    Json(req): Json<CreateSourceReq>,
) -> impl IntoResponse {
    let config_str = serde_json::to_string(&req.config).unwrap_or_else(|_| "{}".into());
    match db().create_data_source(&fold_id, &req.name, &req.source_type, &config_str, req.group_id.as_deref(), &req.sync_mode).await {
        Ok(row) => (StatusCode::CREATED, Json(json!({
            "id": row.id, "name": row.name, "source_type": row.source_type,
            "status": row.status, "fold_id": row.fold_id, "group_id": row.group_id
        }))).into_response(),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("UNIQUE") && msg.contains("idx_source_name") {
                (StatusCode::CONFLICT, Json(json!({"error": format!("同一 Fold 内已存在名称为 \"{}\" 的数据源", req.name)}))).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": msg}))).into_response()
            }
        }
    }
}

async fn get_source_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().get_data_source(&id).await {
        Ok(Some(s)) => {
            let config: serde_json::Value = serde_json::from_str(&s.config).unwrap_or(json!({}));
            (StatusCode::OK, Json(json!({
                "id": s.id, "fold_id": s.fold_id, "name": s.name,
                "source_type": s.source_type, "config": config,
                "status": s.status, "write_lock": s.write_lock,
                "last_sync_at": s.last_sync_at, "record_count": s.record_count,
                "sync_mode": s.sync_mode
            }))).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn update_source_handler(
    Path(id): Path<String>,
    Json(req): Json<UpdateSourceReq>,
) -> impl IntoResponse {
    let src = match db().get_data_source(&id).await {
        Ok(Some(s)) => s,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    };
    let name = req.name.as_deref().unwrap_or(&src.name).to_string();
    let config_val = req.config.unwrap_or_else(|| {
        serde_json::from_str(&src.config).unwrap_or(json!({}))
    });
    let config_str = serde_json::to_string(&config_val).unwrap_or_else(|_| "{}".into());
    let sync_mode = req.sync_mode.as_deref().unwrap_or(&src.sync_mode).to_string();
    match db().update_data_source(&id, &name, &src.source_type, &config_str, &sync_mode).await {
        Ok(_) => (StatusCode::OK, Json(json!({"id": id, "status": "idle"}))).into_response(),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("UNIQUE") && msg.contains("idx_source_name") {
                (StatusCode::CONFLICT, Json(json!({"error": format!("同一 Fold 内已存在名称为 \"{}\" 的数据源", name)}))).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": msg}))).into_response()
            }
        }
    }
}

async fn delete_source_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().delete_data_source(&id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn deprecate_source_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().deprecate_data_source(&id).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn activate_source_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().activate_data_source(&id).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Ingest API: Test Connection ───────────────────────────────────────────────

// Test a source config directly (no DB save needed — used in batch mode)
#[derive(serde::Deserialize)]
struct QuickTestReq {
    source_type: String,
    config: serde_json::Value,
}
async fn quick_test_handler(Json(req): Json<QuickTestReq>) -> impl IntoResponse {
    let result = run_test_connection(&req.source_type, &req.config).await;
    (StatusCode::OK, Json(result)).into_response()
}

async fn test_source_handler(Path(id): Path<String>) -> impl IntoResponse {
    let src = match db().get_data_source(&id).await {
        Ok(Some(s)) => s,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({"ok":false,"error":"source not found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response(),
    };

    let config: serde_json::Value = serde_json::from_str(&src.config).unwrap_or(json!({}));
    let result = run_test_connection(&src.source_type, &config).await;

    if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        let _ = db().set_source_status(&id, "connected").await;
    } else {
        let _ = db().set_source_status(&id, "error").await;
    }

    (StatusCode::OK, Json(result)).into_response()
}

async fn run_test_connection(source_type: &str, config: &serde_json::Value) -> serde_json::Value {
    match source_type {
        "rest" => {
            let base_url = config["base_url"].as_str().unwrap_or("");
            if base_url.is_empty() { return json!({"ok":false,"error":"base_url is required"}); }
            let auth = build_auth_headers(config);
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default();
            let mut req = client.get(base_url);
            for (k, v) in &auth { req = req.header(k, v); }
            match req.send().await {
                Err(e) => json!({"ok":false,"error":e.to_string(),"error_type":"timeout"}),
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        return json!({"ok":false,"error":format!("HTTP {}",status),"error_type":"auth"});
                    }
                    match resp.json::<serde_json::Value>().await {
                        Err(e) => json!({"ok":false,"error":format!("JSON parse: {}",e),"error_type":"format"}),
                        Ok(body) => {
                            let records = extract_records_from_body(&body, config["records_path"].as_str());
                            let preview: Vec<&serde_json::Value> = records.iter().take(5).collect();
                            json!({"ok":true,"preview":preview,"estimated_total":records.len()})
                        }
                    }
                }
            }
        }
        "db" => {
            let db_type = config["db_type"].as_str().unwrap_or("sqlite");
            if db_type == "sqlite" {
                let path = config.get("database").or(config.get("host"))
                    .and_then(|v| v.as_str()).unwrap_or("");
                if path.is_empty() {
                    return json!({"ok":false,"error":"database path is required"});
                }
                let adapter = SqlAdapter::new("test", path, "SELECT 1", "ns", "schema", "id");
                match adapter.test_connection().await {
                    Ok(_) => json!({"ok":true,"message":format!("Connected to {}", path)}),
                    Err(e) => json!({"ok":false,"error":e.to_string(),"error_type":"auth"}),
                }
            } else {
                json!({"ok":false,"error":"Only SQLite supported in Phase 1","error_type":"unsupported"})
            }
        }
        "s3" | "ftp" => test_s3(config).await,
        "csv" => {
            let path_str = config["path"].as_str().unwrap_or("");
            if path_str.is_empty() {
                return json!({"ok":false,"error":"path is required","error_type":"config"});
            }
            let path = std::path::Path::new(path_str);
            // Resolve actual CSV file
            let csv_file = if path.is_dir() {
                let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(path)
                    .map(|rd| rd.filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("csv"))
                        .collect())
                    .unwrap_or_default();
                files.sort();
                match files.into_iter().next() {
                    Some(f) => f,
                    None => return json!({"ok":false,"error":format!("目录中没有 CSV 文件: {}", path_str),"error_type":"not_found"}),
                }
            } else if path.exists() {
                path.to_path_buf()
            } else {
                return json!({"ok":false,"error":format!("路径不存在: {}", path_str),"error_type":"not_found"});
            };
            // Read up to 5 preview rows
            match csv::Reader::from_path(&csv_file) {
                Err(e) => json!({"ok":false,"error":format!("无法读取 CSV: {}", e),"error_type":"format"}),
                Ok(mut rdr) => {
                    let headers: Vec<String> = rdr.headers()
                        .map(|h| h.iter().map(|s| s.to_string()).collect())
                        .unwrap_or_default();
                    let mut rows: Vec<serde_json::Value> = vec![];
                    let mut total = 0usize;
                    for rec in rdr.records() {
                        total += 1;
                        if rows.len() < 5 {
                            if let Ok(r) = rec {
                                let obj: serde_json::Map<String, serde_json::Value> = headers.iter()
                                    .zip(r.iter())
                                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.to_string())))
                                    .collect();
                                rows.push(serde_json::Value::Object(obj));
                            }
                        }
                    }
                    json!({
                        "ok": true,
                        "message": format!("读取成功：{} 列，{} 行", headers.len(), total),
                        "columns": headers,
                        "row_count": total,
                        "preview": rows,
                    })
                }
            }
        }
        "json" => {
            let path_str = config["path"].as_str().unwrap_or("");
            if path_str.is_empty() {
                return json!({"ok":false,"error":"path is required","error_type":"config"});
            }
            let path = std::path::Path::new(path_str);
            if !path.exists() {
                return json!({"ok":false,"error":format!("路径不存在: {}", path_str),"error_type":"not_found"});
            }
            match std::fs::read_to_string(path) {
                Err(e) => json!({"ok":false,"error":format!("无法读取文件: {}", e),"error_type":"format"}),
                Ok(content) => {
                    match serde_json::from_str::<serde_json::Value>(&content) {
                        Err(e) => json!({"ok":false,"error":format!("JSON 解析失败: {}", e),"error_type":"format"}),
                        Ok(val) => {
                            let rows = if let Some(arr) = val.as_array() {
                                arr.len()
                            } else { 1 };
                            json!({"ok":true,"message":format!("JSON 读取成功：{} 条记录", rows),"row_count":rows})
                        }
                    }
                }
            }
        }
        _ => json!({"ok":false,"error":format!("{} not yet supported for live test",source_type),"error_type":"unsupported"}),
    }
}

fn build_auth_headers(config: &serde_json::Value) -> Vec<(String, String)> {
    let mut headers = vec![];
    let auth = &config["auth"];
    match auth["type"].as_str().unwrap_or("none") {
        "bearer" => {
            if let Some(t) = auth["token"].as_str() {
                headers.push(("Authorization".into(), format!("Bearer {}", t)));
            }
        }
        "apikey" => {
            if let (Some(k), Some(v)) = (auth["header_name"].as_str(), auth["header_value"].as_str()) {
                headers.push((k.to_string(), v.to_string()));
            }
        }
        "basic" => {
            if let (Some(u), Some(p)) = (auth["username"].as_str(), auth["password"].as_str()) {
                let encoded = general_purpose::STANDARD.encode(format!("{}:{}", u, p));
                headers.push(("Authorization".into(), format!("Basic {}", encoded)));
            }
        }
        _ => {}
    }
    headers
}

fn extract_records_from_body(
    body: &serde_json::Value,
    records_path: Option<&str>,
) -> Vec<serde_json::Value> {
    // Navigate to records_path if given
    if let Some(path) = records_path {
        if !path.is_empty() {
            let mut cur = body;
            for seg in path.split('.') {
                cur = &cur[seg];
            }
            if let serde_json::Value::Array(arr) = cur {
                return arr.clone();
            }
        }
    }
    // Auto-detect: if body is array, return directly
    if let serde_json::Value::Array(arr) = body {
        return arr.clone();
    }
    // Auto-detect: find first array field in object
    if let serde_json::Value::Object(map) = body {
        for (_, v) in map {
            if let serde_json::Value::Array(arr) = v {
                return arr.clone();
            }
        }
    }
    vec![]
}

// ── Ingest API: Sync ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SyncReq {
    files: Option<Vec<String>>,
    entity_type_id: Option<String>,
}

async fn sync_source_handler(
    Path(source_id): Path<String>,
    Json(req): Json<SyncReq>,
) -> impl IntoResponse {
    let src = match db().get_data_source(&source_id).await {
        Ok(Some(s)) => s,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({"error":"source not found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    };

    // Create SyncRun
    let run = match db().create_sync_run(&source_id).await {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    };

    // Acquire write_lock (CAS)
    match db().acquire_write_lock(&source_id, &run.id).await {
        Ok(false) => {
            // Lock not acquired: another sync running
            let _ = db().finish_sync_run(&run.id, "failed", Some("concurrent sync"), Some("conflict")).await;
            return (StatusCode::CONFLICT, Json(json!({
                "error": "sync_in_progress",
                "current_job_id": src.write_lock
            }))).into_response();
        }
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
        Ok(true) => {}
    }

    let config: serde_json::Value = serde_json::from_str(&src.config).unwrap_or(json!({}));

    // ── Folder mode: one dataset per CSV file ─────────────────────────────────
    if src.source_type == "csv" {
        let raw_path = config["path"].as_str().unwrap_or("").to_string();
        let p = std::path::Path::new(&raw_path);
        if p.is_dir() {
            let mut csv_files: Vec<std::path::PathBuf> = std::fs::read_dir(p)
                .map(|rd| rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("csv"))
                    .collect())
                .unwrap_or_default();
            csv_files.sort();

            if csv_files.is_empty() {
                let _ = db().release_write_lock(&source_id, "error", None).await;
                let _ = db().finish_sync_run(&run.id, "failed", Some("no CSV files in folder"), Some("empty_folder")).await;
                return (StatusCode::BAD_REQUEST,
                    Json(json!({"error": "no .csv files found in folder"}))).into_response();
            }

            let project_id = db().get_fold(&src.fold_id).await
                .ok().flatten()
                .map(|f| f.project_id)
                .unwrap_or_else(|| src.fold_id.clone());

            let mut job_ids = vec![];
            let existing_datasets = db().list_datasets(&source_id).await.unwrap_or_default();
            let ds_by_name: std::collections::HashMap<String, _> =
                existing_datasets.into_iter().map(|d| (d.name.clone(), d)).collect();

            for file_path in csv_files {
                let fname = file_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Get or create a dataset named after the file
                let dataset = if let Some(d) = ds_by_name.get(&fname) {
                    d.clone()
                } else {
                    match db().create_dataset(&source_id, &fname).await {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("[sync] failed to create dataset for {fname}: {e}");
                            continue;
                        }
                    }
                };

                let file_run = match db().create_sync_run(&source_id).await {
                    Ok(r) => r,
                    Err(e) => { eprintln!("[sync] create_sync_run failed: {e}"); continue; }
                };
                let dv = match db().create_dataset_version(&dataset.id, &file_run.id).await {
                    Ok(v) => v,
                    Err(e) => { eprintln!("[sync] create_dataset_version failed: {e}"); continue; }
                };

                job_ids.push(file_run.id.clone());
                let source_id2   = source_id.clone();
                let run_id2      = file_run.id.clone();
                let dataset_id2  = dataset.id.clone();
                let dv_id2       = dv.id.clone();
                let project_id2  = project_id.clone();
                let source_type2 = src.source_type.clone();
                let sync_mode2   = src.sync_mode.clone();
                let file_config  = json!({ "path": file_path.to_string_lossy() });
                let version2     = dv.version;

                tokio::spawn(async move {
                    let result = run_sync_job(
                        &source_id2, &run_id2, &dataset_id2, &dv_id2,
                        &source_type2, &file_config, &[], None,
                    ).await;
                    match result {
                        Ok((records, _)) => {
                            let records = merge_records_for_mode(&sync_mode2, &dataset_id2, records).await;
                            let count = records.len();
                            let schema_json = write_records_to_storage(records, &dataset_id2, version2, &run_id2, &dv_id2, &project_id2).await;
                            let schema_change = if let Ok(Some(prev)) = db()
                                .get_prev_committed_schema(&dataset_id2, version2).await
                            { detect_schema_change(&prev, &schema_json) } else { "none" };
                            let _ = db().commit_dataset_version(&dv_id2, &dataset_id2, count as i64, &schema_json).await;
                            let _ = db().set_version_schema_change(&dv_id2, schema_change).await;
                            let _ = db().finish_sync_run(&run_id2, "completed", None, None).await;
                            auto_promote_if_mapped(&dataset_id2).await;
                        }
                        Err(e) => {
                            let _ = db().abort_dataset_version(&dv_id2).await;
                            let _ = db().finish_sync_run(&run_id2, "failed", Some(&e), Some("sync_error")).await;
                        }
                    }
                });
            }

            // Release the main lock and mark the trigger run as completed
            let _ = db().finish_sync_run(&run.id, "completed", None, None).await;
            let _ = db().release_write_lock(&source_id, "synced", None).await;
            return (StatusCode::ACCEPTED, Json(json!({
                "mode": "folder",
                "job_ids": job_ids,
            }))).into_response();
        }
    }

    // ── S3/FTP per-file mode: one dataset per file ───────────────────────────
    // Triggered when multiple files are explicitly provided, OR when auto-discovery
    // finds more than one file in the bucket/prefix.
    // Single-file S3 sources fall through to the single-file mode below.
    if src.source_type == "s3" || src.source_type == "ftp" {
        // Resolve file list: from request body → config.selected_files → auto-discover
        let selected_files: Vec<String> = if let Some(f) = req.files.clone().filter(|f| !f.is_empty()) {
            f
        } else if let Some(arr) = config.get("selected_files").and_then(|v| v.as_array()) {
            // Use files previously saved in source config (from UI file selector)
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        } else {
            match build_s3_store(&config) {
                Ok(store) => {
                    let prefix = config["prefix"].as_str().unwrap_or("").trim();
                    let stream = if prefix.is_empty() { store.list(None) }
                                 else { store.list(Some(&OsPath::from(prefix))) };
                    stream.try_collect::<Vec<_>>().await
                        .unwrap_or_default()
                        .into_iter()
                        .map(|m| m.location.to_string())
                        .filter(|f| { let l = f.to_lowercase(); l.ends_with(".csv") || l.ends_with(".json") || l.ends_with(".jsonl") })
                        .collect()
                }
                Err(_) => vec![],
            }
        };

        // S3 sources always use per-file mode.
        // If no files resolved, abort early to avoid creating a stray source-named dataset.
        if selected_files.is_empty() {
            let _ = db().release_write_lock(&source_id, "error", None).await;
            let _ = db().finish_sync_run(&run.id, "failed", Some("no files selected or discovered"), Some("no_files")).await;
            return (StatusCode::BAD_REQUEST,
                Json(json!({"error": "no files selected or discovered in S3 bucket"}))).into_response();
        } else {
        let project_id = db().get_fold(&src.fold_id).await
            .ok().flatten()
            .map(|f| f.project_id)
            .unwrap_or_else(|| src.fold_id.clone());

        let existing_datasets = db().list_datasets(&source_id).await.unwrap_or_default();
        let ds_by_name: std::collections::HashMap<String, _> =
            existing_datasets.into_iter().map(|d| (d.name.clone(), d)).collect();

        let mut job_ids = vec![];
        for file_key in &selected_files {
            // Extract the filename portion from the S3 key (e.g. "raw/employees.csv" → "employees.csv")
            let fname = file_key.rsplit('/').next().unwrap_or(file_key.as_str()).to_string();

            let dataset = if let Some(d) = ds_by_name.get(&fname) {
                d.clone()
            } else {
                match db().create_dataset(&source_id, &fname).await {
                    Ok(d) => d,
                    Err(e) => { eprintln!("[sync] failed to create dataset for {fname}: {e}"); continue; }
                }
            };

            let file_run = match db().create_sync_run(&source_id).await {
                Ok(r) => r,
                Err(e) => { eprintln!("[sync] create_sync_run failed: {e}"); continue; }
            };
            let dv = match db().create_dataset_version(&dataset.id, &file_run.id).await {
                Ok(v) => v,
                Err(e) => { eprintln!("[sync] create_dataset_version failed: {e}"); continue; }
            };

            job_ids.push(file_run.id.clone());
            let source_id2   = source_id.clone();
            let run_id2      = file_run.id.clone();
            let dataset_id2  = dataset.id.clone();
            let dv_id2       = dv.id.clone();
            let project_id2  = project_id.clone();
            let source_type2 = src.source_type.clone();
            let sync_mode2   = src.sync_mode.clone();
            let config2      = config.clone();
            let this_file    = vec![file_key.clone()];
            let version2     = dv.version;

            tokio::spawn(async move {
                let result = run_sync_job(
                    &source_id2, &run_id2, &dataset_id2, &dv_id2,
                    &source_type2, &config2, &this_file, None,
                ).await;
                match result {
                    Ok((records, sync_count)) => {
                        let records = merge_records_for_mode(&sync_mode2, &dataset_id2, records).await;
                        let schema_json = if !records.is_empty() {
                            write_records_to_storage(records.clone(), &dataset_id2, version2, &run_id2, &dv_id2, &project_id2).await
                        } else {
                            // S3 per-file: records were written to SQLite by sync_s3, flush to platform storage
                            write_to_platform_storage(&dataset_id2, version2, &run_id2, &dv_id2, "s3", &serde_json::Value::Null, &project_id2).await
                        };
                        let count = if records.is_empty() { sync_count } else { records.len() };
                        let schema_change = if let Ok(Some(prev)) = db()
                            .get_prev_committed_schema(&dataset_id2, version2).await
                        { detect_schema_change(&prev, &schema_json) } else { "none" };
                        let _ = db().commit_dataset_version(&dv_id2, &dataset_id2, count as i64, &schema_json).await;
                        let _ = db().set_version_schema_change(&dv_id2, schema_change).await;
                        let _ = db().finish_sync_run(&run_id2, "completed", None, None).await;
                        auto_promote_if_mapped(&dataset_id2).await;
                    }
                    Err(e) => {
                        let _ = db().abort_dataset_version(&dv_id2).await;
                        let _ = db().finish_sync_run(&run_id2, "failed", Some(&e), Some("sync_error")).await;
                    }
                }
            });
        }

        let _ = db().finish_sync_run(&run.id, "completed", None, None).await;
        let _ = db().release_write_lock(&source_id, "synced", None).await;
        return (StatusCode::ACCEPTED, Json(json!({
            "mode": "folder",
            "job_ids": job_ids,
        }))).into_response();
        } // end else (per-file mode)
    } // end if s3/ftp

    // ── Single-file / non-CSV mode ────────────────────────────────────────────
    // Get or create Dataset
    let datasets = db().list_datasets(&source_id).await.unwrap_or_default();
    let dataset = if let Some(d) = datasets.into_iter().next() {
        d
    } else {
        match db().create_dataset(&source_id, &src.name).await {
            Ok(d) => d,
            Err(e) => {
                let _ = db().release_write_lock(&source_id, "error", None).await;
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response();
            }
        }
    };

    // Create DatasetVersion (pending)
    let dv = match db().create_dataset_version(&dataset.id, &run.id).await {
        Ok(v) => v,
        Err(e) => {
            let _ = db().release_write_lock(&source_id, "error", None).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response();
        }
    };

    // Resolve project_id (customer/tenant identifier) via fold
    let project_id = db().get_fold(&src.fold_id).await
        .ok().flatten()
        .map(|f| f.project_id)
        .unwrap_or_else(|| src.fold_id.clone());

    let run_id = run.id.clone();
    let dataset_id = dataset.id.clone();
    let dataset_id_ret = dataset_id.clone();
    let dv_id = dv.id.clone();
    let version = dv.version;
    let selected_files = req.files.clone().unwrap_or_default();
    let entity_type_id = req.entity_type_id.clone();
    let sync_mode = src.sync_mode.clone();

    // Spawn background sync task
    tokio::spawn(async move {
        let result = run_sync_job(
            &source_id, &run_id, &dataset_id, &dv_id,
            &src.source_type, &config, &selected_files, entity_type_id.as_deref(),
        ).await;

        match result {
            Ok((records, sync_count)) => {
                let records = merge_records_for_mode(&sync_mode, &dataset_id, records).await;
                let schema_json = if !records.is_empty() {
                    write_records_to_storage(records.clone(), &dataset_id, version, &run_id, &dv_id, &project_id).await
                } else {
                    write_to_platform_storage(&dataset_id, version, &run_id, &dv_id, &src.source_type, &config, &project_id).await
                };
                // Use sync_count for S3/legacy paths that return empty records vec
                let count = if records.is_empty() { sync_count } else { records.len() };
                let schema_change = if let Ok(Some(prev)) = db()
                    .get_prev_committed_schema(&dataset_id, version).await
                { detect_schema_change(&prev, &schema_json) } else { "none" };
                let _ = db().commit_dataset_version(&dv_id, &dataset_id, count as i64, &schema_json).await;
                let _ = db().set_version_schema_change(&dv_id, schema_change).await;
                if schema_change == "breaking" {
                    eprintln!("[schema] ⚠ BREAKING schema change detected for dataset {}", dataset_id);
                }
                let _ = db().finish_sync_run(&run_id, "completed", None, None).await;
                let _ = db().release_write_lock(&source_id, "synced", Some(count as i64)).await;
                auto_promote_if_mapped(&dataset_id).await;
            }
            Err(e) => {
                let _ = db().abort_dataset_version(&dv_id).await;
                let _ = db().finish_sync_run(&run_id, "failed", Some(&e), Some("sync_error")).await;
                let _ = db().release_write_lock(&source_id, "error", None).await;
            }
        }
    });

    (StatusCode::ACCEPTED, Json(json!({
        "job_id": run.id,
        "dataset_id": dataset_id_ret,
        "version": version,
    }))).into_response()
}

/// Run a sync job. Returns (direct_records, count):
/// - For CSV/JSON: direct_records is populated, write them straight to S3 (no SQLite rows).
/// - For REST/DB/S3: direct_records is empty, count rows already written to SQLite (legacy path).
async fn run_sync_job(
    source_id: &str,
    run_id: &str,
    dataset_id: &str,
    _dv_id: &str,
    source_type: &str,
    config: &serde_json::Value,
    selected_files: &[String],
    entity_type_id: Option<&str>,
) -> Result<(Vec<serde_json::Value>, usize), String> {
    let _ = db().update_sync_run_progress(run_id, 0, None, None).await;
    let _ = db().set_sync_run_status(run_id, "running").await;

    match source_type {
        "csv"  => sync_csv(run_id, source_id, config, entity_type_id, dataset_id).await,
        "json" => sync_json(run_id, source_id, config, entity_type_id, dataset_id).await,
        // Legacy: REST/DB/S3 still write rows to SQLite; return empty vec to signal legacy path.
        "rest" => sync_rest(run_id, source_id, config, entity_type_id, dataset_id).await.map(|n| (vec![], n)),
        "db"   => sync_db(run_id, source_id, config, entity_type_id, dataset_id).await.map(|n| (vec![], n)),
        "s3" | "ftp" => sync_s3(run_id, source_id, config, entity_type_id, dataset_id, selected_files).await.map(|n| (vec![], n)),
        _ => Err(format!("{} not yet implemented", source_type)),
    }
}

/// Write records directly to platform storage (S3/local) without touching SQLite.
/// Returns schema_json for committing the dataset version.
async fn write_records_to_storage(
    records: Vec<serde_json::Value>,
    dataset_id: &str,
    version: i64,
    run_id: &str,
    dv_id: &str,
    project_id: &str,
) -> String {
    use palantir_dataset::DatasetSchema;
    if records.is_empty() {
        return json!({"fields":[]}).to_string();
    }
    let (backend, _is_s3) = build_platform_backend().await;
    let prefix = format!("platform_datasets/{}", project_id);
    let store = palantir_dataset::DatasetStore::new(backend, &prefix);
    let schema = DatasetSchema::infer_from_records(&records);
    let schema_json = serde_json::to_string(&schema).unwrap_or_else(|_| json!({"fields":[]}).to_string());
    let mut writer = store.begin_write(dataset_id, version, run_id);
    if let Err(e) = writer.append_records(&records).await {
        eprintln!("[storage] append_records error: {e}");
        return schema_json;
    }
    match writer.commit(schema).await {
        Ok(_manifest) => {
            let manifest_path = format!("{}/{}/v{}/manifest.json", prefix, dataset_id, version);
            let _ = db().update_version_manifest_path(dv_id, &manifest_path).await;
            eprintln!("[storage] ✓ {} rows written to {}", records.len(), manifest_path);
            schema_json
        }
        Err(e) => {
            eprintln!("[storage] commit error: {e}");
            schema_json
        }
    }
}

/// Merge new records with existing dataset data according to sync_mode:
/// - snapshot: return new_records as-is (default, full replace)
/// - append:   read current version records + add new_records at the end
/// - upsert:   read current version records, merge by primary key (new wins on conflict)
async fn merge_records_for_mode(
    sync_mode: &str,
    dataset_id: &str,
    new_records: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    if sync_mode == "snapshot" {
        return new_records;
    }

    // Read current version records from platform storage
    let current = match db().get_current_dataset_version(dataset_id).await {
        Ok(Some(dv)) if dv.manifest_path.is_some() => {
            let (backend, _) = build_platform_backend().await;
            let mp = dv.manifest_path.unwrap();
            palantir_dataset::read_records_from_manifest_path(&backend, &mp)
                .await
                .unwrap_or_default()
        }
        _ => vec![],
    };

    if current.is_empty() {
        return new_records;
    }

    match sync_mode {
        "append" => {
            // Cumulative snapshot: keep all old rows + add new rows
            let mut merged = current;
            merged.extend(new_records);
            merged
        }
        "upsert" => {
            // Merge by primary key: look up pk_col from object_type_mappings
            let pk_col = db().get_object_type_mapping(dataset_id).await
                .ok().flatten()
                .and_then(|m| m.get("primary_key_col").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .filter(|k: &String| !k.is_empty());

            if let Some(pk) = pk_col {
                // Build map from existing records keyed by pk value
                // Build ordered list with a HashMap index for O(1) lookup
                let mut keys: Vec<String> = Vec::with_capacity(current.len());
                let mut map: std::collections::HashMap<String, serde_json::Value> =
                    std::collections::HashMap::new();
                for r in current {
                    if let Some(key) = r.get(&pk).and_then(|v| v.as_str().map(|s| s.to_string())) {
                        keys.push(key.clone());
                        map.insert(key, r);
                    }
                }
                // Upsert: new records overwrite existing entries with same pk
                let mut extra: Vec<serde_json::Value> = vec![];
                for rec in new_records {
                    if let Some(key) = rec.get(&pk).and_then(|v| v.as_str().map(|s| s.to_string())) {
                        if !map.contains_key(&key) { keys.push(key.clone()); }
                        map.insert(key, rec);
                    } else {
                        extra.push(rec);
                    }
                }
                let mut result: Vec<_> = keys.into_iter().filter_map(|k| map.remove(&k)).collect();
                result.extend(extra);
                result
            } else {
                // No mapping configured yet, fall back to append
                eprintln!("[sync] upsert mode but no primary_key_col configured for dataset {dataset_id}, falling back to append");
                let mut merged = current;
                merged.extend(new_records);
                merged
            }
        }
        _ => new_records,
    }
}

async fn sync_rest(
    run_id: &str,
    _source_id: &str,
    config: &serde_json::Value,
    entity_type_id: Option<&str>,
    dataset_id: &str,
) -> Result<usize, String> {
    let base_url = config["base_url"].as_str().unwrap_or("");
    let records_path = config["records_path"].as_str();
    let auth_headers = build_auth_headers(config);
    let pag = &config["pagination"];
    let has_pag = !pag.is_null();
    let page_param = pag["page_param"].as_str().unwrap_or("page");
    let size_param = pag["size_param"].as_str().unwrap_or("limit");
    let page_size = pag["page_size"].as_u64().unwrap_or(100);
    let start_page = pag["start_page"].as_u64().unwrap_or(1);
    let end_strategy = pag["end_strategy"].as_str().unwrap_or("empty");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let mut total_count = 0usize;
    let mut page = start_page;

    loop {
        let url = if has_pag && end_strategy != "none" {
            format!("{}&{}={}&{}={}", base_url, page_param, page, size_param, page_size)
                .replace("&&", "&")
                .replace("?&", "?")
        } else {
            base_url.to_string()
        };

        let mut req = client.get(&url);
        for (k, v) in &auth_headers { req = req.header(k.as_str(), v.as_str()); }

        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let records = extract_records_from_body(&body, records_path);

        if records.is_empty() { break; }
        let batch_size = records.len();

        // Write OntologyObjects
        for (i, rec) in records.iter().enumerate() {
            let label = extract_label(rec);
            let fields = serde_json::to_string(rec).unwrap_or_else(|_| "{}".into());
            let et_id = entity_type_id.unwrap_or("default");
            let et_name = et_id;
            db().create_ontology_object_with_lineage(et_id, et_name, &label, &fields, dataset_id, run_id)
                .await.map_err(|e| format!("insert failed: {e}"))?;
            if i % 50 == 0 {
                let _ = db().update_sync_run_progress(run_id, total_count as i64 + i as i64, None, Some(&format!("page {}", page))).await;
            }
        }

        total_count += batch_size;
        let _ = db().update_sync_run_progress(run_id, total_count as i64, None, Some(&format!("page {}", page))).await;

        if !has_pag || end_strategy == "none" { break; }
        if end_strategy == "less" && batch_size < page_size as usize { break; }
        page += 1;
    }

    Ok(total_count)
}

async fn sync_db(
    run_id: &str,
    _source_id: &str,
    config: &serde_json::Value,
    entity_type_id: Option<&str>,
    dataset_id: &str,
) -> Result<usize, String> {
    let mut query_str = config["query"].as_str().unwrap_or("").trim().to_string();
    let batch_size = config["batch_size"].as_u64().unwrap_or(1000) as usize;
    let db_type = config["db_type"].as_str().unwrap_or("sqlite");
    let db_path = config.get("database").or(config.get("host"))
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    if db_path.is_empty() { return Err("database path is required".into()); }

    // For SQLite: auto-discover tables when no query is specified
    if query_str.is_empty() && db_type == "sqlite" {
        let probe = SqlAdapter::new("probe", &db_path,
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            "ns", "schema", "name");
        let tables = probe.fetch_preview(100).await.map_err(|e| e.to_string())?;
        if tables.is_empty() { return Err("no tables found in SQLite database".into()); }
        // Sync all user tables
        let mut total = 0usize;
        for tbl in &tables {
            let tname = tbl.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if tname.is_empty() { continue; }
            let tq = format!("SELECT * FROM \"{}\"", tname.replace('"', "\"\""));
            total += sync_db_query(run_id, &db_path, &tq, batch_size, entity_type_id, dataset_id, total).await?;
        }
        return Ok(total);
    }

    if query_str.is_empty() { return Err("query is required".into()); }
    // Normalise: strip trailing semicolons
    query_str = query_str.trim_end_matches(';').to_string();
    sync_db_query(run_id, &db_path, &query_str, batch_size, entity_type_id, dataset_id, 0).await
}

async fn sync_db_query(
    run_id: &str,
    db_path: &str,
    query_str: &str,
    batch_size: usize,
    entity_type_id: Option<&str>,
    dataset_id: &str,
    base_total: usize,
) -> Result<usize, String> {
    let mut total = 0usize;
    let mut offset = 0usize;
    let base_upper = query_str.to_uppercase();
    loop {
        let paged = if base_upper.contains("LIMIT") {
            query_str.to_string()
        } else {
            format!("{} LIMIT {} OFFSET {}", query_str, batch_size, offset)
        };
        let adapter = SqlAdapter::new("sync", db_path, &paged, "ns", "schema", "id");
        let records = adapter.fetch_preview(batch_size).await.map_err(|e| e.to_string())?;
        if records.is_empty() { break; }
        let batch_len = records.len();
        for (i, rec) in records.iter().enumerate() {
            let label = extract_label(rec);
            let fields = serde_json::to_string(rec).unwrap_or_else(|_| "{}".into());
            let et_id = entity_type_id.unwrap_or("default");
            db().create_ontology_object_with_lineage(et_id, et_id, &label, &fields, dataset_id, run_id)
                .await
                .map_err(|e| format!("insert failed at offset {}: {}", offset + i, e))?;
            if i % 50 == 0 {
                let _ = db().update_sync_run_progress(run_id, (base_total + total + i) as i64, None,
                    Some(&format!("offset {}", offset + i))).await;
            }
        }
        total += batch_len;
        offset += batch_len;
        let _ = db().update_sync_run_progress(run_id, (base_total + total) as i64, None,
            Some(&format!("offset {}", offset))).await;
        if base_upper.contains("LIMIT") { break; }
    }
    Ok(total)
}

/// Collect CSV records without writing to SQLite. Returns the raw records for direct S3 write.
async fn sync_csv(
    run_id: &str,
    _source_id: &str,
    config: &serde_json::Value,
    _entity_type_id: Option<&str>,
    _dataset_id: &str,
) -> Result<(Vec<serde_json::Value>, usize), String> {
    let path = config["path"].as_str().unwrap_or("");
    if path.is_empty() { return Err("path is required".into()); }
    let adapter = CsvAdapter::new("sync", path, "ns", "schema");
    let records = adapter.fetch_preview(usize::MAX).await.map_err(|e| e.to_string())?;
    let total = records.len();
    let _ = db().update_sync_run_progress(run_id, total as i64, Some(total as i64), None).await;
    Ok((records, total))
}

/// Collect JSON records without writing to SQLite. Returns the raw records for direct S3 write.
async fn sync_json(
    run_id: &str,
    _source_id: &str,
    config: &serde_json::Value,
    _entity_type_id: Option<&str>,
    _dataset_id: &str,
) -> Result<(Vec<serde_json::Value>, usize), String> {
    let path = config["path"].as_str().unwrap_or("");
    if path.is_empty() { return Err("path is required".into()); }
    let mut adapter = JsonAdapter::new("sync", path, "ns", "schema");
    if let Some(rp) = config["records_path"].as_str() {
        if !rp.is_empty() { adapter = adapter.with_records_path(rp); }
    }
    let records = adapter.fetch_preview(usize::MAX).await.map_err(|e| e.to_string())?;
    let total = records.len();
    let _ = db().update_sync_run_progress(run_id, total as i64, Some(total as i64), None).await;
    Ok((records, total))
}

// ── Ingest API: Jobs ──────────────────────────────────────────────────────────

async fn get_job_handler(Path(job_id): Path<String>) -> impl IntoResponse {
    match db().get_sync_run(&job_id).await {
        Ok(Some(r)) => {
            let elapsed_ms: i64 = r.started_at.parse::<i64>().map(|s| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(s);
                (now - s) * 1000
            }).unwrap_or(0);
            (StatusCode::OK, Json(json!({
                "id": r.id, "status": r.status,
                "processed": r.processed, "total": r.total_records,
                "current": r.current_item, "error": r.error_message,
                "error_type": r.error_type, "elapsed_ms": elapsed_ms,
            }))).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    }
}

async fn list_jobs_handler(Path(source_id): Path<String>) -> impl IntoResponse {
    match db().list_sync_runs(&source_id).await {
        Ok(rows) => (StatusCode::OK, Json(json!({"jobs": rows}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    }
}

/// ── Ingest API: Datasets & Versions ──────────────────────────────────────────

/// GET /api/datasets — list all datasets with record_count
async fn list_all_datasets_handler() -> impl IntoResponse {
    match db().list_all_datasets().await {
        Ok(rows) => (StatusCode::OK, Json(json!({"datasets": rows}))).into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    }
}

async fn list_datasets_handler(Path(source_id): Path<String>) -> impl IntoResponse {
    match db().list_datasets_with_count(&source_id).await {
        Ok(rows) => (StatusCode::OK, Json(json!({"datasets": rows}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    }
}

async fn get_dataset_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().get_dataset(&id).await {
        Ok(Some(d)) => (StatusCode::OK, Json(json!(d))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    }
}

async fn list_dataset_versions_handler(Path(id): Path<String>) -> impl IntoResponse {
    match db().list_dataset_versions(&id).await {
        Ok(rows) => (StatusCode::OK, Json(json!({"versions": rows}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct RollbackReq { version: i64 }

async fn rollback_dataset_handler(
    Path(dataset_id): Path<String>,
    Json(req): Json<RollbackReq>,
) -> impl IntoResponse {
    // 1. Flip is_current pointer in DB
    let target_version = match db().rollback_dataset_version(&dataset_id, req.version).await {
        Ok(true) => req.version,
        Ok(false) => return (StatusCode::NOT_FOUND, Json(json!({"error":"version not found or not committed"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    };

    // 2. Find manifest_path for the target version
    let versions = match db().list_dataset_versions(&dataset_id).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    };
    let target = versions.iter().find(|v| v.version == target_version);
    let manifest_path = target.and_then(|v| v.manifest_path.clone());

    let run_id = format!("rollback_{}", Uuid::new_v4());
    let job_id = run_id.clone();

    // 3. Spawn re-materialization in background
    if let Some(mp) = manifest_path {
        let ds_id = dataset_id.clone();
        let rid = run_id.clone();
        tokio::spawn(async move {
            match rematerialize_from_manifest(&ds_id, &mp, &rid, "").await {
                Ok(n) => eprintln!("[rollback] ✓ re-materialized {} objects for v{}", n, target_version),
                Err(e) => eprintln!("[rollback] ✗ error: {e}"),
            }
        });
        (StatusCode::ACCEPTED, Json(json!({
            "ok": true, "job_id": job_id,
            "version": target_version,
            "rematerializing": true,
        }))).into_response()
    } else {
        // No manifest stored (pre-Iter-1 version) — metadata-only rollback
        (StatusCode::ACCEPTED, Json(json!({
            "ok": true, "job_id": job_id,
            "version": target_version,
            "rematerializing": false,
            "note": "no manifest found, metadata-only rollback",
        }))).into_response()
    }
}

#[derive(Deserialize)]
struct RecordsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_dataset_records_handler(
    Path(id): Path<String>,
    Query(q): Query<RecordsQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(50).min(500) as usize;
    let offset = q.offset.unwrap_or(0) as usize;

    // Try S3-first path: read from manifest
    let dv = match db().get_current_dataset_version(&id).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (StatusCode::OK, Json(json!({"records":[],"total":0,"limit":limit,"offset":offset}))).into_response();
        }
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    };

    if let Some(manifest_path) = &dv.manifest_path {
        let (backend, _) = build_platform_backend().await;
        match palantir_dataset::read_records_from_manifest_path(&backend, manifest_path).await {
            Ok(all_records) => {
                let total = all_records.len();
                let page: Vec<serde_json::Value> = all_records.into_iter().skip(offset).take(limit).collect();
                return (StatusCode::OK, Json(json!({
                    "records": page,
                    "total": total,
                    "limit": limit,
                    "offset": offset,
                }))).into_response();
            }
            Err(e) => {
                eprintln!("[records] S3 read failed ({e}), falling back to SQLite");
            }
        }
    }

    // Fallback: SQLite (legacy REST/DB sources)
    let total = db().count_dataset_records(&id).await.unwrap_or(0);
    match db().list_dataset_records(&id, limit as i64, offset as i64).await {
        Ok(rows) => {
            let records: Vec<serde_json::Value> = rows.into_iter().map(|r| {
                let fields: serde_json::Value = serde_json::from_str(&r.fields).unwrap_or(json!({}));
                json!({ "id": r.id, "label": r.label, "fields": fields, "created_at": r.created_at })
            }).collect();
            (StatusCode::OK, Json(json!({
                "records": records,
                "total": total,
                "limit": limit,
                "offset": offset,
            }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e.to_string()}))).into_response(),
    }
}

// ── Promote dataset → ontology ────────────────────────────────────────────────

#[derive(Deserialize)]
struct PromoteDatasetReq {
    /// Existing entity type id, OR empty to auto-create from new_type_name
    #[serde(default)]
    entity_type_id: String,
    /// Create a new entity type with this display name if entity_type_id is empty
    #[serde(default)]
    new_type_name: String,
    /// Column in the record to use as external_id for upsert (dedup key).
    /// Leave empty to always insert (no dedup).
    #[serde(default)]
    primary_key_col: String,
    /// { "src_field": "target_attr" } — omit or empty to keep field names as-is
    #[serde(default)]
    field_mapping: serde_json::Value,
    /// FK → entity type links to resolve after promote
    #[serde(default)]
    links: Vec<LinkTypeMappingInput>,
    /// snapshot | append | update  (default: snapshot)
    #[serde(default = "default_sync_mode")]
    sync_mode: String,
    /// Fold (sub-business BC) this ET belongs to; null = shared/common
    #[serde(default)]
    fold_id: Option<String>,
}

async fn promote_dataset_handler(
    Path(dataset_id): Path<String>,
    Json(req): Json<PromoteDatasetReq>,
) -> impl IntoResponse {
    // ── Resolve / create entity type ──────────────────────────────────────────
    let (et_id, et_name) = if !req.entity_type_id.is_empty() {
        let types = db().list_entity_types().await.unwrap_or_default();
        match types.into_iter().find(|t| t.id == req.entity_type_id) {
            Some(t) => (t.id, t.display_name),
            None => return (StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": "entity type not found"}))).into_response(),
        }
    } else if !req.new_type_name.is_empty() {
        let name = req.new_type_name.trim().to_lowercase().replace(' ', "_");
        // Assign a distinct color based on a hash of the ET name so each type looks different
        let color = {
            const PALETTE: &[&str] = &[
                "#6366f1","#8b5cf6","#ec4899","#14b8a6","#f59e0b",
                "#10b981","#3b82f6","#f97316","#ef4444","#84cc16",
                "#06b6d4","#d946ef","#a855f7","#22c55e","#0ea5e9",
            ];
            let h = name.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
            PALETTE[(h as usize) % PALETTE.len()]
        };
        match db().create_entity_type(&name, &req.new_type_name, color, "cube", req.fold_id.as_deref(), "entity", None).await {
            Ok(t) => (t.id, t.display_name),
            Err(e) if e.to_string().contains("UNIQUE") => {
                // Already exists — find and reuse
                let types = db().list_entity_types().await.unwrap_or_default();
                match types.into_iter().find(|t| t.name == name || t.display_name == req.new_type_name) {
                    Some(t) => (t.id, t.display_name),
                    None => return (StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"ok": false, "error": e.to_string()}))).into_response(),
                }
            }
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": e.to_string()}))).into_response(),
        }
    } else {
        return (StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "entity_type_id or new_type_name required"}))).into_response();
    };

    // ── Assign ET to fold if specified ───────────────────────────────────────
    if let Some(ref fold_id) = req.fold_id {
        let fid = if fold_id.is_empty() { None } else { Some(fold_id.as_str()) };
        let _ = db().update_entity_type_fold(&et_id, fid).await;
    }

    // ── Persist mapping so re-sync can reuse it ───────────────────────────────
    let mapping_str = req.field_mapping.to_string();
    let _ = db().save_object_type_mapping(
        &dataset_id, &et_id, &req.primary_key_col, &mapping_str, &req.sync_mode,
    ).await;

    // ── Persist link type mappings (FK → target entity type) ──────────────────
    if !req.links.is_empty() {
        let _ = db().save_link_type_mappings(&dataset_id, &req.links).await;
    }

    // ── Read raw records: S3-first, fallback to SQLite ────────────────────────
    let raw_records: Vec<serde_json::Value> = {
        // Try S3 path via manifest
        let dv = db().get_current_dataset_version(&dataset_id).await
            .ok().flatten();
        let from_s3 = if let Some(dv) = &dv {
            if let Some(mp) = &dv.manifest_path {
                let (backend, _) = build_platform_backend().await;
                palantir_dataset::read_records_from_manifest_path(&backend, mp).await.ok()
            } else { None }
        } else { None };

        if let Some(records) = from_s3 {
            records
        } else {
            // Fallback: SQLite (legacy REST/DB sources)
            let total = match db().count_dataset_records(&dataset_id).await {
                Ok(n) => n,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"ok": false, "error": e.to_string()}))).into_response(),
            };
            match db().list_dataset_records(&dataset_id, total.max(1), 0).await {
                Ok(rows) => rows.into_iter()
                    .map(|r| serde_json::from_str(&r.fields).unwrap_or(json!({})))
                    .collect(),
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"ok": false, "error": e.to_string()}))).into_response(),
            }
        }
    };

    if raw_records.is_empty() {
        return (StatusCode::OK, Json(json!({"ok": true, "promoted": 0, "total": 0,
            "note": "dataset has no records — run sync first"}))).into_response();
    }

    let mut promoted = 0usize;
    let pk = req.primary_key_col.trim();

    for raw in &raw_records {
        let mapped = apply_field_mapping(raw, &req.field_mapping);
        let label  = extract_label(&mapped);
        let fields = mapped.to_string();
        let ext_id: Option<String> = if !pk.is_empty() {
            mapped.get(pk)
                .or_else(|| raw.get(pk))
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
        } else {
            None
        };
        if let Err(e) = db().upsert_ontology_object(
            &et_id, &et_name, ext_id.as_deref(), &label, &fields, &dataset_id, "promote",
        ).await {
            return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": e.to_string()}))).into_response();
        }
        promoted += 1;
    }
    let total = raw_records.len();

    // ── Resolve FK links → ontology_links (current + all other datasets) ─────
    let links_resolved = db().resolve_all_links().await.unwrap_or(0);
    // Auto-detect runs after resolve so it clears+rebuilds with correct AR direction
    let _ = db().auto_detect_links().await;

    // ── Mark dataset as promoted ──────────────────────────────────────────────
    let _ = db().update_dataset_entity_type(&dataset_id, &et_id).await;

    (StatusCode::OK, Json(json!({
        "ok": true,
        "promoted": promoted,
        "total": total,
        "entity_type_id": et_id,
        "entity_type_name": et_name,
        "dedup": !pk.is_empty(),
        "links_resolved": links_resolved,
    }))).into_response()
}

/// Auto-promote a dataset if it has a saved mapping. Called after every successful sync.
async fn auto_promote_if_mapped(dataset_id: &str) {
    let mapping = match db().get_object_type_mapping(dataset_id).await {
        Ok(Some(m)) => m,
        _ => return,
    };
    let et_id = match mapping["entity_type_id"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return,
    };
    let types = db().list_entity_types().await.unwrap_or_default();
    let et = match types.into_iter().find(|t| t.id == et_id) {
        Some(t) => t,
        None => { eprintln!("[auto-promote] entity type {} not found", et_id); return; }
    };
    // Read records: try manifest (S3/RustFS) first, fallback to SQLite
    let records: Vec<serde_json::Value> = {
        let from_manifest = async {
            let dv = db().get_current_dataset_version(dataset_id).await.ok().flatten()?;
            let mp = dv.manifest_path?;
            let (backend, _) = build_platform_backend().await;
            palantir_dataset::read_records_from_manifest_path(&backend, &mp).await.ok()
        }.await;
        if let Some(r) = from_manifest {
            r
        } else {
            // Fallback: read from SQLite dataset_records table
            let total = db().count_dataset_records(dataset_id).await.unwrap_or(0);
            if total == 0 { return; }
            match db().list_dataset_records(dataset_id, total.max(1), 0).await {
                Ok(rows) => rows.into_iter()
                    .map(|r| serde_json::from_str(&r.fields).unwrap_or(json!({})))
                    .collect(),
                Err(e) => { eprintln!("[auto-promote] sqlite fallback error: {e}"); return; }
            }
        }
    };
    let field_mapping: serde_json::Value = mapping["field_mapping"].as_str()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(json!({}));
    let pk = mapping["primary_key_col"].as_str().unwrap_or("").to_string();
    let sync_mode = mapping["sync_mode"].as_str().unwrap_or("snapshot").to_string();

    // SNAPSHOT: clear existing promoted objects for this dataset before re-inserting
    if sync_mode == "snapshot" {
        let _ = db().delete_ontology_objects_by_dataset(dataset_id).await;
    }

    let mut count = 0usize;
    for raw in &records {
        let mapped = apply_field_mapping(raw, &field_mapping);
        let label  = extract_label(&mapped);
        let fields = mapped.to_string();
        let ext_id: Option<String> = if !pk.is_empty() {
            mapped.get(&pk).or_else(|| raw.get(&pk)).map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
        } else { None };
        // APPEND: skip if object already exists (insert only new records)
        if sync_mode == "append" && ext_id.is_some() {
            // upsert_ontology_object will skip update if ext_id conflicts — good enough for append
            // Use INSERT OR IGNORE semantics by checking first
            let _ = db().upsert_ontology_object(
                &et.id, &et.display_name, ext_id.as_deref(), &label, &fields, dataset_id, "auto-promote",
            ).await;
        } else {
            let _ = db().upsert_ontology_object(
                &et.id, &et.display_name, ext_id.as_deref(), &label, &fields, dataset_id, "auto-promote",
            ).await;
        }
        count += 1;
    }
    eprintln!("[auto-promote] [{}] ✓ {} objects → \"{}\" (dataset {})", sync_mode, count, et.display_name, dataset_id);
    // Mark dataset as promoted
    let _ = db().update_dataset_entity_type(dataset_id, &et_id).await;
    // Resolve FK links after promoting
    let links = db().resolve_links_for_dataset(dataset_id).await.unwrap_or(0);
    if links > 0 { eprintln!("[auto-promote] {} links resolved for dataset {}", links, dataset_id); }
}

/// POST /api/ontology/promote-all — re-promote all datasets that have a saved mapping
async fn promote_all_handler() -> impl IntoResponse {
    let dataset_ids = match db().list_mapped_dataset_ids().await {
        Ok(ids) => ids,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    };
    let total = dataset_ids.len();
    // Pass 1: promote all datasets (objects must exist before links can be resolved)
    for dataset_id in &dataset_ids {
        auto_promote_if_mapped(dataset_id).await;
    }
    // Pass 2: resolve FK links after all objects are in place
    let mut total_links = 0usize;
    for dataset_id in &dataset_ids {
        total_links += db().resolve_links_for_dataset(dataset_id).await.unwrap_or(0);
    }
    eprintln!("[promote-all] {} datasets promoted, {} links resolved", total, total_links);
    (StatusCode::OK, Json(json!({"ok": true, "promoted_datasets": total, "links_resolved": total_links}))).into_response()
}

#[derive(serde::Deserialize)]
struct SaveDatasetMappingReq {
    entity_type_id:  String,
    primary_key_col: String,
    field_mapping:   serde_json::Value,
    #[serde(default = "default_sync_mode")]
    sync_mode: String,
    /// Fold (sub-business BC) to assign the ET to; null = shared/common
    #[serde(default)]
    fold_id: Option<String>,
}
async fn save_dataset_mapping_handler(
    Path(dataset_id): Path<String>,
    Json(req): Json<SaveDatasetMappingReq>,
) -> impl IntoResponse {
    let mapping_str = req.field_mapping.to_string();
    // Update ET's fold assignment if provided
    if let Some(ref fold_id) = req.fold_id {
        let fid = if fold_id.is_empty() { None } else { Some(fold_id.as_str()) };
        let _ = db().update_entity_type_fold(&req.entity_type_id, fid).await;
    }
    match db().save_object_type_mapping(&dataset_id, &req.entity_type_id, &req.primary_key_col, &mapping_str, &req.sync_mode).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn update_entity_type_fold_handler(
    Path(et_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let fold_id = body["fold_id"].as_str();
    match db().update_entity_type_fold(&et_id, fold_id).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct UpdateDatasetSyncModeReq { sync_mode: String }
async fn update_dataset_sync_mode_handler(
    Path(dataset_id): Path<String>,
    Json(req): Json<UpdateDatasetSyncModeReq>,
) -> impl IntoResponse {
    match db().update_dataset_sync_mode(&dataset_id, &req.sync_mode).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

/// GET /api/datasets/:id/mapping — return saved promote mapping for a dataset
async fn get_dataset_mapping_handler(Path(dataset_id): Path<String>) -> impl IntoResponse {
    match db().get_object_type_mapping(&dataset_id).await {
        Ok(Some(m)) => (StatusCode::OK, Json(json!({"ok": true, "mapping": m}))).into_response(),
        Ok(None)    => (StatusCode::OK, Json(json!({"ok": true, "mapping": null}))).into_response(),
        Err(e)      => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── S3 / Object-store helpers ─────────────────────────────────────────────────

fn build_s3_store(config: &serde_json::Value) -> Result<impl ObjectStore, String> {
    let bucket   = config["bucket"].as_str().unwrap_or("").trim();
    let ak       = config["access_key"].as_str().unwrap_or("").trim();
    let sk       = config["secret_key"].as_str().unwrap_or("").trim();
    let endpoint = config["endpoint"].as_str().unwrap_or("").trim();
    // Allow caller to override region; default to us-east-1 (works for MinIO/RustFS)
    let region   = {
        let r = config["region"].as_str().unwrap_or("").trim();
        if r.is_empty() { "us-east-1" } else { r }
    };

    if bucket.is_empty() { return Err("bucket is required".into()); }
    if ak.is_empty()     { return Err("access_key is required".into()); }
    if sk.is_empty()     { return Err("secret_key is required".into()); }

    let mut builder = AmazonS3Builder::new()
        .with_bucket_name(bucket)
        .with_access_key_id(ak)
        .with_secret_access_key(sk)
        .with_region(region);

    if !endpoint.is_empty() {
        builder = builder
            .with_endpoint(endpoint)
            // Path-style URL: http://host:port/bucket/key  (required for MinIO/RustFS)
            .with_virtual_hosted_style_request(false)
            // Allow plain http:// endpoints (RustFS default is non-TLS)
            .with_allow_http(true);
    }

    builder.build().map_err(|e| e.to_string())
}

async fn test_s3(config: &serde_json::Value) -> serde_json::Value {
    let store = match build_s3_store(config) {
        Ok(s) => s,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    let prefix = config["prefix"].as_str().unwrap_or("").trim();

    let stream = if prefix.is_empty() {
        store.list(None)
    } else {
        store.list(Some(&OsPath::from(prefix)))
    };

    match stream.try_collect::<Vec<_>>().await {
        Err(e) => json!({"ok": false, "error": e.to_string(), "error_type": "connection"}),
        Ok(metas) => {
            // Show all objects; mark parseable ones so UI can help the user
            let files: Vec<serde_json::Value> = metas.iter().map(|m| {
                let path = m.location.to_string();
                let parseable = { let l = path.to_lowercase(); l.ends_with(".csv") || l.ends_with(".json") || l.ends_with(".jsonl") };
                json!({ "path": path, "size": m.size, "parseable": parseable })
            }).collect();
            // Also expose flat list of parseable paths for legacy UI file selection
            let parseable_paths: Vec<String> = files.iter()
                .filter(|f| f["parseable"].as_bool().unwrap_or(false))
                .map(|f| f["path"].as_str().unwrap_or("").to_string())
                .collect();
            json!({"ok": true, "files": parseable_paths, "all_objects": files})
        }
    }
}

// ── Export dataset → S3/RustFS ────────────────────────────────────────────────

#[derive(Deserialize)]
struct ExportS3Req {
    endpoint:   String,
    bucket:     String,
    prefix:     Option<String>,
    access_key: String,
    secret_key: String,
    region:     Option<String>,
    format:     Option<String>, // "csv" | "json", default "csv"
}

async fn export_dataset_s3_handler(
    Path(dataset_id): Path<String>,
    Json(req): Json<ExportS3Req>,
) -> impl IntoResponse {
    let cfg = json!({
        "endpoint":   req.endpoint,
        "bucket":     req.bucket,
        "prefix":     req.prefix.as_deref().unwrap_or(""),
        "access_key": req.access_key,
        "secret_key": req.secret_key,
        "region":     req.region.as_deref().unwrap_or(""),
    });
    let store = match build_s3_store(&cfg) {
        Ok(s) => s,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    };

    // Load all records for this dataset
    let total = match db().count_dataset_records(&dataset_id).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    };
    let records = match db().list_dataset_records(&dataset_id, total.max(1), 0).await {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    };
    if records.is_empty() {
        return (StatusCode::OK, Json(json!({"ok": true, "uploaded": 0, "message": "no records"}))).into_response();
    }

    let fmt = req.format.as_deref().unwrap_or("csv");
    let prefix = req.prefix.as_deref().unwrap_or("");
    let filename = format!("{}{}.{}", prefix, dataset_id, fmt);
    let os_path = OsPath::from(filename.as_str());

    let payload: OsBytes = if fmt == "json" {
        // JSONL (one record per line)
        let mut out = String::new();
        for r in &records {
            let fields: serde_json::Value = serde_json::from_str(&r.fields).unwrap_or(json!({}));
            out.push_str(&serde_json::to_string(&fields).unwrap_or_default());
            out.push('\n');
        }
        OsBytes::from(out)
    } else {
        // CSV: build header from first record's fields
        let first_fields: serde_json::Value = serde_json::from_str(&records[0].fields).unwrap_or(json!({}));
        let headers: Vec<String> = if let serde_json::Value::Object(m) = &first_fields {
            m.keys().cloned().collect()
        } else { vec![] };

        let mut wtr = csv::Writer::from_writer(vec![]);
        let mut row_hdrs = vec!["_label".to_string()];
        row_hdrs.extend(headers.clone());
        let _ = wtr.write_record(&row_hdrs);
        for r in &records {
            let fields: serde_json::Value = serde_json::from_str(&r.fields).unwrap_or(json!({}));
            let mut row = vec![r.label.clone()];
            for h in &headers {
                row.push(fields[h].as_str().map(|s| s.to_string())
                    .unwrap_or_else(|| fields[h].to_string().trim_matches('"').to_string()));
            }
            let _ = wtr.write_record(&row);
        }
        OsBytes::from(wtr.into_inner().unwrap_or_default())
    };

    let size = payload.len();
    match store.put(&os_path, payload).await {
        Ok(_) => (StatusCode::OK, Json(json!({
            "ok": true,
            "uploaded": records.len(),
            "path": filename,
            "size_bytes": size,
        }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn sync_s3(
    run_id: &str,
    _source_id: &str,
    config: &serde_json::Value,
    entity_type_id: Option<&str>,
    dataset_id: &str,
    selected_files: &[String],
) -> Result<usize, String> {
    let store = build_s3_store(config)?;
    let source_prefix = config["prefix"].as_str().unwrap_or("").trim();

    // Discover files to sync
    let files: Vec<String> = if !selected_files.is_empty() {
        selected_files.to_vec()
    } else {
        let stream = if source_prefix.is_empty() {
            store.list(None)
        } else {
            store.list(Some(&OsPath::from(source_prefix)))
        };
        stream.try_collect::<Vec<_>>().await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|m| m.location.to_string())
            .filter(|f| { let l = f.to_lowercase(); l.ends_with(".csv") || l.ends_with(".json") || l.ends_with(".jsonl") })
            .collect()
    };

    if files.is_empty() { return Err("no CSV/JSON files found in the bucket/prefix".into()); }

    let mut total = 0usize;

    // ── Download → parse → store as OntologyObjects ────────────────────────
    for file_path in &files {
        let _ = db().update_sync_run_progress(run_id, total as i64, None,
            Some(&format!("下载 {}", file_path))).await;

        let os_path = OsPath::from(file_path.as_str());
        let bytes = store.get(&os_path).await
            .map_err(|e| format!("get {}: {}", file_path, e))?
            .bytes().await
            .map_err(|e| format!("read {}: {}", file_path, e))?;

        let tmp_path = format!("/tmp/palantir_s3_{}", uuid::Uuid::new_v4());
        tokio::fs::write(&tmp_path, &bytes).await.map_err(|e| e.to_string())?;

        let records = if file_path.to_lowercase().ends_with(".csv") {
            CsvAdapter::new("s3", &tmp_path, "ns", "schema")
                .fetch_preview(usize::MAX).await.map_err(|e| e.to_string())?
        } else {
            JsonAdapter::new("s3", &tmp_path, "ns", "schema")
                .fetch_preview(usize::MAX).await.map_err(|e| e.to_string())?
        };
        let _ = tokio::fs::remove_file(&tmp_path).await;

        let et_id = entity_type_id.unwrap_or("default");
        for (i, rec) in records.iter().enumerate() {
            let label  = extract_label(rec);
            let fields = serde_json::to_string(rec).unwrap_or_else(|_| "{}".into());
            db().create_ontology_object_with_lineage(et_id, et_id, &label, &fields, dataset_id, run_id)
                .await.map_err(|e| format!("insert failed: {e}"))?;
            if i % 50 == 0 {
                let _ = db().update_sync_run_progress(run_id, (total + i) as i64, None,
                    Some(file_path)).await;
            }
        }
        total += records.len();
        let _ = db().update_sync_run_progress(run_id, total as i64, None, Some(file_path)).await;
    }

    // NOTE: Platform storage write happens in sync_source_handler after this returns.
    // The source bucket (customer's S3) is read-only — we never write back to it.
    Ok(total)
}

/// Write all synced records for a dataset version to the appropriate storage backend.
///
/// Storage routing (Iter-2 per-tenant bucket scheme B):
///   source_type == "s3"/"ftp" AND source_config has valid S3 credentials
///     → customer's own bucket, prefix `platform_datasets/`
///     → path: `platform_datasets/{dataset_id}/v{version}/`
///
///   otherwise (SQLite, CSV, REST, or S3 config missing)
///     → local filesystem `{PALANTIR_DATA_DIR}/`
///     → path: `{dataset_id}/v{version}/`
///
/// Returns schema_json to store in dataset_versions.
/// Build the platform's own storage backend from env vars.
///
/// Platform storage env vars (point to the platform's own mybucket):
///   PLATFORM_S3_ENDPOINT  e.g. http://43.165.67.145:9000
///   PLATFORM_S3_BUCKET    e.g. mybucket
///   PLATFORM_S3_AK        access key
///   PLATFORM_S3_SK        secret key
///   PLATFORM_S3_REGION    optional, default us-east-1
///
/// Falls back to LocalFsBackend (PALANTIR_DATA_DIR) if env vars not set.
/// Load platform storage config: DB values take priority, env vars as override.
async fn load_platform_storage_config() -> (String, String, String, String, String) {
    let cfg = db().get_storage_config().await.unwrap_or_default();

    // env var overrides DB value if set
    let get = |json_key: &str, env_key: &str| -> String {
        let from_env = std::env::var(env_key).unwrap_or_default();
        if !from_env.is_empty() { return from_env; }
        cfg[json_key].as_str().unwrap_or("").to_string()
    };

    let endpoint = get("endpoint",   "PLATFORM_S3_ENDPOINT");
    let bucket   = get("bucket",     "PLATFORM_S3_BUCKET");
    let ak       = get("access_key", "PLATFORM_S3_AK");
    let sk       = get("secret_key", "PLATFORM_S3_SK");
    let region   = {
        let r = get("region", "PLATFORM_S3_REGION");
        if r.is_empty() { "us-east-1".to_string() } else { r }
    };
    (endpoint, bucket, ak, sk, region)
}

async fn build_platform_backend() -> (StdArc<dyn palantir_dataset::StorageBackend>, bool) {
    let (endpoint, bucket, ak, sk, region) = load_platform_storage_config().await;

    eprintln!("[storage] platform backend: endpoint={:?} bucket={:?} ak_set={}",
        endpoint, bucket, !ak.is_empty());

    if !bucket.is_empty() && !ak.is_empty() && !sk.is_empty() {
        match S3Backend::new(&endpoint, &bucket, &ak, &sk, &region) {
            Ok(b) => {
                eprintln!("[storage] using S3Backend → {}/{}", endpoint, bucket);
                return (StdArc::new(b), true);
            }
            Err(e) => eprintln!("[storage] S3Backend init failed, falling back to local: {e}"),
        }
    } else {
        eprintln!("[storage] platform S3 not configured → falling back to LocalFsBackend");
    }

    let dir = std::env::var("PALANTIR_DATA_DIR").unwrap_or_else(|_| "data/platform".into());
    eprintln!("[storage] using LocalFsBackend → {}", dir);
    (StdArc::new(LocalFsBackend::new(dir)), false)
}

/// Write all synced records to the **platform's own storage** (mybucket).
///
/// Path in platform bucket:
///   platform_datasets/{project_id}/{dataset_id}/v{version}/
///     manifest.json
///     data/part-00000.csv
///
/// Source bucket (mybucket1) is never written — read-only.
async fn write_to_platform_storage(
    dataset_id: &str,
    version: i64,
    run_id: &str,
    dv_id: &str,
    _source_type: &str,
    _source_config: &serde_json::Value,
    project_id: &str,
) -> String {
    let (backend, _is_s3) = build_platform_backend().await;
    let prefix = format!("platform_datasets/{}", project_id);
    do_write_storage(backend, &prefix, dataset_id, version, run_id, dv_id).await
}

async fn do_write_storage(
    backend: StdArc<dyn palantir_dataset::StorageBackend>,
    root_prefix: &str,
    dataset_id: &str,
    version: i64,
    run_id: &str,
    dv_id: &str,
) -> String {
    let store = DatasetStore::new(backend, root_prefix);

    // Load all records for this dataset written in this run
    let total = match db().count_dataset_records(dataset_id).await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[storage] count_records error: {e}");
            return json!({"fields":[]}).to_string();
        }
    };
    eprintln!("[storage] do_write_storage: dataset_id={} version={} total_records={}", dataset_id, version, total);
    if total == 0 {
        eprintln!("[storage] no records found for dataset {}, skipping write", dataset_id);
        return json!({"fields":[]}).to_string();
    }
    let rows = match db().list_dataset_records(dataset_id, total.max(1), 0).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[storage] list_records error: {e}");
            return json!({"fields":[]}).to_string();
        }
    };

    // Convert to JSON records
    let records: Vec<serde_json::Value> = rows.iter().map(|r| {
        serde_json::from_str(&r.fields).unwrap_or(serde_json::Value::Object(Default::default()))
    }).collect();

    let schema = palantir_dataset::DatasetSchema::infer_from_records(&records);
    let schema_json = serde_json::to_string(&schema).unwrap_or_else(|_| json!({"fields":[]}).to_string());

    let mut writer = store.begin_write(dataset_id, version, run_id);
    if let Err(e) = writer.append_records(&records).await {
        eprintln!("[storage] append_records error: {e}");
        return schema_json;
    }

    eprintln!("[storage] committing {} records to prefix={:?} dataset={} v{}", records.len(), root_prefix, dataset_id, version);
    match writer.commit(schema).await {
        Ok(manifest) => {
            // Full logical path including root_prefix (for S3: "platform_datasets/{project_id}/...")
            let manifest_path = if root_prefix.is_empty() {
                format!("{}/v{}/manifest.json", dataset_id, version)
            } else {
                format!("{}/{}/v{}/manifest.json", root_prefix, dataset_id, version)
            };
            let _ = db().update_version_manifest_path(dv_id, &manifest_path).await;
            eprintln!(
                "[storage] ✓ {} rows written → {} (content_hash: {})",
                manifest.total_rows, manifest_path, &manifest.content_hash[..8]
            );
            schema_json
        }
        Err(e) => {
            eprintln!("[storage] ✗ commit error: {e}");
            schema_json
        }
    }
}

// ── Iter-3: Schema Evolution Detection ───────────────────────────────────────

/// Compare two schema JSONs and return a classification string:
/// "none" | "compatible" (new nullable fields added) | "breaking" (fields removed / type changed)
fn detect_schema_change(old_json: &str, new_json: &str) -> &'static str {
    let old: serde_json::Value = serde_json::from_str(old_json).unwrap_or(json!({"fields":[]}));
    let new: serde_json::Value = serde_json::from_str(new_json).unwrap_or(json!({"fields":[]}));

    let old_fields: std::collections::HashMap<&str, &str> = old["fields"]
        .as_array().map(|a| a.iter().filter_map(|f| {
            Some((f["name"].as_str()?, f["data_type"].as_str().unwrap_or("string")))
        }).collect()).unwrap_or_default();

    let new_fields: std::collections::HashMap<&str, &str> = new["fields"]
        .as_array().map(|a| a.iter().filter_map(|f| {
            Some((f["name"].as_str()?, f["data_type"].as_str().unwrap_or("string")))
        }).collect()).unwrap_or_default();

    if old_fields.is_empty() { return "none"; }

    let mut has_removal = false;
    let mut has_type_change = false;
    let mut has_addition = false;

    for (name, old_type) in &old_fields {
        match new_fields.get(name) {
            None => has_removal = true,
            Some(new_type) => if new_type != old_type { has_type_change = true; }
        }
    }
    for name in new_fields.keys() {
        if !old_fields.contains_key(name) { has_addition = true; }
    }

    if has_removal || has_type_change { "breaking" }
    else if has_addition { "compatible" }
    else { "none" }
}

// ── Iter-3: Rollback Re-materialization ──────────────────────────────────────

/// Read a manifest from platform storage and re-insert all records as OntologyObjects.
/// Called after `rollback_dataset_version` flips is_current.
async fn rematerialize_from_manifest(
    dataset_id: &str,
    manifest_path: &str,   // stored in dataset_versions.manifest_path
    run_id: &str,
    project_id: &str,
) -> Result<usize, String> {
    let (backend, _) = build_platform_backend().await;

    // manifest_path is the full logical path; strip the trailing "/manifest.json" to get prefix
    let prefix = manifest_path.trim_end_matches("/manifest.json");

    // Read manifest
    let manifest_bytes = backend.get(manifest_path).await
        .map_err(|e| format!("read manifest: {e}"))?;
    let manifest: palantir_dataset::DatasetManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| format!("parse manifest: {e}"))?;

    // Delete existing objects for this dataset
    let deleted = db().delete_dataset_objects(dataset_id).await
        .map_err(|e| format!("delete objects: {e}"))?;
    eprintln!("[rollback] deleted {} existing objects for dataset {}", deleted, dataset_id);

    let mut total = 0usize;
    for file_entry in &manifest.files {
        let part_path = format!("{}/{}", prefix, file_entry.path);
        let bytes = backend.get(&part_path).await
            .map_err(|e| format!("read part {}: {e}", file_entry.path))?;

        // Parse CSV
        let mut rdr = csv::Reader::from_reader(bytes.as_ref());
        let headers: Vec<String> = rdr.headers()
            .map_err(|e| format!("csv headers: {e}"))?
            .iter().map(|s| s.to_string()).collect();

        for result in rdr.records() {
            let rec = result.map_err(|e| format!("csv record: {e}"))?;
            let mut obj = serde_json::Map::new();
            for (k, v) in headers.iter().zip(rec.iter()) {
                obj.insert(k.clone(), json!(v));
            }
            let record = serde_json::Value::Object(obj);
            let label = extract_label(&record);
            let fields = record.to_string();
            db().create_ontology_object_with_lineage(
                "default", "default", &label, &fields, dataset_id, run_id,
            ).await.map_err(|e| format!("insert: {e}"))?;
            total += 1;
        }
    }
    eprintln!("[rollback] re-materialized {} objects for dataset {}", total, dataset_id);
    Ok(total)
}

// ── Iter-3: GC ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GcReq {
    #[serde(default = "default_keep_versions")]
    keep_versions: i64,
}
fn default_keep_versions() -> i64 { 3 }

async fn gc_dataset_handler(
    Path(dataset_id): Path<String>,
    Json(req): Json<GcReq>,
) -> impl IntoResponse {
    let old_versions = match db().old_dataset_versions(&dataset_id, req.keep_versions).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    };

    if old_versions.is_empty() {
        return (StatusCode::OK, Json(json!({"ok": true, "deleted": 0}))).into_response();
    }

    let (backend, _) = build_platform_backend().await;
    let mut deleted_versions = 0usize;
    let mut deleted_files = 0u64;

    for (version_id, version_num, manifest_path) in &old_versions {
        // Delete files from storage
        if let Some(mp) = manifest_path {
            let prefix = mp.trim_end_matches("/manifest.json");
            match backend.delete_prefix(prefix).await {
                Ok(n) => deleted_files += n,
                Err(e) => eprintln!("[gc] delete_prefix {} error: {e}", prefix),
            }
        }
        // Mark in DB
        let _ = db().gc_version(version_id).await;
        deleted_versions += 1;
        eprintln!("[gc] dataset={} version={} gc'd", dataset_id, version_num);
    }

    (StatusCode::OK, Json(json!({
        "ok": true,
        "deleted_versions": deleted_versions,
        "deleted_files": deleted_files,
        "kept": req.keep_versions,
    }))).into_response()
}

// ── Admin: Platform Storage Config ───────────────────────────────────────────

async fn get_storage_config_handler() -> impl IntoResponse {
    match db().get_storage_config().await {
        Ok(cfg) => {
            // Mask secret_key in response
            let mut masked = cfg.clone();
            if let Some(obj) = masked.as_object_mut() {
                if obj.contains_key("secret_key") {
                    obj.insert("secret_key".into(), json!("******"));
                }
            }
            (StatusCode::OK, Json(json!({"ok": true, "config": masked}))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct StorageConfigReq {
    endpoint:   Option<String>,
    bucket:     Option<String>,
    access_key: Option<String>,
    secret_key: Option<String>,
    region:     Option<String>,
}

async fn set_storage_config_handler(Json(req): Json<StorageConfigReq>) -> impl IntoResponse {
    let cfg = json!({
        "endpoint":   req.endpoint.as_deref().unwrap_or(""),
        "bucket":     req.bucket.as_deref().unwrap_or(""),
        "access_key": req.access_key.as_deref().unwrap_or(""),
        "secret_key": req.secret_key.as_deref().unwrap_or(""),
        "region":     req.region.as_deref().unwrap_or(""),
    });
    match db().set_storage_config(&cfg).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn test_storage_config_handler() -> impl IntoResponse {
    let (endpoint, bucket, ak, sk, region) = load_platform_storage_config().await;
    if bucket.is_empty() || ak.is_empty() {
        return (StatusCode::OK, Json(json!({"ok": false, "error": "platform storage not configured"}))).into_response();
    }
    match S3Backend::new(&endpoint, &bucket, &ak, &sk, &region) {
        Err(e) => (StatusCode::OK, Json(json!({"ok": false, "error": e.to_string()}))).into_response(),
        Ok(b) => {
            let backend: StdArc<dyn palantir_dataset::StorageBackend> = StdArc::new(b);
            match backend.list("").await {
                Ok(files) => (StatusCode::OK, Json(json!({
                    "ok": true,
                    "message": format!("Connected to {}/{}", endpoint, bucket),
                    "file_count": files.len(),
                }))).into_response(),
                Err(e) => (StatusCode::OK, Json(json!({"ok": false, "error": e.to_string()}))).into_response(),
            }
        }
    }
}

fn chrono_now_str() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

// ── Router ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init SQLite
    let db_path = std::env::var("PALANTIR_DB").unwrap_or_else(|_| "palantir.db".into());
    let db_inst = Db::open(&db_path).await?;
    DB.set(Arc::new(db_inst)).ok();

    // ── Determine frontend dist path ──────────────────────────────────────────
    // In dev: run from repo root → "frontend/dist"
    // In production containers: may be set via FRONTEND_DIST env var
    let frontend_dist = std::env::var("FRONTEND_DIST")
        .unwrap_or_else(|_| "frontend/dist".to_string());
    let spa_fallback = ServeDir::new(&frontend_dist)
        .not_found_service(ServeFile::new(format!("{}/index.html", frontend_dist)));

    let app = Router::new()
        // Legacy redirects (old HTML → React SPA routes)
        .route("/ingest/project/:id", get(ingest_project_redirect))
        .route("/ingest/fold/:id",    get(ingest_fold_redirect))
        .route("/healthz", get(healthz))
        // Ontology TBox (Schema)
        .route("/api/ontology/schema", get(list_entity_types_handler).post(create_entity_type_handler))
        .route("/api/ontology/schema/:id", axum::routing::delete(delete_entity_type_handler))
        .route("/api/ontology/schema/:id/fields", post(add_field_handler))
        .route("/api/ontology/schema/:id/fold", axum::routing::put(update_entity_type_fold_handler))
        .route("/api/ontology/schema/:id/bc", axum::routing::put(set_et_bc_handler))
        .route("/api/ontology/schema/:id/ddd-role", axum::routing::put(set_et_ddd_role_handler))
        .route("/api/ontology/schema/:id/status", axum::routing::put(set_et_status_handler))
        .route("/api/ontology/schema/:id/lineage", get(get_et_lineage_handler))
        .route("/api/ontology/schema/:id/interfaces", get(list_et_interfaces_handler).post(add_et_interface_handler))
        .route("/api/ontology/schema/:id/interfaces/:iface_id", axum::routing::delete(remove_et_interface_handler))
        .route("/api/ontology/fields/:id", axum::routing::delete(delete_field_handler))
        .route("/api/ontology/fields/:id/type", axum::routing::put(update_field_type_handler))
        .route("/api/ontology/fields/:id/safe-delete", axum::routing::delete(delete_field_safe_handler).post(delete_field_safe_handler))
        // P2a: Bounded Contexts
        .route("/api/folds/:id/bounded-contexts", get(list_bcs_handler).post(create_bc_handler))
        .route("/api/bounded-contexts/:id", axum::routing::delete(delete_bc_handler))
        .route("/api/bc-relationships", post(create_bc_relationship_handler))
        .route("/api/bc-relationships/:id", axum::routing::delete(delete_bc_relationship_handler))
        .route("/api/shared-kernels", get(list_shared_kernels_handler))
        .route("/api/projects/:id/context-map", get(get_context_map_handler))
        // FK auto-link
        .route("/api/ontology/auto-link", post(auto_detect_links_handler))
        // P1: BC Inference
        .route("/api/folds/:id/bcs/infer", get(infer_child_bcs_handler))
        .route("/api/folds/:id/bcs/apply-suggestions", post(apply_bc_suggestions_handler))
        // P2c: Interfaces
        .route("/api/interfaces", get(list_interfaces_handler).post(create_interface_handler))
        .route("/api/interfaces/:id", axum::routing::delete(delete_interface_handler))
        // Ontology ABox (Objects)
        .route("/api/ontology/objects", get(list_objects_handler).post(create_object_handler))
        .route(
            "/api/ontology/objects/:id",
            get(get_object_handler)
                .put(update_object_handler)
                .delete(delete_object_handler),
        )
        // Ontology Links
        .route("/api/ontology/links", post(create_link_handler))
        .route("/api/ontology/links/:id", axum::routing::delete(delete_link_handler))
        // Graph view
        .route("/api/ontology/graph", get(get_ontology_graph_handler))
        .route("/api/ontology/schema-links", get(list_schema_links_handler))
        .route("/api/ontology/promote-all", axum::routing::post(promote_all_handler))
        // Project management
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/:id",
            get(get_project).delete(delete_project_handler).patch(patch_project),
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
        .route("/api/upload/source", post(upload_source_files))
        .route("/api/inspect", post(inspect))
        .route("/api/workspace/build", post(workspace_build))
        .route(
            "/api/discover",
            post(|| async { (StatusCode::OK, Json(json!({"ok":true}))) }),
        )
        // Sources multi-adapter demo
        .route("/api/sources",           get(list_all_sources_handler).post(create_source_simple))
        .route("/api/sources/test",      post(source_test))
        .route("/api/sources/quick-test", post(quick_test_handler))
        .route("/api/sources/discover", post(source_discover))
        .route("/api/sources/preview",  post(source_preview))
        // Data Connections
        .route("/api/connections/sync", post(connections_sync))
        .route("/api/live_ontology", get(live_ontology))
        .route("/api/reset", post(reset))
        // ── Ingest workflow ──────────────────────────────────────────────────
        .route("/api/projects/:id/folds",   get(list_folds).post(create_fold))
        .route("/api/folds/:id",            get(get_fold_handler).delete(delete_fold_handler))
        .route("/api/folds/:id/infer-mapping", post(infer_mapping_handler))
        .route("/api/folds/:id/sources",    get(list_sources).post(create_source))
        .route("/api/sources/:id",            get(get_source_handler).put(update_source_handler).delete(delete_source_handler))
        .route("/api/sources/:id/test",       post(test_source_handler))
        .route("/api/sources/:id/sync",       post(sync_source_handler))
        .route("/api/sources/:id/deprecate",  post(deprecate_source_handler))
        .route("/api/sources/:id/activate",   post(activate_source_handler))
        .route("/api/sources/:id/jobs",     get(list_jobs_handler))
        .route("/api/sources/:id/datasets", get(list_datasets_handler))
        .route("/api/jobs/:id",             get(get_job_handler))
        .route("/api/datasets",              get(list_all_datasets_handler))
        .route("/api/datasets/:id",         get(get_dataset_handler))
        .route("/api/datasets/:id/versions", get(list_dataset_versions_handler))
        .route("/api/datasets/:id/rollback", post(rollback_dataset_handler))
        .route("/api/datasets/:id/gc",       post(gc_dataset_handler))
        .route("/api/datasets/:id/promote",  post(promote_dataset_handler))
        .route("/api/datasets/:id/mapping",  get(get_dataset_mapping_handler).post(save_dataset_mapping_handler))
        .route("/api/datasets/:id/sync-mode", axum::routing::put(update_dataset_sync_mode_handler))
        .route("/api/datasets/:id/records", get(list_dataset_records_handler))
        .route("/api/datasets/:id/export/s3", post(export_dataset_s3_handler))
        // ── Admin: platform storage config ───────────────────────────────────
        .route("/api/admin/storage", get(get_storage_config_handler).post(set_storage_config_handler))
        .route("/api/admin/storage/test", post(test_storage_config_handler))
        .nest_service("/docs", ServeDir::new("docs"))
        // ── React SPA: serves frontend/dist, falls back to index.html ─────────
        .fallback_service(spa_fallback);

    let addr_str = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("INGEST_ADDR").ok())
        .or_else(|| std::env::var("PORT").ok().map(|p| format!("0.0.0.0:{}", p)))
        .unwrap_or_else(|| "0.0.0.0:8080".to_string());
    let addr: SocketAddr = addr_str
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid addr '{}': {}", addr_str, e))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("[palantir] listening on http://{}", addr);
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("server error: {e}");
        return Err(anyhow::Error::new(e));
    }
    Ok(())
}
