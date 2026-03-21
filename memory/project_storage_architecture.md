---
name: project_storage_architecture
description: palantir-storage crate 设计、Iter-1/2 实现状态、per-tenant bucket 路由方案
type: project
---

## palantir-storage crate（Iter-1 + Iter-2 已完成）

### 核心设计
- `StorageBackend` trait：put/get/list/delete/delete_prefix
- `LocalFsBackend`：本地磁盘，temp→rename 原子写入（Iter-1）
- `S3Backend`：object_store 0.9，支持 AWS/OSS/MinIO/RustFS（Iter-2）
- `DatasetWriter`：50k行/part 分文件，SHA256 per part，manifest.json
- `DatasetStore`：版本路径管理，`begin_write → append_records → commit`

### 路径规范
- S3 源：`{customer_bucket}/platform_datasets/{dataset_id}/v{version}/`
- 本地/DB/CSV/REST：`{PALANTIR_DATA_DIR}/{dataset_id}/v{version}/`

### 方案 B（用户确认）
客户数据写入客户自己的 bucket（不出客户边界）。
平台只管 `platform_datasets/` 前缀，元数据留在 SQLite（未来 MySQL）。

**Why:** 不同客户需要不同 bucket 隔离，符合 Palantir "数据不离开客户环境" 理念。
**How to apply:** S3 数据源 sync 完成后，用同一份 S3 config 写回客户 bucket 的 platform_datasets/ 前缀；非 S3 源用 LocalFsBackend。

### 存储路由逻辑（main.rs write_to_platform_storage）
```
source_type == "s3"/"ftp" → S3Backend(source_config) → platform_datasets/
otherwise → LocalFsBackend(PALANTIR_DATA_DIR) → {dataset_id}/v{version}/
```

### DB 变更
- `dataset_versions.manifest_path TEXT`（idempotent ALTER TABLE）
- `DatasetVersionRow.manifest_path: Option<String>`
- `Db::update_version_manifest_path(version_id, path)`

### 待做（Iter-3+）
- 版本回滚（is_current 指针切换）
- GC（keep_versions / keep_days）
- Crash Recovery（startup 扫描 status=pending 超时）
- Schema 演进检测
