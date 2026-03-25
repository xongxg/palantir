# palantir-ontology Crate — 架构设计

> **文档版本**：v0.1
> **状态**：草稿·待讨论
> **日期**：2026-03-25
> **受众**：后端团队
> **关联文档**：
> - `ontology_manager_design.md` — 功能设计（P0~P2d，Schema 保护 / ET 生命周期 / Shared Kernel / System Interface）
> - `palantir_db_abstraction_design.md` — 元数据存储抽象层设计

---

## 一、问题：Ontology 逻辑在哪里？

### 1.1 当前现状

```
palantir-ingest-api/src/main.rs   (~3000 行)
  ├── /api/ontology/schema/*     ← TBox 管理（Entity Type / Field CRUD）
  ├── /api/ontology/objects/*    ← ABox 管理（Object / Link CRUD）
  ├── /api/ontology/graph        ← 关系图谱查询
  └── /api/datasets/*/promote    ← Dataset → Ontology 提升

palantir-sqlite/src/db.rs         (~2300 行)
  └── create_entity_type / add_entity_field / upsert_ontology_object 等
      ← 所有 SQL 实现混在同一个 Db struct 里
```

**问题**：
- Ontology **业务逻辑**（验证、状态机、血缘推导、breaking change 检测）全部嵌在 HTTP handler 里
- 无法单元测试（handler 依赖 HTTP context）
- 无法复用（Agent 层、未来的 ontology-svc 要重写一遍）
- 与存储层直接耦合（handler → Db，跳过抽象层）

### 1.2 目标

抽取一个 `palantir-ontology` crate：

```
palantir-ingest-api   →   palantir-ontology   →   palantir-meta-store（trait）
  (薄路由层)                 (业务逻辑层)               (存储层)
```

`palantir-ingest-api` 的 handler 变成：
```rust
// 现在（业务逻辑混在 handler）
async fn create_entity_type(...) {
    // 验证、SQL、状态机... 几十行
}

// 目标（handler 只做路由）
async fn create_entity_type(...) {
    let result = ontology.create_entity_type(req).await?;
    Json(result)
}
```

---

## 二、Crate 定位

### 2.1 在 Workspace 中的位置

```
crates/
  store/
    palantir-meta-store/      ← 存储 trait（Port）
    palantir-sqlite/          ← SQLite 实现（Adapter）

  ontology/                   ← 新建分组
    palantir-ontology/        ← Ontology Manager（业务逻辑层）
    palantir-ontology-api/    ← （未来）独立部署的服务壳

  palantir-source-adapter/
  palantir-dataset/
  palantir-ingest-api/        ← 薄路由层（依赖 palantir-ontology）
  palantir-agent/             ← 也可依赖 palantir-ontology
```

### 2.2 依赖关系

```
palantir-ingest-api
  └── palantir-ontology
        └── palantir-meta-store（Arc<dyn MetadataStore>）

palantir-agent
  └── palantir-ontology（查询 ET / Object，不写入）
```

`palantir-ontology` **不直接依赖** `palantir-sqlite`——通过 trait 解耦，具体实现由上层注入。

---

## 三、核心 Rust 设计

### 3.1 OntologyManager 结构体

```rust
// crates/ontology/palantir-ontology/src/lib.rs

use std::sync::Arc;
use palantir_meta_store::MetadataStore;

pub struct OntologyManager {
    store: Arc<dyn MetadataStore>,
}

impl OntologyManager {
    pub fn new(store: Arc<dyn MetadataStore>) -> Self {
        Self { store }
    }
}
```

所有业务方法挂在 `OntologyManager` 上，通过 `self.store` 访问存储层。

### 3.2 模块划分

```
crates/ontology/palantir-ontology/src/
├── lib.rs              ← pub use + OntologyManager struct
│
├── tbox/               ← TBox 管理（Schema 层）
│   ├── mod.rs
│   ├── entity_type.rs  ← ET CRUD + 状态机 + breaking change 检测
│   ├── entity_field.rs ← 字段 CRUD + 类型校验
│   └── link_type.rs    ← Link Type 定义（未来）
│
├── abox/               ← ABox 管理（实例层）
│   ├── mod.rs
│   ├── object.rs       ← Object CRUD + Interface 字段校验
│   └── link.rs         ← Object 间 Link CRUD
│
├── lineage/            ← 数据血缘
│   └── mod.rs          ← dataset → ET 血缘查询
│
├── shared_kernel/      ← Shared Kernel（P2a）
│   └── mod.rs          ← SK fold 管理 + 协商警告 + Context Map
│
├── interface/          ← System Interface（P2c）
│   └── mod.rs          ← Interface 定义 + ET 实现 + 字段预填
│
└── promote/            ← Dataset Promote 编排
    └── mod.rs          ← 校验 ET 状态 + 写入 Object + 触发血缘更新
```

### 3.3 TBox 方法示意

```rust
// crates/ontology/palantir-ontology/src/tbox/entity_type.rs

impl OntologyManager {

    /// 创建 Entity Type。新建默认状态为 active（可配置为 draft）。
    pub async fn create_entity_type(
        &self,
        req: CreateEntityTypeReq,
    ) -> Result<EntityTypeRow> {
        // 1. 名称唯一性校验
        // 2. fold_id 合法性校验
        // 3. 写入存储
        self.store.create_entity_type(
            &req.name, &req.display_name, &req.color,
            &req.icon, req.fold_id.as_deref(),
            &req.ddd_role, req.namespace.as_deref(),
        ).await
    }

    /// 修改字段类型 — 含 breaking change 检测。
    /// 若为 breaking change，返回 BreakingChangeInfo 要求调用方提供迁移策略。
    pub async fn update_field_type(
        &self,
        field_id: &str,
        new_type: &str,
        strategy: Option<MigrationStrategy>,
    ) -> Result<UpdateFieldResult> {
        let field = self.store.get_entity_field(field_id).await?
            .ok_or_else(|| anyhow::anyhow!("field not found"))?;

        if field.data_type != new_type {
            let affected = self.store.count_ontology_objects_for_et(
                &field.entity_type_id
            ).await?;

            if affected > 0 && strategy.is_none() {
                // 返回 breaking change 信息，要求策略
                return Ok(UpdateFieldResult::RequiresStrategy {
                    affected_count: affected,
                    change_type: ChangeType::TypeChange {
                        old_type: field.data_type.clone(),
                        new_type: new_type.to_string(),
                    },
                    available_strategies: vec![
                        MigrationStrategy::Drop,
                        MigrationStrategy::Cast,
                    ],
                });
            }

            // 执行迁移
            if let Some(strat) = strategy {
                self.apply_field_migration(&field, new_type, strat).await?;
            }
        }

        self.store.update_entity_field_type(field_id, new_type).await?;
        Ok(UpdateFieldResult::Ok)
    }

    /// ET 状态变更（draft → active → deprecated）。
    pub async fn change_et_status(
        &self,
        et_id: &str,
        new_status: EtStatus,
    ) -> Result<EtStatusChangeResult> {
        let et = self.store.get_entity_type(et_id).await?
            .ok_or_else(|| anyhow::anyhow!("entity type not found"))?;

        // 状态机校验
        validate_status_transition(&et.status, &new_status)?;

        // deprecated 时，统计受影响的 datasets
        let affected_datasets = if new_status == EtStatus::Deprecated {
            self.store.list_datasets_mapped_to_et(et_id).await?
        } else {
            vec![]
        };

        self.store.update_entity_type_status(et_id, new_status.as_str()).await?;

        Ok(EtStatusChangeResult {
            affected_datasets,
        })
    }
}
```

### 3.4 Promote 编排方法

```rust
// crates/ontology/palantir-ontology/src/promote/mod.rs

impl OntologyManager {

    /// Dataset Promote：将 Dataset 最新版本的所有行写入 Ontology ABox。
    ///
    /// 含前置校验：ET 状态、Interface 字段存在性、主键唯一性。
    pub async fn promote_dataset(
        &self,
        dataset_id: &str,
        mapping: &ObjectTypeMapping,
        records: Vec<serde_json::Value>,
    ) -> Result<PromoteResult> {
        // 1. 校验 ET 状态（draft/deprecated 拒绝）
        let et = self.store.get_entity_type(&mapping.entity_type_id).await?
            .ok_or_else(|| anyhow::anyhow!("entity type not found"))?;

        match et.status.as_str() {
            "draft"      => anyhow::bail!("entity type is in draft status"),
            "deprecated" => anyhow::bail!("entity type is deprecated"),
            _            => {}
        }

        // 2. Interface 字段存在性校验（警告，不阻断）
        let warnings = self.check_interface_fields(&mapping.entity_type_id, &records).await?;

        // 3. 批量写入 Ontology Objects
        let mut written = 0usize;
        for record in &records {
            self.store.upsert_ontology_object(
                &mapping.entity_type_id,
                &et.name,
                extract_label(record, &mapping.pk_col),
                &record.to_string(),
                dataset_id,
                extract_pk(record, &mapping.pk_col),
                &mapping.sync_mode,
            ).await?;
            written += 1;
        }

        Ok(PromoteResult { written, warnings })
    }
}
```

### 3.5 关键数据类型

```rust
// crates/ontology/palantir-ontology/src/types.rs

#[derive(Debug, Clone, PartialEq)]
pub enum EtStatus {
    Draft,
    Active,
    Deprecated,
}

#[derive(Debug, Clone)]
pub enum MigrationStrategy {
    Drop,         // 丢弃已有对象的该字段值
    Cast,         // 类型兼容转换，失败置 null
    Rename(String), // 将旧字段值迁移到新字段名
}

#[derive(Debug)]
pub enum UpdateFieldResult {
    Ok,
    RequiresStrategy {
        affected_count: i64,
        change_type: ChangeType,
        available_strategies: Vec<MigrationStrategy>,
    },
}

#[derive(Debug)]
pub enum ChangeType {
    TypeChange { old_type: String, new_type: String },
    Delete,
    Rename { old_name: String },
    PkChange,
}

#[derive(Debug)]
pub struct PromoteResult {
    pub written: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct EtStatusChangeResult {
    pub affected_datasets: Vec<DatasetRow>,
}
```

---

## 四、palantir-ingest-api 如何变薄

### 4.1 handler 变化对比

```rust
// ── 现在（业务逻辑在 handler）──────────────────────────────────

async fn update_entity_field(
    State(db): State<Arc<Db>>,
    Path(field_id): Path<String>,
    Json(payload): Json<UpdateFieldReq>,
) -> impl IntoResponse {
    // 几十行：校验、SQL、迁移、错误处理...
    let field = db.get_entity_field(&field_id).await?;
    let affected = db.count_objects_for_et(&field.entity_type_id).await?;
    if affected > 0 && payload.strategy.is_none() {
        return Json(json!({ "breaking": true, "affected_count": affected }));
    }
    // ... 执行迁移 ...
    db.update_entity_field_type(&field_id, &payload.data_type).await?;
    Json(json!({ "ok": true }))
}

// ── 目标（handler 只做路由）───────────────────────────────────

async fn update_entity_field(
    State(om): State<Arc<OntologyManager>>,
    Path(field_id): Path<String>,
    Json(payload): Json<UpdateFieldReq>,
) -> impl IntoResponse {
    let result = om.update_field_type(
        &field_id,
        &payload.data_type,
        payload.strategy,
    ).await?;
    Json(result)
}
```

### 4.2 State 注入变化

```rust
// main.rs 启动时

// 现在
let db = Arc::new(Db::open(&db_path).await?);
let app = Router::new().with_state(db);

// 目标
let store: Arc<dyn MetadataStore> = Arc::new(
    palantir_sqlite::Db::open(&db_path).await?
);
let ontology = Arc::new(OntologyManager::new(Arc::clone(&store)));
let app = Router::new().with_state(ontology);
```

---

## 五、Cargo.toml

```toml
# crates/ontology/palantir-ontology/Cargo.toml
[package]
name    = "palantir-ontology"
version = "0.1.0"
edition = "2024"

[dependencies]
palantir-meta-store = { path = "../../store/palantir-meta-store" }
anyhow      = "1"
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
async-trait = "0.1"
uuid        = { version = "1", features = ["v4"] }
```

**注意**：
- 不依赖 `palantir-sqlite`（通过 trait 解耦）
- 不依赖 `axum`（纯业务逻辑，无 HTTP 概念）
- 不依赖 `sqlx`（不直接写 SQL）

---

## 六、迁移路径

### 阶段一：建骨架，0 功能迁移（可与现有代码并行）

```
1. 创建 crates/ontology/palantir-ontology/
2. 定义 OntologyManager struct + 空方法签名
3. 加入 workspace，cargo build 通过
4. palantir-ingest-api 加入依赖（但暂时不调用）
```

### 阶段二：逐方法迁移（按 P0/P1 顺序）

```
1. 迁移 create_entity_type → OntologyManager::create_entity_type
2. handler 改为调用 OntologyManager
3. 单元测试用 mock MetadataStore 验证业务逻辑
4. 重复，直到所有 handler 变薄
```

### 阶段三：新功能在 OntologyManager 中实现（P0~P2）

```
breaking change 检测、ET 状态机、数据血缘、Shared Kernel、System Interface
全部在 palantir-ontology 中实现，handler 只做参数解析和结果序列化
```

---

## 七、待讨论问题

| 问题 | 选项 A | 选项 B | 当前倾向 |
|------|--------|--------|---------|
| `OntologyManager` 是 struct 还是 trait？ | struct（简单，直接持有 store） | trait（可 mock，但多一层） | **struct**，store 本身是 trait 已够 mock |
| Promote 逻辑放在 `palantir-ontology` 还是 `palantir-dataset`？ | ontology（因为写的是 Ontology Objects） | dataset（因为读的是 Dataset） | **ontology**，最终写入目标决定归属 |
| `palantir-ingest-api` 是否应直接依赖 `palantir-sqlite` 还是只依赖 trait？ | 直接依赖 palantir-sqlite（现在的做法） | 只依赖 palantir-meta-store + env var 工厂 | **短期 A，中期 B** |
| 阶段一骨架是否现在开始建？ | 现在建 | 等功能设计确认后建 | 待用户决策 |
