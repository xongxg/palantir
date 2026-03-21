# ADR-34: Dataset 存储后端可插拔 + 版本化管理策略

> 状态：✅ 已决策 | 日期：2026-03-20 | 作者：架构组
> 依赖：[ADR-28](ADR-28-pluggable-storage.md) · [ingest-workflow v0.2.0](../domain/ingest-workflow_v0.2.0.md)

---

## 问题

1. Raw Dataset 需要版本化（每次 SyncRun 产生一个不可变快照），但存储后端多样：本地 FS、rustfs、S3、阿里云 OSS、腾讯云 COS、MinIO
2. 版本管理逻辑（创建、回滚、保留策略、血缘）不应与存储后端耦合
3. 如何在 Phase 1（thin Dataset）基础上演进为 Phase 2（Raw Dataset 快照），而不重写上层代码

---

## 背景：两个正交维度

```
                   版本管理（Versioning）
                         │
          ┌──────────────┼──────────────┐
          │              │              │
     no-version      manifest      full snapshot
    (Phase 1)        (Phase 2)      (Phase 3)
          │              │              │
          └──────────────┼──────────────┘
                         │
                   存储后端（StorageBackend）
                         │
       ┌─────────┬────────┼────────┬──────────┐
       │         │        │        │          │
    LocalFs    rustfs     S3      OSS        MinIO
```

两个维度**完全正交**：存储后端换了，版本管理逻辑不变；版本策略升级，存储后端不动。

---

## 决策

### 核心设计：两层 trait

**层 1 — StorageBackend（纯存储，无版本概念）**

```rust
/// 可插拔存储后端 — 只负责 bytes 的读写，不关心版本
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// 写入对象，path 为相对路径（如 "ds_xxx/v3/part-000.csv"）
    async fn put(&self, path: &str, data: Bytes) -> Result<()>;

    /// 读取对象
    async fn get(&self, path: &str) -> Result<Bytes>;

    /// 检查对象是否存在
    async fn exists(&self, path: &str) -> Result<bool>;

    /// 列出指定前缀下的所有对象路径
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;

    /// 删除单个对象
    async fn delete(&self, path: &str) -> Result<()>;

    /// 批量删除（用于版本清理）
    async fn delete_prefix(&self, prefix: &str) -> Result<u64> {
        let paths = self.list(prefix).await?;
        let count = paths.len() as u64;
        for p in paths {
            self.delete(&p).await?;
        }
        Ok(count)
    }
}
```

**层 2 — DatasetStore（版本管理，依赖 StorageBackend + SQLite Catalog）**

```rust
pub struct DatasetStore {
    backend: Arc<dyn StorageBackend>,
    catalog: Arc<Db>, // SQLite 元数据库
}

impl DatasetStore {
    /// 开启一次新版本写入（返回 DatasetWriter）
    pub async fn begin_write(
        &self,
        dataset_id: &str,
        sync_run_id: &str,
    ) -> Result<DatasetWriter>;

    /// 提交版本（写 manifest + 更新 catalog current_version）
    pub async fn commit(&self, writer: DatasetWriter) -> Result<DatasetVersion>;

    /// 读取当前版本 manifest
    pub async fn current_version(&self, dataset_id: &str) -> Result<DatasetManifest>;

    /// 读取指定版本 manifest
    pub async fn version(&self, dataset_id: &str, version: u32) -> Result<DatasetManifest>;

    /// 列出所有历史版本
    pub async fn list_versions(&self, dataset_id: &str) -> Result<Vec<DatasetVersionMeta>>;

    /// 回滚到指定版本（只更新 catalog 指针，不删除文件）
    pub async fn rollback(&self, dataset_id: &str, version: u32) -> Result<()>;

    /// 按保留策略清理旧版本文件
    pub async fn expire_versions(
        &self,
        dataset_id: &str,
        policy: &RetentionPolicy,
    ) -> Result<u64>; // 返回删除的文件数
}
```

---

## Manifest 结构（版本的核心记录）

```rust
/// 一个版本的完整描述（存储在 object store + SQLite 双写）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetManifest {
    pub dataset_id: String,
    pub version: u32,
    pub sync_run_id: String,
    pub source_id: String,
    pub created_at: u64,            // Unix timestamp
    pub schema: DatasetSchema,      // 字段名 + 类型
    pub files: Vec<FileEntry>,      // 各分片文件
    pub total_rows: u64,
    pub total_bytes: u64,
    pub content_hash: String,       // SHA256 of all file hashes，用于 dedup
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,               // 相对路径，如 "data/part-000.csv"
    pub sha256: String,             // 内容哈希
    pub rows: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSchema {
    pub fields: Vec<SchemaField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,
    pub data_type: String,          // "string" | "integer" | "float" | "boolean" | "timestamp"
    pub nullable: bool,
}
```

**路径约定**：

```
{dataset_id}/
  ├── v1/
  │   ├── manifest.json
  │   └── data/
  │       └── part-000.csv
  ├── v2/
  │   ├── manifest.json
  │   └── data/
  │       └── part-000.csv
  └── v3/               ← current_version
      ├── manifest.json
      └── data/
          ├── part-000.csv
          └── part-001.csv
```

---

## StorageBackend 具体实现

### LocalFsBackend（Phase 1，开发/单机）

```rust
pub struct LocalFsBackend {
    root: PathBuf, // 本地目录，如 ~/.palantir/datasets/
}
```

- 直接用 `std::fs` + `tokio::fs`
- 零依赖，最快上手

### S3Backend（Phase 2，覆盖 AWS S3 / 阿里云 OSS / 腾讯云 COS / MinIO / OBS）

```rust
pub struct S3Backend {
    store: Arc<dyn ObjectStore>, // object_store crate，一个 trait 覆盖所有 S3 兼容存储
    prefix: String,              // bucket 内的路径前缀
}
```

- `object_store` crate 已在 adapters_s3.rs 中引用
- 配置不同的 `endpoint_url` 即可切换云厂商：
  - AWS S3: `https://s3.amazonaws.com`
  - 阿里云 OSS: `https://{region}.aliyuncs.com`
  - 腾讯云 COS: `https://cos.{region}.myqcloud.com`
  - MinIO: 自定义地址

### RustFsBackend（Phase 2/3，本地分布式场景）

```rust
pub struct RustFsBackend {
    client: RustFsClient, // rustfs Rust client
    bucket: String,
}
```

- rustfs 若实现了 S3 兼容接口，可直接复用 `S3Backend`（endpoint 指向 rustfs）
- 若 rustfs 有原生 Rust API，则封装一个专用 Backend

**关键洞察：rustfs 若兼容 S3 协议，= `S3Backend { endpoint: rustfs_url }`，无需单独实现。**

---

## SQLite Catalog 补充表结构

```sql
-- Dataset 版本索引（Catalog）
CREATE TABLE IF NOT EXISTS dataset_versions (
    id             TEXT PRIMARY KEY,        -- version UUID
    dataset_id     TEXT NOT NULL,
    version        INTEGER NOT NULL,
    sync_run_id    TEXT NOT NULL,
    source_id      TEXT NOT NULL,
    manifest_path  TEXT NOT NULL,           -- object store 中 manifest.json 路径
    total_rows     INTEGER NOT NULL DEFAULT 0,
    total_bytes    INTEGER NOT NULL DEFAULT 0,
    content_hash   TEXT NOT NULL,
    schema_json    TEXT NOT NULL,           -- schema 快照，避免读 manifest
    created_at     TEXT NOT NULL,
    is_current     INTEGER NOT NULL DEFAULT 0,  -- 1 = 当前版本
    UNIQUE(dataset_id, version)
);

CREATE INDEX idx_dataset_versions_current
    ON dataset_versions(dataset_id, is_current);

-- 版本保留策略（可选）
CREATE TABLE IF NOT EXISTS dataset_retention_policies (
    dataset_id      TEXT PRIMARY KEY,
    keep_versions   INTEGER NOT NULL DEFAULT 10,  -- 保留最近 N 个版本
    keep_days       INTEGER,                       -- 或保留 N 天内的版本
    updated_at      TEXT NOT NULL
);
```

---

## 版本管理核心逻辑

### 写入新版本（SyncRun 触发）

```
1. DatasetStore::begin_write(dataset_id, sync_run_id)
   └── 分配 version = current_version + 1
   └── 返回 DatasetWriter（路径前缀: {dataset_id}/v{n}/）

2. DatasetWriter::write_chunk(data: Bytes, filename: &str)
   └── 计算 SHA256
   └── 若 content_hash 已存在 → 跳过写入（dedup）
   └── 否则 backend.put({dataset_id}/v{n}/data/{filename}, data)

3. DatasetWriter::finish(schema)
   └── 构建 DatasetManifest
   └── backend.put({dataset_id}/v{n}/manifest.json, manifest_bytes)
   └── catalog.insert_version(manifest) + 更新 is_current

4. 触发 OntologyObject 重物化（异步，由 SyncRun 事件驱动）
```

### 回滚

```
1. 验证目标 version 存在且 manifest 完整
2. UPDATE dataset_versions SET is_current=0 WHERE dataset_id=? AND is_current=1
3. UPDATE dataset_versions SET is_current=1 WHERE dataset_id=? AND version=?
4. 触发 OntologyObject 重物化（基于目标版本 manifest，不重拉数据源）
```

### 版本清理（Retention）

```
1. 查询 is_current=0 且超出 keep_versions 或 keep_days 的版本
2. 对每个待删除版本：backend.delete_prefix({dataset_id}/v{n}/)
3. DELETE FROM dataset_versions WHERE id IN (...)
```

---

## 与 ingest-workflow v0.2.0 演进对齐

| Phase | 存储后端 | 版本化 | Manifest |
|-------|---------|--------|---------|
| Phase 1 | 无（thin Dataset，元数据仅在 SQLite） | ❌ | ❌ |
| Phase 2 | LocalFsBackend / S3Backend | ✅ 每次 SyncRun = 新版本 | ✅ |
| Phase 3 | + RustFsBackend / 多云 | ✅ + 保留策略 + dedup | ✅ + Parquet |

**Phase 1 → Phase 2 升级路径**：

- `ontology_objects` 已有 `dataset_id` 字段（Phase 1 已设计）
- Phase 2 只需：新增 `dataset_versions` 表 + 引入 `StorageBackend` trait
- 上层 API 接口不变（`POST /api/sources/:id/sync` 等）

---

## 备选方案评估

| 方案 | 优点 | 缺点 | 结论 |
|------|------|------|------|
| 直接用 Delta Lake（Rust 版） | 功能完整 | 生态不成熟，依赖重 | ❌ 过早 |
| 直接存 S3，不做版本化 | 简单 | 无法回滚，无血缘 | ❌ 不满足需求 |
| SQLite 存所有原始记录 | 无需对象存储 | 大数据量不可行 | ❌ 扩展性差 |
| **StorageBackend trait + Manifest（本方案）** | 可插拔、可演进、对象级 dedup | Phase 2 需实现 Backend | ✅ 采用 |

---

## 设计盲点补全（v0.3.0 追加）

### 盲点 1：写入原子性 & 崩溃恢复

`begin_write` 开始后进程崩溃 → 对象存储有孤儿文件，SQLite 无记录，永远泄漏。

**解决方案**：

```sql
-- dataset_versions.status 引入 pending 中间状态
-- pending = begin_write 已调用，commit 未完成
-- committed = 正常完成
-- aborted = 主动取消或 GC 清理
```

**崩溃恢复流程（服务启动时执行）**：

```
SELECT * FROM dataset_versions
WHERE status = 'pending'
  AND created_at < NOW() - INTERVAL '15 minutes'

→ 对每条记录：
    backend.delete_prefix("{dataset_id}/v{version}/")
    UPDATE dataset_versions SET status='aborted'
    UPDATE data_sources SET write_lock=NULL WHERE write_lock = sync_run_id
```

---

### 盲点 2：`put()` 原子性（半写文件）

网络中断可能导致 LocalFs 上的文件不完整，SHA256 验证失败但文件已存在。

**解决方案**：LocalFs 使用 temp-then-rename 模式：

```rust
// LocalFsBackend::put 实现
async fn put(&self, path: &str, data: Bytes) -> Result<()> {
    let real_path = self.root.join(path);
    let tmp_path = real_path.with_extension("tmp");
    tokio::fs::write(&tmp_path, &data).await?;
    tokio::fs::rename(&tmp_path, &real_path).await?;  // 原子重命名
    Ok(())
}
// S3/OSS 的 put_object 本身原子（要么成功要么不存在），无需额外处理
```

---

### 盲点 3：Schema 演进兼容性检测

每次 SyncRun 源端 schema 可能变化。`commit` 前必须与上一版本对比。

**三分类处理**：

```
COMPATIBLE          = 无变化                    → 直接通过
BACKWARD_COMPATIBLE = 新增字段（有默认值）       → 自动通过，摘要提示
FORWARD_COMPATIBLE  = 删除字段                  → 弹窗警告，用户确认后继续
BREAKING            = 字段类型变化              → 阻止 commit，返回 422
```

`DatasetManifest` 新增字段：

```rust
pub schema_change: SchemaChange,
// SchemaChange { added, removed, changed, compatibility }
```

---

### 盲点 4：DatasetWriter 必须是流式的

REST API 按页来、DB 按 LIMIT/OFFSET 批来，不能将全量数据 buffer 在内存。

**接口设计**：

```rust
impl DatasetWriter {
    /// 追加一批记录（可多次调用，内部维护当前 part 文件）
    pub async fn append_chunk(&mut self, records: &[Record]) -> Result<()>;

    /// part 文件超过 part_size_limit（默认 128MB）时自动滚动
    fn should_roll(&self) -> bool {
        self.current_part_bytes >= self.part_size_limit
    }
}
```

Phase 1 实现：`append_chunk` 直接写 OntologyObject（无文件落盘），接口已固定，Phase 2 切换为真实文件写入时上层代码不变。

---

### 盲点 5：多租户存储路径隔离

当前路径 `{dataset_id}/v{n}/` 无租户前缀，无法用 S3 IAM Policy 按租户隔离授权。

**路径规范（Phase 2 起强制）**：

```
{tenant_id}/{project_id}/{dataset_id}/v{n}/
  ├── manifest.json
  └── data/
      ├── part-000.csv
      └── part-001.csv
```

S3 IAM Policy 可直接按 `/{tenant_id}/*` 隔离，业务层无需额外 authz 判断。

Phase 1 单租户：`tenant_id` 使用固定值 `"default"`，路径兼容后续升级。

---

### 盲点 6：并发写冲突保护

两个 SyncRun 同时 `begin_write` 同一 `dataset_id` 会产生 version 号冲突。

**乐观锁（CAS）**：

```sql
-- begin_write 时原子占锁
UPDATE data_sources
SET write_lock = ?new_sync_run_id
WHERE id = ?source_id
  AND write_lock IS NULL;
-- 受影响行数 = 0 → 已有同步执行 → 返回 409 Conflict

-- commit 或 abort 后释放锁
UPDATE data_sources SET write_lock = NULL WHERE id = ?source_id;
```

API 返回：

```json
{ "error": "sync_in_progress", "current_job_id": "run_yyy" }  // HTTP 409
```

---

## 实现优先级

```
P0（Phase 1，当前）:
  ├── StorageBackend trait 定义（接口固定，Phase 2 切换后端不改上层）
  ├── LocalFsBackend stub（put/get 接口存在，Phase 1 不落实际文件）
  ├── DatasetManifest + SchemaChange 结构定义
  ├── dataset_versions 表迁移（含 status/is_current/schema_change 字段）
  ├── DatasetWriter.append_chunk（Phase 1 直写 OntologyObject）
  ├── write_lock CAS（data_sources 表，防并发）
  └── 崩溃恢复扫描（服务启动时执行）

P1（Phase 2，有实际存储需求时）:
  ├── LocalFsBackend 真实实现（temp-then-rename，append_chunk 落 CSV）
  ├── Manifest 写入 + SHA256 计算
  ├── S3Backend（object_store crate，覆盖 AWS/OSS/COS/MinIO）
  ├── DatasetStore::rollback（移动 is_current 指针 + 触发重物化）
  ├── 版本保留策略 GC job
  └── 多租户路径前缀（{tenant_id}/{project_id}/{dataset_id}/v{n}/）

P2（Phase 3，有分布式部署需求时）:
  ├── RustFsBackend（若 rustfs 不兼容 S3 协议）
  ├── Parquet 输出格式（arrow2 / parquet2）
  └── DataFusion 直查 DatasetVersion
```

---

## 相关 ADR

- ADR-28：通用存储 trait 体系（StorageBackend 是其具体化）
- ADR-10：Event Bus（SyncRun 完成事件触发重物化）
- ADR-33：模块化部署（LocalFsBackend vs S3Backend 对应不同部署模式）
