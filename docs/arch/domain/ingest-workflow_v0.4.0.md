# 数据接入工作流设计 v0.4.0

> 版本：v0.4.0
> 日期：2026-03-20
> 状态：迭代规划阶段（Iter-1 实现中）
> 前置文档：[ingest-workflow v0.3.0](ingest-workflow_v0.3.0.md) · [ADR-34 Dataset 存储后端与版本化](../adr/ADR-34-dataset-storage-versioning.md)

---

## 变更记录

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初稿：US、领域模型、交互图、状态机 |
| v0.2.0 | 2026-03-20 | 引入薄 Dataset 层；Palantir 对比；规范化 US；三阶段演进；API 契约 |
| v0.3.0 | 2026-03-20 | StorageBackend 可插拔体系；六个设计盲点；Epic E5（Dataset 版本管理）；细化全部 US |
| **v0.4.0** | **2026-03-20** | **Palantir 四层精确对位分析；七次迭代路线图（Iter-1 ~ Iter-7）；Phase 实现状态更新；palantir-storage crate 设计** |

---

## 一、背景与目标

### 1.1 Palantir Foundry 四层模型与本平台的精确对位

Palantir Foundry 数据接入体系完整四层：

```
                         外部数据源（External Sources）
                                    │
                                    ▼
┌───────────────────────────────────────────────────────────────┐
│  Layer 1: Raw / Integration（原始层）                          │
│  不可变快照，版本化存储（Compass FS）                           │
│  本平台对应：DatasetVersion + StorageBackend                    │
├───────────────────────────────────────────────────────────────┤
│  Layer 2: Transform / Pipeline（加工层）                       │
│  Code Repos / Contour / PySpark DAG → 衍生 Dataset             │
│  本平台对应：Phase 3 Transform Pipeline（待建）                  │
├───────────────────────────────────────────────────────────────┤
│  Layer 3: Semantic / Logical（语义层）                         │
│  业务数据集，Join / Enrich 视图                                  │
│  本平台对应：未规划（视 Phase 3 成熟度决定）                      │
├───────────────────────────────────────────────────────────────┤
│  Layer 4: Ontology（本体层）                                   │
│  Object Types / Properties / Links / Actions                  │
│  本平台对应：OntologyObject / EntityType / Link（已实现）        │
└───────────────────────────────────────────────────────────────┘
```

**本平台当前现状（Phase 1 → 2 过渡中）**：

```
Layer 4: Ontology ✅ Phase 1 已实现
Layer 3: Semantic ❌ 未规划
Layer 2: Transform ❌ Phase 3 规划
Layer 1: Raw      ⚠️  Iter-1 补全中（元数据已有，真实文件 Iter-1 实现）
```

当前 sync 流程是 **Layer 1 直接跳 Layer 4**，中间两层有意跳过：

```
DataSource ──sync──▶ DatasetVersion(raw) ──直接写──▶ OntologyObject
```

**跳过 Layer 2/3 的理由**（刻意决策，非遗漏）：
1. Layer 2 是复杂度中枢（DAG 执行引擎），无真实 Transform 需求时引入是过度设计
2. Layer 3 的价值依赖 Layer 2，无衍生 Dataset 则语义层价值有限
3. Layer 1 → Layer 4 直连是中小团队数据工具的标准模式（Airbyte、Fivetran 等均如此）
4. 先跑通端到端可用链路，再按真实需求深化

### 1.2 能力对比（v0.4.0 更新）

| 能力 | Palantir | Phase 1（已完成）| Phase 2（Iter-1~4）| Phase 3（Iter-5~7）|
|------|---------|----------------|-------------------|-------------------|
| 原始数据版本化 | ✅ 不可变快照 | ⚠️ 元数据版本 | ✅ Manifest + 文件 | ✅ |
| Pipeline 加工 | ✅ 可视化 DAG | ❌ | ❌ | ✅ 雏形 |
| 血缘追踪 | ✅ 完整 DAG | ✅ dataset_id | ✅ parent_dataset_id | ✅ 完整 DAG |
| 重跑 Materialize | ✅ 不重拉源 | ⚠️ 需重拉 | ✅ 读 Manifest | ✅ |
| 同步原子性 | ✅ | ✅ SyncRun 锁 | ✅ | ✅ |
| Schema 演进检测 | ✅ | ⚠️ 警告 | ✅ 自动分类 | ✅ |
| 版本回滚 | ✅ | ❌ | ✅ | ✅ |
| 多租户隔离 | ✅ | 暂缓 | ✅ 路径前缀 | ✅ |
| 增量同步 | ✅ | ❌ | ❌ | ✅ cursor-based |
| 定时调度 | ✅ | ❌ | ❌ | ✅ cron |
| 数据质量 | ✅ | ❌ | ❌ | ✅ 基础检查 |
| Transform DAG | ✅ 完整 | ❌ | ❌ | ✅ 雏形 |

### 1.3 StorageBackend 可插拔设计（v0.3.0 引入，v0.4.0 Iter-1 落地）

```
┌─────────────────────────────────────────┐
│  版本管理层 DatasetStore                 │  ← 感知版本，不感知存储细节
│  begin_write → append_records → commit  │
│  rollback / expire_versions             │
├─────────────────────────────────────────┤
│  存储后端层 StorageBackend（可插拔 trait）│  ← 感知字节，不感知版本
│  put / get / list / delete              │
├──────────────┬──────────────┬───────────┤
│  LocalFs     │  S3/OSS/COS  │  RustFS   │
│ （Iter-1）   │  （Iter-2）  │（Iter-2+）│
└──────────────┴──────────────┴───────────┘
```

---

## 二、七次迭代路线图（Phase 2 → Phase 3）

> v0.4.0 核心新增：将 Phase 2/3 细化为 7 个有明确交付物的迭代。

### 总览

```
当前（Phase 1 ✅ 完成）
  DataSource CRUD + Fold/Project 管理 + sync 流程 + 元数据版本 + UI 三件套
      │
      ▼
 ─── Phase 2 ─────────────────────────────────────────────────────
 Iter-1: Phase 2a — 本地存储基础          [当前进行中]
 Iter-2: Phase 2b — 云存储 + 多租户
 Iter-3: Phase 2c — 运维能力（回滚/GC/恢复）
 Iter-4: Phase 2d — 质量与体验（Phase 2 收尾）
 ─── Phase 3 ─────────────────────────────────────────────────────
 Iter-5: Phase 3a — Transform Pipeline 雏形   ← Layer 2 分水岭
 Iter-6: Phase 3b — 增量同步 + 完整血缘
 Iter-7: Phase 3c — 高级能力（Phase 3 收尾）
      │
      ▼
 Phase 3 🎯
```

---

### Iter-1：Phase 2a — 本地存储基础

**核心目标**：DatasetVersion 从"元数据虚影"变成有真实文件的版本

**交付物**：

| 组件 | 内容 |
|------|------|
| `palantir-storage` crate | 新建，包含 StorageBackend trait / LocalFsBackend / DatasetManifest / DatasetStore / DatasetWriter |
| `LocalFsBackend` | 真实文件写入（temp → rename 原子性），put/get/list/delete/delete_prefix |
| `DatasetWriter` | append_records() → 行数阈值分 part 文件（50k 行/part），flush_part() |
| `DatasetManifest` | 写入 manifest.json（files[], schema, total_rows, SHA256 per file, content_hash） |
| SHA256 dedup | content_hash 相同时跳过重复写入 |
| sync 流程接入 | sync_source_handler 接入 DatasetStore，commit 后 manifest_path 写入 dataset_versions |
| Phase 2a 内存取舍 | 先收集全量 records 再写 DatasetWriter（接受内存缓冲，Iter-4 流式化） |

**不做**：S3Backend、版本回滚、SSE、加密

---

### Iter-2：Phase 2b — 云存储 + 多租户

**核心目标**：StorageBackend 可插拔，接入 S3/OSS/MinIO

| 组件 | 内容 |
|------|------|
| `S3Backend` | `object_store` crate，覆盖 AWS S3 / 阿里云 OSS / 腾讯云 COS / MinIO / 华为 OBS |
| 路径规范落地 | `{tenant_id}/{dataset_id}/v{n}/data/part-xxx.csv` |
| 配置驱动切换 | env var `STORAGE_BACKEND=local|s3`，启动时初始化对应 Backend |
| S3 文件列表预览 | DataSource 配置页 Browse tab 真实可用（调用 S3Backend.list） |

---

### Iter-3：Phase 2c — 运维能力

**核心目标**：生产可用的版本管理操作

| 组件 | 内容 |
|------|------|
| 版本回滚 | `is_current` 指针切换 + 异步重物化 OntologyObject（读目标 Manifest，不重拉数据源）|
| 保留策略 GC | `keep_versions` / `keep_days` → 删除旧版本文件 + DB 记录 |
| Crash Recovery | 启动扫描 `status=pending` 且超 15min TTL → abort + release write_lock |
| Schema 演进 | 自动分类（Compatible / BackwardCompatible / ForwardCompatible / Breaking）；Breaking 阻断 sync |
| Schema Diff UI | 前端弹窗展示字段级别 diff，Breaking 时要求手动确认 |

---

### Iter-4：Phase 2d — 质量与体验（Phase 2 收尾）

**核心目标**：Phase 2 功能完整，体验可用

| 组件 | 内容 |
|------|------|
| SSE 进度推送 | `GET /api/jobs/:id/stream`，替代 2s 轮询 |
| 敏感字段加密 | AES-256-GCM 加密 config 中的 AK/SK / 密码字段 |
| FTP/SFTP 真实连接 | `ssh2` crate，真实文件下载 |
| DatasetWriter 流式化 | sync_* 函数逐批 append 到 DatasetWriter，不缓冲全量（解决 Iter-1 内存取舍） |
| 版本 Diff UI | 版本历史页展示 Schema Diff、文件列表、content_hash |

---

### Iter-5：Phase 3a — Transform Pipeline 雏形

**核心目标**：引入 Layer 2，DatasetVersion 可派生（**Layer 2 分水岭**）

| 组件 | 内容 |
|------|------|
| `Transform` 实体 | SQL transform / 字段映射 / 过滤规则，存 DB |
| `DerivedDataset` | `parent_dataset_id` 血缘链接，Dataset 可从另一 Dataset 派生 |
| 简单 DAG | `RawDataset → Transform → DerivedDataset`（单级） |
| 自动触发 | Source sync 完成 → 自动触发下游 Transform |
| Transform UI | Pipeline 配置页（列表形式，Phase 3b 升级为节点连线） |

---

### Iter-6：Phase 3b — 增量同步 + 完整血缘

**核心目标**：减少重复拉取，构建可追溯的数据谱系

| 组件 | 内容 |
|------|------|
| 增量同步 | cursor-based，`WHERE updated_at > last_cursor`；SyncRun 存储水位线 |
| 血缘 DAG API | `GET /api/ontology/objects/:id/lineage` → 返回完整血缘链 |
| cron 调度 | DataSource 配置定时表达式，ADR-11 实现 |
| 多级 DAG | 支持 `RawDataset → Transform → DerivedDataset → Transform → DerivedDataset` |

---

### Iter-7：Phase 3c — 高级能力（Phase 3 收尾）

**核心目标**：Phase 3 功能完整，对齐 Palantir 核心能力

| 组件 | 内容 |
|------|------|
| Parquet 输出 | DatasetWriter 支持 Parquet 格式（`parquet` crate）；DataFusion 直查 |
| 数据质量检查 | 空值率 / 类型一致性 / 异常值检测，结果写入 DatasetVersion |
| 数据融合 | 多 DatasetVersion → 单 EntityType（合并多数据源到同一类型）|
| 存储配额管理 | `total_bytes` 聚合 + 阈值告警 |
| RustFsBackend | 若 RustFS 提供原生 Rust API（S3 不兼容时的备选）|

---

## 三、用户故事（v0.3.0 规范，保持不变）

> 完整 US 见 [ingest-workflow_v0.3.0.md §二](ingest-workflow_v0.3.0.md)
> v0.4.0 不修改任何已有 US，仅增加 Iter 对应关系标注。

**US 与 Iter 对应关系速查**：

| Epic | US | 实现 Iter |
|------|----|---------|
| E1 工作台 | US-E1-01/02 | ✅ Phase 1 完成 |
| E2 Fold 管理 | US-E2-01/02 | ✅ Phase 1 完成 |
| E3 数据源配置 | US-E3-01（S3）| Iter-2（真实文件列表）|
| E3 数据源配置 | US-E3-02（DB）| ✅ Phase 1 完成（SQLite），Iter-2（MySQL/PG）|
| E3 数据源配置 | US-E3-03（REST）| ✅ Phase 1 完成 |
| E3 数据源配置 | US-E3-04（FTP）| Iter-4（真实连接）|
| E4 同步执行 | US-E4-01（触发）| ✅ Phase 1 完成 |
| E4 同步执行 | US-E4-02（进度）| ✅ Phase 1 完成（轮询）；Iter-4（SSE）|
| E4 同步执行 | US-E4-03（失败处理）| ✅ Phase 1 完成 |
| E5 版本管理 | US-E5-01（查看历史）| ✅ Phase 1 完成（元数据）；Iter-1（文件信息）|
| E5 版本管理 | US-E5-02（回滚）| Iter-3 |
| E5 版本管理 | US-E5-03（Schema 演进）| Iter-3 |
| E5 版本管理 | US-E5-04（保留策略）| Iter-3 |

---

## 四、领域模型（v0.3.0，保持不变）

> 完整领域模型见 [ingest-workflow_v0.3.0.md §三](ingest-workflow_v0.3.0.md)

**v0.4.0 新增：palantir-storage crate 中新增的类型**

```rust
// crates/palantir-storage/src/manifest.rs

pub struct DatasetManifest {
    pub dataset_id:   String,
    pub version:      i64,
    pub sync_run_id:  String,
    pub created_at:   u64,           // Unix timestamp
    pub schema:       DatasetSchema,
    pub files:        Vec<FileEntry>,
    pub total_rows:   u64,
    pub total_bytes:  u64,
    pub content_hash: String,        // SHA256(concat(file.sha256 for file in files))
}

pub struct FileEntry {
    pub path:   String,   // 相对路径：data/part-00000.csv
    pub sha256: String,
    pub rows:   u64,
    pub bytes:  u64,
}

pub struct DatasetSchema {
    pub fields: Vec<SchemaField>,
}

pub struct SchemaField {
    pub name:      String,
    pub data_type: String,   // string | integer | float | boolean | timestamp
    pub nullable:  bool,
}

// crates/palantir-storage/src/backend.rs

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn put(&self, path: &str, data: Bytes) -> Result<()>;
    async fn get(&self, path: &str) -> Result<Bytes>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn delete(&self, path: &str) -> Result<()>;
    async fn delete_prefix(&self, prefix: &str) -> Result<u64>;
}

pub struct LocalFsBackend { root: PathBuf }  // Iter-1
pub struct S3Backend      { ... }            // Iter-2
pub struct RustFsBackend  { ... }            // Iter-7（可选）

// crates/palantir-storage/src/store.rs

pub struct DatasetStore {
    backend: Arc<dyn StorageBackend>,
    root_prefix: String,
}

impl DatasetStore {
    pub async fn begin_write(&self, dataset_id: &str, version: i64, sync_run_id: &str)
        -> Result<DatasetWriter>;
    pub async fn read_manifest(&self, dataset_id: &str, version: i64)
        -> Result<DatasetManifest>;
    pub async fn delete_version(&self, dataset_id: &str, version: i64)
        -> Result<u64>;
}

// crates/palantir-storage/src/writer.rs

pub struct DatasetWriter {
    // 内部状态：part 缓冲、文件列表、行计数
}

impl DatasetWriter {
    pub async fn append_records(&mut self, records: &[serde_json::Value]) -> Result<()>;
    pub async fn commit(self, schema: DatasetSchema) -> Result<DatasetManifest>;
    pub async fn abort(self) -> Result<()>;  // GC 已写 parts
}
```

**路径约定（Iter-1 LocalFs，Iter-2 加 tenant_id 前缀）**：

```
Phase 2a（Iter-1）: {data_dir}/{dataset_id}/v{version}/
Phase 2b（Iter-2）: {data_dir}/{tenant_id}/{dataset_id}/v{version}/

目录结构：
  {prefix}/
    manifest.json        ← DatasetManifest
    data/
      part-00000.csv
      part-00001.csv
      ...
```

---

## 五～十（状态机 / 流程图 / API / DB Schema / 前端结构 / 设计盲点）

> v0.4.0 这些章节内容与 v0.3.0 相同，无修改。
> 完整内容见 [ingest-workflow_v0.3.0.md §四~十](ingest-workflow_v0.3.0.md)

**v0.4.0 补充：DatasetVersion DB 字段 manifest_path 在 Iter-1 后开始填充**

```sql
-- dataset_versions.manifest_path 字段（已存在于 Phase 1 Schema）
-- Phase 1: NULL（无实际文件）
-- Iter-1 后: "{dataset_id}/v{version}/manifest.json"
```

---

## 十一、实现状态追踪（v0.4.0 新增）

| 组件 | 状态 | 所属 Iter |
|------|------|---------|
| Fold / Project / DataSource CRUD | ✅ 完成 | Phase 1 |
| sync_source_handler + SyncRun | ✅ 完成 | Phase 1 |
| dataset_versions DB + API | ✅ 完成 | Phase 1 |
| UI：ingest_project / fold 页面 | ✅ 完成 | Phase 1 |
| palantir-storage crate 骨架 | 🚧 进行中 | Iter-1 |
| LocalFsBackend 真实写入 | 🚧 进行中 | Iter-1 |
| DatasetWriter + Manifest | 🚧 进行中 | Iter-1 |
| sync 流程接入 DatasetStore | 🚧 进行中 | Iter-1 |
| S3Backend | ⏳ 待开始 | Iter-2 |
| 版本回滚 | ⏳ 待开始 | Iter-3 |
| Crash Recovery | ⏳ 待开始 | Iter-3 |
| Schema 演进分类 | ⏳ 待开始 | Iter-3 |
| SSE 进度推送 | ⏳ 待开始 | Iter-4 |
| AES-256 加密 | ⏳ 待开始 | Iter-4 |
| Transform Pipeline | ⏳ 待开始 | Iter-5 |
| 增量同步 | ⏳ 待开始 | Iter-6 |
| Parquet 输出 | ⏳ 待开始 | Iter-7 |
