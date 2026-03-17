# Palantir — Roadmap & Decisions · 路线图与决策记录

Date / 日期: 2026-03-17 · Repo: /Users/xongxg/works/rust/codex/palantir

---

## Guiding Principles / 指导原则

- 保持领域层纯净无 I/O，适配器放在 infrastructure。
- 小而专一的函数；D3 资产保持原生 HTML/JS。
- Demo 既是可运行样例也是规约，用示例验证变更。

---

## Platform Positioning / 平台定位

Palantir 将自身定位为"企业操作系统"：

- **Foundry** — 数据语义化 + 运营应用；Ontology 是语义与行动的核心。
- **AIP** — 生成式 AI 与代理层，在 Ontology 之上运行。
- **Apollo** — 跨多云与边缘的自治交付与运维。

核心差异：以 Ontology 为中心，将数据、语义与动作统一到运营语境，并提供从数据→AI→部署的闭环能力。

本项目定位：**简化版 Foundry** — 多源数据接入 → Ontology 映射 → 关系发现 → 可视化，平台核心域与业务场景域分离。

---

## Architecture Decisions / 架构决策

### Cargo Workspace 拆分

采用 Cargo Workspace 管理多 crate，应用 DDD 分层架构：

| Crate | 职责 |
|---|---|
| `palantir-domain` | 聚合根、值对象、领域事件（纯领域，无 I/O）|
| `palantir-pipeline` | Dataset / Filter / Join / Aggregate / Sort 管道 |
| `palantir-application` | Commands、Queries、Ontology 发现与图 |
| `palantir-infrastructure` | 内存仓库、CSV 加载、JSON 导出、事件存储适配器 |
| `palantir-ontology-manager` | CSV 适配器、TOML 映射引擎、事件写入 |
| `palantir-agent` | LLM 辅助映射意图识别 |
| `palantir-ingest-api` | Axum HTTP API + 内嵌工作区 UI |

根 crate `palantir` 通过 `pub use` 聚合各 crate，供 examples 统一引用。

### Ontology Manager 设计

- `CanonicalRecord` — 标准化中间层，屏蔽数据源差异
- `SourceAdapter` trait — CSV / SQL / Kafka / Parquet 均实现此接口
- TOML 映射文件 — 声明式字段映射 + 类型转换 + 外键链接
- `DiscoveryEngine::discover_links()` — 自动推断跨实体关系

### Workspace UI — 4-Step Wizard

`/workspace` 四步流程（`crates/palantir-ingest-api/src/ui/workspace.html`）：

1. **Upload Files** — 拖拽多 CSV，每文件通过 `/api/upload` + `/api/inspect` 建立 connector
2. **Map Properties** — 每文件卡片：entity 类型、id 字段、列→属性/类型/FK 表
3. **Build Ontology** — 异步进度面板（per-file → discover → graph），完成后自动跳转
4. **View Graph** — D3 力导向图内嵌，支持全屏

关键端点：
- `POST /api/workspace/build` — 结构化配置 → 生成 TOML → apply all → discover_links → 返回统计
- `GET /api/live_ontology` — 返回当前内存图（entities, relationships, BCs）

### 已修复问题

- **ns 生成 bug** — 原用 tmp 路径含 UUID，改为原始文件名 stem
- **D3 加载失败** — 移除无效 `/static/d3.v7.min.js` + `document.write` fallback，改用 CDN 直接加载

---

## Roadmap / 扩展路线图

### Phase 1 — 基础夯实 / Foundation

**持久化层**
- SQLite（本地）或 Postgres（生产）存储 connectors / mappings / ontology graph
- 落地：`crates/palantir-infrastructure/` 新增 persistence adapter

**更多数据源 Connector**
- SQL（Postgres/MySQL）、REST API、JSON/Parquet、Kafka/消息队列
- 设计：Connector 作为抽象 trait，各源 → `Dataset` → Ontology

**图查询 / 搜索 API**
- 属性过滤：`salary > 100000 AND department = 'Engineering'`
- 图遍历：从 Employee 找所有 HAS 的 Transaction
- 暴露为 REST 或 GraphQL

### Phase 2 — 语义增强 / Semantic Layer

**Ontology Actions（本体动作）**
- Palantir 最大差异点：entity 不只有属性，还有可执行 Actions（Logic / Integration / Workflow / Search）
- 落地：`src/application/action.rs` 已有骨架，扩展即可

**Ontology 版本化与 Diff**
- 每次 apply 生成快照；两版本之间 diff；支持回滚

**Pipeline / Transform Builder**
- 可视化 pipeline（Filter → Join → Aggregate → Derive），结果写入新 entity type
- `crates/palantir-pipeline/` 已有 transforms 骨架

### Phase 3 — 差异化竞争 / Differentiation

**LLM 辅助映射**
- 上传文件后自动建议 entity 名、property 名；语义识别跨文件关联
- 结合点：`crates/palantir-agent/` 已存在

**自然语言查询**
- "给我所有花费超过 5000 的工程师" → 转换为图查询，结果高亮到 D3

**数据血缘（Lineage）**
- 每条 entity 记录来源（文件、apply 批次、mapping）；字段级血缘；影响面分析

**数据质量规则**
- 字段约束（not null、唯一、范围）；导入时校验；质量评分可视化

### 其他方向（来自 Roadmap）

- **Quick Wins**：Ontology 校验器、`petgraph` 图分析、Mermaid/DOT/C4 导出器
- **APIs & UI**：GraphQL 层、Viewer 搜索/过滤/时间滑块/路径高亮
- **Security**：ABAC/RLS + 字段级脱敏、审计与溯源报告
- **Dev UX**：`insta` 快照测试、`cargo dist` 打包

### 优先级总结

| 阶段 | 目标 | 核心价值 |
|------|------|----------|
| Phase 1 | 持久化 + 多 Connector + 图查询 | 从 demo 到可用平台 |
| Phase 2 | Actions + 版本化 + Pipeline Builder | 从"看数据"到"操作数据" |
| Phase 3 | LLM 映射 + NL 查询 + 血缘 | 差异化，超越 BI 工具定位 |

> **最高优先**：Actions + 持久化 — Actions 是 Palantir 与 BI 工具本质区别；持久化解决平台感缺失。

---

## Milestones / 里程碑

- **M1**：Ontology 校验器 + Mermaid/DOT 导出器 + CLI 骨架
- **M2**：Postgres 适配器 + 影响面分析 + Viewer 搜索/过滤
- **M3**：增量 DAG + Ontology 版本化/Diff + 基础 ABAC 遮罩

---

## Run / 运行

```bash
cargo build
cargo run                           # 入口提示
cargo run -p palantir_ingest_api    # 启动工作区 UI → http://localhost:8080/workspace
cargo run --example 01_ddd_core
cargo run --example 08_multi_bc
cargo fmt && cargo clippy
```
