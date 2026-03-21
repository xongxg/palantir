# 架构设计思考日志 — 2026-03-20

> 记录当日架构讨论的推演过程、被否决的方向、以及关键转折点。
> ADR 文件记录"决策是什么"，本日志记录"我们是怎么想到这个决策的"。

---

## 1. 数据接入四层模型的对位分析

### 起点：我们在 Phase 几？

在完成了 ingest-workflow v0.3.0 的设计与实现（DB Schema、API、UI 三件套）之后，重新审视了一个问题：

> "数据接入工作流设计，我们目前的方案与 Palantir 的方案是有差异的，Palantir 是四层，我们目前是处于 Phase 2 对吧"

这个问题促使我们做了一次精确的"对位分析"——我们现在到底对应 Palantir 哪个层次。

### Palantir Foundry 真实四层

Palantir Foundry 的数据接入体系是四层，从底到顶：

```
Layer 4: Ontology（本体层）
         Object Types / Properties / Links / Actions / Views
              ▲
Layer 3: Semantic / Logical（语义层）
         Business datasets, joined / enriched views
              ▲
Layer 2: Transform / Pipeline（加工层）
         Code Repositories / Contour / PySpark DAG → derived datasets
              ▲
Layer 1: Raw / Integration（原始层）
         Immutable snapshots, versioned (Compass FS)
              ▲
         External Data Sources（外部数据源）
```

### 我们的现状：只有 Layer 1 + Layer 4，中间两层跳过

```
Layer 4: Ontology ✅ Phase 1 已实现
         OntologyObject / EntityType / Link

Layer 3: Semantic ❌ 未设计

Layer 2: Transform ❌ Phase 3 规划

Layer 1: Raw ⚠️  Phase 1 → 2 过渡中
         DatasetVersion（当前只有 SQLite 元数据，无真实文件）
```

核心问题：我们的 sync 流程是 **Layer 1 直接跳 Layer 4**。

```
DataSource ──sync──▶ DatasetVersion(raw) ──直接写──▶ OntologyObject
                          ↑
          当前只有元数据（无文件），Layer 2/3 完全缺失
```

### 为什么这样设计是合理的

这是刻意的分级策略，与 [ADR-feedback: 避免过度设计，P0/P2 分级] 一致：

1. **Layer 2（Transform）是复杂度中枢**：引入 DAG、执行引擎、依赖追踪，开发成本极高。在没有真实 Transform 需求之前引入，是过度设计。

2. **Layer 3（Semantic）价值取决于 Layer 2**：没有 Transform 就没有 Derived Dataset，语义层几乎没有独立价值。

3. **端到端可用比"理想架构"更重要**：先把 DataSource → OntologyObject 的完整链路跑通，让用户能用起来，再迭代深化。

4. **Layer 1 → Layer 4 直连是很多 BI 工具的标准做法**（Tableau Prep、Airbyte 等），在中小团队完全够用。

### 最终定位共识

| 我们 | ≈ Palantir | 说明 |
|------|-----------|------|
| Phase 1 | Layer 1 简化版 + Layer 4 | SQLite 元数据版本，直接写 OntologyObject |
| Phase 2 | Layer 1 完整版 | 真实文件写入，Manifest，S3 Backend，版本回滚 |
| Phase 3 | Layer 2 雏形 | Transform Pipeline DAG，衍生 Dataset |
| 未规划 | Layer 3 | 语义层（视 Phase 3 成熟度决定是否引入） |

---

## 2. 从 Phase 到 Iteration 的拆解

### 为什么需要 Iteration 粒度

Phase 1/2/3 的粒度太大，一个 Phase 可能包含 10+ 个独立功能，难以有节奏地推进和评估进度。需要更细的 Iteration 维度。

### 七次迭代路线图

```
Iter-1: Phase 2a — 本地存储基础
  核心：DatasetVersion 从"元数据虚影"变成有真实文件的版本
  LocalFsBackend 真实写入 + DatasetWriter + Manifest + SHA256 dedup

Iter-2: Phase 2b — 云存储 + 多租户
  核心：StorageBackend 可插拔，接入 S3/OSS/MinIO
  S3Backend (object_store crate) + 路径规范 {tenant_id}/{dataset_id}/v{n}/

Iter-3: Phase 2c — 运维能力（回滚 / 保留 / 崩溃恢复）
  核心：生产可用的版本管理操作
  版本回滚（指针切换 + 异步重物化）+ 保留策略 GC + 启动扫描 + Schema 演进

Iter-4: Phase 2d — 质量与体验
  核心：Phase 2 收尾
  SSE 进度推送 + AES-256 敏感字段加密 + FTP/SFTP 真实连接 + UI 版本 diff

Iter-5: Phase 3a — Transform Pipeline 雏形
  核心：引入 Layer 2，DatasetVersion 可派生
  Transform 实体 + DerivedDataset + parent_dataset_id + 自动触发下游

Iter-6: Phase 3b — 增量同步 + 完整血缘
  核心：减少重复拉取，构建可追溯的数据谱系
  cursor-based 增量 + 血缘 DAG API + cron 调度

Iter-7: Phase 3c — 高级能力
  核心：Phase 3 收尾，对齐 Palantir 核心能力
  Parquet 输出 + 数据质量检查 + 多 Dataset → 单 EntityType 合并 + 存储配额
```

### 分水岭判断

- **Iter-1 是当前最高优先级**：没有真实文件写入，DatasetVersion 就是空壳，版本回滚和 Schema diff 都是无本之木。
- **Iter-5 是 Phase 3 的分水岭**：Transform Pipeline 存在与否，决定了我们是"数据导入工具"还是"数据加工平台"。
- **Iter-6 是生产就绪的关键**：全量重拉在大数据量下不可接受，增量同步是必须的。

---

## 3. Iter-1 技术设计要点

### 新建 `palantir-storage` crate

职责：存储抽象层，与持久化元数据（palantir-persistence）完全正交。

```
crates/palantir-storage/
  src/
    backend.rs   ← StorageBackend trait + LocalFsBackend
    manifest.rs  ← DatasetManifest, FileEntry, DatasetSchema
    store.rs     ← DatasetStore (begin_write / commit / abort)
    writer.rs    ← DatasetWriter (append_records / flush_part)
    lib.rs
```

### 关键设计决策

**文件格式：CSV（带表头）**
- Phase 2a 优先，与现有 CSV adapter 体系一致
- Phase 3 升级为 Parquet（DataFusion 支持）

**Part 文件拆分：行数阈值（50,000 行/part）**
- 简单直接，避免字节计算复杂度
- Phase 2 可升级为字节阈值（128MB/part）

**SHA256 dedup：内容级去重**
- content_hash = SHA256(所有 part 文件 hash 的连接)
- 相同内容的 DatasetVersion 不重复写文件

**temp-then-rename 原子性（LocalFsBackend）**
- 写 `.tmp` 后 rename 到正式路径
- 避免部分写入后崩溃留下损坏文件

**Phase 2a 内存取舍**
- Iter-1 中各 sync_* 函数先收集全量 records，再写 DatasetWriter
- 接受在内存中缓冲全量数据的限制（文档说明）
- Iter-4（流式写入）解决此问题

### sync 流程变更（Iter-1 后）

```
原来（Phase 1）:
  DataSource → SyncRun → OntologyObject（直接写）

Iter-1 后（Phase 2a）:
  DataSource → SyncRun → DatasetWriter.append_records() → [part-xxx.csv + manifest.json]
                       ↘ OntologyObject（保持，血缘 dataset_id 关联）
```

两条写路径并行：
1. 文件路径：DatasetWriter → LocalFsBackend → part files + manifest
2. Ontology 路径：直接写 OntologyObject（保持现有行为）

Phase 3 时，Ontology 路径改为从 Manifest 重物化，不再双写。

