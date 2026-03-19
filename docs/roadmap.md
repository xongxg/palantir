# Palantir Roadmap & 技术储备

---

## 概念模型：Ontology / Logic / Action / Workflow 的关系

> 理解四个概念的本质差异，是设计正确系统边界的前提。

### 核心类比

| 概念 | 直觉类比 | 技术本质 |
|------|----------|----------|
| **Ontology** | 地图（世界的实时状态） | 语义化实体图谱，ObjectType + LinkType + 实例数据 |
| **Logic** | 参谋分析（读地图，给判断） | 对 Ontology 的**只读计算**，产出分数/风险值/派生属性，无副作用 |
| **Action** | 下达命令（改变地图一个节点） | 对 Ontology 的**写操作**，有前置条件校验、授权、不可随意重放 |
| **Workflow** | 作战计划（串联分析→决策→执行的闭环） | 跨时间的**编排层**，协调 Logic + Action + 人工节点，自身有持久化状态 |

### Logic vs Action 的本质区别

```
Logic  = 纯函数，无副作用，随时可调用，结果可缓存
Action = 有副作用，改变状态，需授权，不可随意重放
```

### Workflow 的核心特征

1. **有状态 + 跨时间**：Action 是瞬时的（t=0 执行即结束），Workflow 可以存活数天甚至数月，中间状态需持久化
2. **编排者角色**：Workflow 自己不计算、不执行，它决定**什么时候**调用哪个 Logic，在**什么条件下**触发哪个 Action
3. **人机协作节点**：Workflow 可以暂停等待人工审批，Action 不行
4. **事件驱动触发**：Ontology 状态变化 → 自动触发 Workflow 启动

### 完整闭环

```
         ┌─────────────────────────────────────────┐
         │           Ontology（世界状态）            │
         │   Employee / Supplier / Contract / Risk  │
         └───────┬─────────────────────▲────────────┘
                 │ 读取                 │ 写入
                 ▼                     │
         ┌───────────────┐     ┌───────────────┐
         │    Logic       │     │    Action      │
         │  （只读计算）  │     │  （改变状态）  │
         │  风险分 = 87  │     │  暂停合同      │
         └───────┬───────┘     └───────▲────────┘
                 │ 分析结果             │ 执行决策
                 └─────────┬───────────┘
                           │
                  ┌────────▼─────────┐
                  │    Workflow       │
                  │   （编排者）     │
                  │  Logic → 判断    │
                  │  人工 → 审批     │
                  │  Action → 执行   │
                  │  Loop  → 复查    │
                  └──────────────────┘
```

### 典型业务场景举例（供应商风险响应）

```
触发：供应商风险分 > 80（Ontology 状态变化）
  → Workflow 启动
  → Logic: 计算影响范围（关联合同数、金额）
  → 通知采购经理                    ← 人工节点，暂停等待
  → 审批通过 → Action: 暂停合同     ← 写入 Ontology
  → 审批拒绝 → Action: 标记待观察
  → 定时触发季度复查 Workflow
```

> **设计原则**：Logic 保持纯粹（可测试、可复用）；Action 保持原子（单一职责、可审计）；Workflow 负责业务流程的时序和分支——三层职责严格分离，不要在 Action 里写判断逻辑，不要在 Logic 里触发副作用。

---

## 概念深化：AI Agent vs Workflow 的边界

### 核心区别：思考发生在哪个时间

```
Workflow  = 开发者在设计时完成了思考，把结论编码进流程，运行时只是执行脚本
AI Agent  = 思考发生在运行时，每次面对真实数据重新推理，可处理未预料的边缘情况
```

### Workflow 的边界

```
Workflow 只能处理"设计时能枚举的情况"：

  if risk_score > 80  → escalate     ✅ 枚举到了
  if risk_score = 79 but 5 other signals  ❌ 没枚举
  if supplier just completed remediation  ❌ 没枚举
  if this is our only supplier for Q4    ❌ 没枚举

遇到边缘情况：要么走默认分支，要么失败
```

### Agent vs Workflow 分工

```
Agent  负责：理解情况 → 综合多信号推理 → 提出建议
Workflow 负责：执行被批准的计划 → 保证合规 → 留审计记录

实际协作流程：
  用户问题 / 事件触发
       ↓
  Agent 推理（调用 Function 获取信号，综合判断）
       ↓
  Agent 输出建议（附带理由和影响分析）
       ↓
  人确认
       ↓
  触发对应 Workflow（走标准流程，审批、执行 Action、记录）
```

### 信任边界：为什么 Agent 不能直接执行 Action

```
Workflow → 可以直接执行 Action
           理由：开发者已经想清楚了后果，编码进流程，可预测

Agent    → 只能提议，不能直接执行高风险 Action
           理由：LLM 推理不是 100% 可预测，高风险操作必须人工确认

Agent 的工具箱：
  ✅ 调用 Function（读数据、计算）
  ✅ 查询 Ontology（读状态）
  ✅ 提议 Action（建议 + 理由）
  ✅ 触发 Workflow（启动预定义流程）
  ❌ 直接执行高风险 Action（没有人工确认前）
```

### Function / Logic / Agent 的关系澄清

> Logic（计算属性）不是"思维过程"，是机械推导。

```
is_high_risk = risk_score > 80   → 这是规则，不是思维
                                   输入确定，输出确定，没有判断过程

真正的"思维过程"是 Agent：
  综合风险分、历史记录、行业均值、合同影响、替代供应商情况...
  → 在不确定性中形成判断
  → 处理规则无法覆盖的边缘情况
```

| 概念 | 别名 | 本质 | 思考？ |
|------|------|------|--------|
| computed property | Logic（狭义） | 机械推导，表达式 | ❌ |
| Function | 参数化计算 | 命令式代码，可跨对象 | ❌ |
| AI Agent | Logic（广义） | 运行时 LLM 推理 | ✅ |
| Workflow | 流程编排 | 设计时思考的固化 | ❌（思考已在设计时完成） |

### 完整五层模型

```
Ontology   → 事实层    世界是什么样的（状态）
Function   → 规则层    从事实机械推导（无参/有参，纯计算）
AI Agent   → 判断层    运行时推理，处理模糊性和边缘情况
Workflow   → 编排层    将批准的判断安全落地（设计时固化的流程）
Action     → 执行层    改变世界的原子操作

                设计时                运行时
                ──────────────────────────────
Function/Logic  开发者定义规则         机械执行
Workflow        开发者编码流程         机械执行
Action          开发者定义效果         有条件执行
AI Agent        ─────────────────────  LLM 实时推理
```

> **Agent 是大脑（想清楚做什么），Workflow 是手（按规矩去做）。**
> 两者最佳搭档：Agent 判断 → 人确认 → Workflow 执行。

---

## 开发路线图

### P1 — 近期：补全核心层

- PostgreSQL / REST SourceAdapter
- 语义查询 API：`graph.query().navigate()`
- 通用 Function 定义（任意 ObjectType）
- 声明式 Action Schema + 触发条件
- 事件驱动 Workflow 自动触发

### P2 — 中期：AIP + 血缘 + 多端同步

- Agent 接入 Claude API
- Ontology 自然语言问答
- 数据血缘 Transform DAG
- Bi-temporal 双时间轴查询
- Schema 版本管理 + 迁移
- **本地 WAL + LCA Checkpoint**（离线写入队列 + 三路合并基准快照）
- **三路合并引擎**（字段级 diff，OR-Set Link 自动合并）
- **重连同步协议**（delta fetch + rebase push）
- **冲突注册表 + 解决 UI**（Base / Mine / Theirs 三列视图）

### P3 — 长期：生产化

- 低代码 App 构建（Workshop 等效）
- RBAC + ABAC + ReBAC 三维权限控制（见下方权限设计章节）
- PostgreSQL 生产存储
- 大规模图计算（亿级对象）
- 多租户 / 团队协作

> **最高价值单个改进**：Agent 接入 LLM + 直接查询 Ontology。数据已语义化，缺的是自然语言入口——让人能问 *"有哪些高风险供应商？"* 并直接得到答案。

---

## 技术储备：图查询 + Clustering Rust Crate 方案

> 未来开工时的选型参考，按使用场景分类。

### 场景一：图内聚类（Graph Clustering）

在 Ontology 图拓扑中找社区/分组，例如"哪些 Object 之间关联最紧密"。

#### `petgraph` — 图结构与拓扑算法

```toml
petgraph = { version = "0.6", features = ["graphmap"] }
```

| 算法 | API | 适用场景 |
|------|-----|----------|
| 连通分量 | `connected_components` | 找孤立子图 / 分组 |
| 强连通分量 | `tarjan_scc` / `kosaraju_scc` | 有向图循环依赖检测 |
| 最短路径 | `dijkstra` / `astar` | Object 间关系距离 |
| 最小生成树 | `min_spanning_tree` | 稀疏化图结构 |
| 拓扑排序 | `toposort` | Action 依赖顺序 |

```rust
use petgraph::graph::Graph;
use petgraph::algo::tarjan_scc;

let mut g: Graph<ObjectId, LinkType> = Graph::new();
let emp  = g.add_node(obj_employee);
let proj = g.add_node(obj_project);
g.add_edge(emp, proj, LinkType::WorksOn);

let clusters = tarjan_scc(&g); // 返回强连通分量列表
```

#### `linfa` + `linfa-clustering` — 基于属性的 ML 聚类

```toml
linfa             = "0.7"
linfa-clustering  = "0.7"   # DBSCAN, K-Means, OPTICS
ndarray           = "0.15"
```

适合将 Object Properties（数值特征）向量化后聚类，而非纯图拓扑聚类：

```rust
use linfa::prelude::*;
use linfa_clustering::Dbscan;

let dataset = Dataset::from(feature_matrix);
let clusters = Dbscan::params(3).tolerance(0.5).fit(&dataset)?;
```

#### 社区发现（Louvain）

Rust 目前无成熟 Louvain 实现，备选：

- **小图**：基于 `petgraph` 手写模块度优化
- **大图**：通过 `pyo3` 调用 Python `networkx` / `igraph`
- **离线预处理**：Python 跑 Louvain，结果存回 Ontology Object 属性字段

---

### 场景二：分布式集群（Distributed Cluster）

Ontology 数据量超出单机内存时，需要分布式执行。

#### `datafusion` — 分布式 SQL / Arrow 查询引擎

```toml
datafusion = "37"
```

- Object Instance 存为 Parquet，SQL 表达 join 遍历
- 支持自定义 UDF 嵌入图遍历逻辑

```rust
let ctx = SessionContext::new();
ctx.register_parquet("employees", "data/employees.parquet", Default::default()).await?;
ctx.register_parquet("projects",  "data/projects.parquet",  Default::default()).await?;

let df = ctx.sql("
    SELECT e.name, p.name AS project
    FROM employees e
    JOIN works_on w ON e.id = w.employee_id
    JOIN projects p ON w.project_id = p.id
").await?;
```

#### `ballista` — DataFusion 的分布式执行层

```toml
ballista = "0.12"
```

DataFusion 集群版，调度多节点并行执行，适合超大规模 Ontology 数据集。

---

### 场景三：语义图查询（RDF / SPARQL）

#### `oxigraph` — 嵌入式 RDF 图数据库

```toml
oxigraph = "0.3"
```

- 支持 SPARQL 1.1，单机嵌入式，零外部依赖
- Ontology Object/Link 天然映射为 RDF Subject/Predicate/Object

#### `neo4rs` — Neo4j Rust 异步驱动

```toml
neo4rs = "0.7"
```

接入 Neo4j GDS 插件，内置 Louvain / PageRank / Betweenness Centrality / K 近邻等图算法：

```rust
let mut result = graph.execute(
    query("MATCH (e:Employee)-[:WORKS_ON]->(p:Project)-[:USES_ASSET]->(a:Asset)
           WHERE a.type = $t RETURN e.name")
        .param("t", "compute")
).await?;
```

---

### 选型决策矩阵

| 场景 | 数据规模 | 推荐方案 | 备注 |
|------|----------|----------|------|
| Object/Link 图遍历（内存） | < 1M 节点 | `petgraph` | 最轻量，零依赖 |
| 按属性特征聚类 | 任意 | `linfa-clustering` | 需先向量化属性 |
| 社区发现 / Louvain | 中小图 | `petgraph` 手写 / pyo3 | 暂无纯 Rust 成熟库 |
| 大规模数据聚合查询 | > 1B 行 | `datafusion` + Parquet | SQL-first，易上手 |
| 分布式多机图查询 | 超大规模 | `ballista` | DataFusion 集群版 |
| 语义图 / SPARQL 查询 | 中等 | `oxigraph` | 嵌入式，无外部依赖 |
| 复杂图算法（PageRank 等） | 大图 | `neo4rs` + Neo4j GDS | 需外部 Neo4j 服务 |

---

## 技术储备：海量 Ontology 存储管理

> 面向中小/大企业 SaaS 场景，解决多租户隔离、海量对象存储、Schema 演化、查询性能四大核心挑战。

### 多租户存储隔离策略

#### 模式 A — Shared Schema（共享表 + tenant_id）

```
objects 表: id | tenant_id | type_id | properties(JSONB) | created_at
links   表: id | tenant_id | src_id  | dst_id | link_type | props(JSONB)
```

- **优点**：运维简单，资源利用率高
- **缺点**：大租户 noisy neighbor 风险，行级安全策略复杂
- **适用**：SME SaaS，租户多但单租户数据量小

#### 模式 B — Schema Per Tenant（独立 Schema/Namespace）

```
tenant_abc.objects | tenant_abc.links | tenant_abc.object_types
tenant_xyz.objects | tenant_xyz.links | tenant_xyz.object_types
```

- **优点**：索引独立，Schema 可独立演化
- **适用**：中大型企业，租户数 < 1000

#### 模式 C — Database Per Tenant（物理隔离）

```
tenant_abc_db (独立 PostgreSQL / Neo4j 实例)
tenant_xyz_db (独立 PostgreSQL / Neo4j 实例)
```

- **优点**：完全隔离，支持私有部署，满足数据主权
- **适用**：金融、医疗、政府等强合规大企业

---

### 存储引擎选型

#### 方案 1：PostgreSQL + Apache AGE（推荐起步）

```toml
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio"] }
```

- JSONB 存 Object Properties，递归 CTE 做图遍历
- Apache AGE 扩展添加 Cypher 查询能力

```sql
WITH RECURSIVE traversal AS (
  SELECT id, type_id, 0 AS depth FROM objects WHERE id = $start_id
  UNION ALL
  SELECT o.id, o.type_id, t.depth + 1
  FROM traversal t
  JOIN links l ON l.src_id = t.id
  JOIN objects o ON o.id = l.dst_id
  WHERE t.depth < $max_depth
)
SELECT * FROM traversal;
```

#### 方案 2：SurrealDB — 多模型原生图数据库

```toml
surrealdb = "1"
```

- 原生图关系（`->` 语法），内置 Namespace/Database 二级隔离
- 纯 Rust 实现，嵌入式或独立服务均可

```rust
db.use_ns("tenant_abc").use_db("ontology").await?;
let result: Vec<Asset> = db.query(
    "SELECT ->works_on->project->uses_asset->asset.* FROM employee:emp_001"
).await?.take(0)?;
```

#### 方案 3：ScyllaDB / Cassandra — 超高并发写入

```toml
scylla = "0.12"
```

- 邻接表模型，适合实时数据流高频写入（IoT、日志、交易流水）
- 深度遍历需应用层拼接，结合 `datafusion` 做离线分析

#### 方案 4：TiKV + 自定义图层 — 超大规模分布式

```toml
tikv-client = "0.3"
```

- 分布式 KV，水平无限扩展，需自研图语义层，门槛最高

---

### 分层存储架构（冷热分离）

```
┌──────────────────────────────────────────────────┐
│  Hot Layer  (最近 7 天 / 高频访问)                │
│  Redis / DragonflyDB                             │
│  - Object 缓存，Link 邻接表缓存，TTL 自动淘汰      │
├──────────────────────────────────────────────────┤
│  Warm Layer  (运营数据 / 图查询)                  │
│  PostgreSQL+AGE 或 SurrealDB                     │
│  - Object Instance + Link + Schema Registry      │
│  - 支持实时图遍历                                  │
├──────────────────────────────────────────────────┤
│  Cold Layer  (历史 / 分析 / 审计)                 │
│  Parquet on S3/MinIO + DataFusion                │
│  - 按 tenant_id + date 分区                      │
│  - Action Audit Log，离线聚合分析                  │
└──────────────────────────────────────────────────┘
```

```toml
fred         = "8"    # Redis 异步客户端
object_store = "0.9"  # S3/MinIO/GCS 统一抽象
parquet      = "51"   # Apache Parquet 读写
```

---

### 向量 + 图双引擎（AI 语义查询）

```toml
qdrant-client = "1"
```

```
NL 问题 → embedding → Qdrant ANN 搜索 → 候选 Object Set → 精确图遍历 → 结果
```

语义粗筛（Qdrant）+ 关系精筛（图遍历）是 AI 查询的标配架构。

---

### 综合选型建议

| 企业规模 | 存储方案 | 多租户策略 | 图查询 | 分析查询 |
|----------|----------|------------|--------|----------|
| 初创 / 中小 | SurrealDB | Namespace 隔离 | 内置图语法 | DataFusion |
| 中型企业 | PostgreSQL + AGE | Schema Per Tenant | Cypher / CTE | DataFusion + Parquet |
| 大型企业 | PostgreSQL + ScyllaDB | Database Per Tenant | CTE + Neo4j | Ballista |
| 超大规模 | TiKV + 自研图层 | 物理分片 | 自定义 | DataFusion + S3 |

---

### 核心设计原则

1. **存储与查询分离** — Object 存储层和图遍历引擎解耦，可独立升级
2. **Schema 即代码** — Object Type 定义版本化管理，走 CI/CD 流程
3. **租户隔离从第一天开始** — 后期迁移隔离策略代价极高
4. **冷热分离减成本** — 90% 的查询命中 10% 的热数据，缓存层收益显著
5. **向量 + 图双引擎** — 语义粗筛（Qdrant）+ 关系精筛（图遍历），AI 查询标配

---

## 技术储备：多端离线同步与冲突解决

> 场景：Web / App / Desktop 同时修改同一个 Ontology Object，或本地离线修改后服务端已变更（类 Dropbox 文件同步问题）。

### 问题本质：状态分叉

```
t=0  Client 与 Server 状态一致 → Last Common Ancestor (LCA)
     Object A: { risk: "low", tags: ["finance"] }

     ← 网络断开 →

t=1  Client 离线修改：risk="high"，tags+="urgent"
     Server（其他端写入）：risk="medium"，tags+="approved"

t=2  Client 重连，发现两端状态均不等于 LCA → 状态分叉
```

纯 CRDT 解决并发写入；**三路合并（Three-Way Merge）** 解决离线后重连的状态分叉。两者互补，不可替代。

### 三路合并的字段级判断

| LCA | Mine | Theirs | 结论 |
|-----|------|--------|------|
| A | B | A | 只有我改了 → 用 Mine |
| A | A | B | 只有服务端改了 → 用 Theirs |
| A | B | B | 两端改的一样 → 任取其一 |
| A | B | C | 两端都改且不同 → **CONFLICT** |

```
risk 字段：LCA=low, Mine=high, Theirs=medium → CONFLICT ⚠️
tags 字段：Mine+urgent, Theirs+approved，LCA 中均无 → AUTO-MERGE ✓
           结果：["finance", "urgent", "approved"]
```

**关键洞察**：字段级三路合并可以自动解决大多数"表面冲突"，只有语义真正冲突的字段才需要人工介入。

### 四个核心组件

**① 本地 WAL（Write-Ahead Log）**
- 所有写操作先写本地 SQLite，记录 `base_version`（基于哪个版本写的）
- `synced=false` 的 op 在网络恢复后统一同步
- 离线期间 UI 照常响应（乐观更新）

**② LCA Checkpoint**
- 每次成功同步后，保存当时的对象快照 + `VersionVector`
- 作为下次重连三路合并的"公共祖先"

**③ 重连同步协议**
```
Client 重连：
  1. 发送本地 VersionVector 给服务器
  2. 服务器返回自该版本之后的 delta
  3. Client 取 LCA Checkpoint，对每个受影响 Object 做三路合并
  4. 自动解决可合并字段，冲突字段写入冲突注册表
  5. 将本地 ops rebase 到服务端最新版本后推送
  6. 更新 LCA Checkpoint
```

**④ 冲突注册表 + 解决 UI**
- 无法自动合并的字段存入冲突注册表
- UI 展示三列视图：Base（原始值）/ Mine（本地值）/ Theirs（服务端值）
- 用户选择或手动输入合并结果后，生成新 OntologyEvent 推送

### Links 的特殊处理

Links 使用 OR-Set 天然支持三路合并：

```
我加的 link（新 tag）+ 服务端加的 link（新 tag）→ 两条都保留（Add-wins）
我删的 link（tombstone tag）只影响我知道的那个 tag
  → 不影响服务端并发新增的同类 link
```

### OntologyEvent 扩展

```rust
OntologyEvent::Upsert {
    object,
    hlc,
    actor_id,
    base_version: Option<VersionVector>,  // 新增：声明写入基于哪个版本
    // 服务端据此判断是否需要触发三路合并
}
```

### 自动冲突解决策略

| 字段类型 | 自动策略 | 理由 |
|----------|----------|------|
| 数值（风险分、金额） | 取较大值 | 保守策略，偏高风险 |
| 枚举状态（有优先级） | 取优先级高的 | 如 critical > high > medium > low |
| 集合（tags、categories） | OR-Set union | 并发添加不丢数据 |
| 自由文本 | 浮出冲突 | 无法语义合并 |
| Link 关系 | Add-wins | 宁多一条关系也不丢关系 |
| 对象删除 vs 属性修改 | Delete-wins（可配置） | 删除是强意图 |

### Rust Crate 选型

| 用途 | Crate |
|------|-------|
| 基础 CRDT 原语（LWWReg、MVReg、ORSet） | `crdts` |
| 完整协作文档模型 | `automerge` |
| 实时多端同步（含网络层） | `yrs`（Yjs Rust 实现） |
| Hybrid Logical Clock | `hlc` |
| 本地 WAL 持久化 | `rusqlite`（已有） |

### 设计原则

1. **乐观更新** — 本地写操作立即生效，不等服务器确认，保证离线 UX 流畅
2. **LCA 是核心** — 没有公共祖先就无法做三路合并，每次同步必须更新 Checkpoint
3. **字段级粒度** — 合并粒度越细，自动解决率越高；Object 级合并几乎必然冲突
4. **Links 设计为 Add-wins** — 语义图谱宁可有多余关系，不能丢失关系
5. **冲突可见可审计** — 所有冲突记录入库，解决过程可追溯

---

## Ontology 管理典型用户故事

> Ontology Dataset = Ontology Schema（类型定义）+ Knowledge Graph（实例数据）的结合体。
> 以下 User Story 覆盖系统完整生命周期，可作为功能设计与优先级排序的参考基线。

### 角色定义

| 角色 | 职责 |
|------|------|
| 数据工程师 | 管理 Schema、配置数据源、维护映射规则 |
| 业务分析师 | 查询、探索、分析 Ontology 数据 |
| 业务用户 | 执行 Action、参与 Workflow 审批 |
| 合规审计员 | 审查变更记录、追踪数据溯源 |
| AI 工程师 | 在 Ontology 上构建 Function 和 Agent |
| 平台管理员 | 权限管理、同步监控、健康状态 |

---

### 一、Schema 管理（Ontology 层）

**US-1.1 定义 ObjectType**
> 作为**数据工程师**，我想定义新的 ObjectType 及其属性，以便业务实体被正式建模到系统中。
- 可声明名称、属性名称和类型（String / Float / Bool / Date / Enum）
- 定义后立即在查询接口可见

**US-1.2 定义 LinkType**
> 作为**数据工程师**，我想定义 ObjectType 之间的 LinkType（如 Employee -[BelongsTo]→ Department），以便对象间业务关系被语义化表达。

**US-1.3 Schema 版本管理**
> 作为**数据工程师**，我想对 Schema 做版本管理并查看变更历史，以便 Schema 演化时不破坏已有查询和映射。
- 每次变更生成版本号
- 旧版本数据仍可按对应版本 Schema 解析

---

### 二、数据接入（SourceAdapter 层）

**US-2.1 声明式映射配置**
> 作为**数据工程师**，我想通过 TOML 配置文件声明数据源到 ObjectType 的字段映射，以便无需写代码即可接入新数据源。
- 配置 `[source]` / `[mapping]` / `[map]` / `[[links]]` 四个部分
- 保存后自动触发全量导入，支持预览前 N 条结果

**US-2.2 增量同步游标**
> 作为**数据工程师**，我想配置增量同步游标，以便数据源更新时只同步变化记录，不重复全量导入。

**US-2.3 同步状态监控**
> 作为**数据工程师**，我想查看每个 SourceAdapter 的同步状态（上次同步时间、记录数、错误信息），以便知道数据是否新鲜。

**US-2.4 映射结果预览**
> 作为**数据工程师**，我想输入一条原始数据预览映射后的 OntologyObject，以便在正式导入前验证映射正确性。
- 标注出由外键声明自动推导出的 Link

---

### 三、数据探索与查询（Knowledge Graph 层）

**US-3.1 属性过滤查询**
> 作为**业务分析师**，我想按属性过滤 ObjectSet（如所有 risk_score > 80 的供应商），以便快速定位关注对象。

**US-3.2 图遍历查询**
> 作为**业务分析师**，我想沿 Link 关系做多跳图遍历（如 供应商 → 关联合同 → 合同金额汇总），以便回答跨实体的关联问题。
- 每跳可附加过滤条件
- 返回路径上所有节点的属性

**US-3.3 对象关系图**
> 作为**业务分析师**，我想以某对象为中心查看其 N 跳以内的完整关系图，以便理解该对象在图谱中的位置。

**US-3.4 属性变更历史**
> 作为**业务分析师**，我想查看一个 Object 的属性变更历史（谁在什么时候改了什么），以便理解对象状态的演变过程。

---

### 四、计算与分析（Function / Logic 层）

**US-4.1 注册参数化 Function**
> 作为 **AI 工程师**，我想为 ObjectType 注册带参数的计算 Function（如风险评分、影响范围计算），以便业务分析师可复用同一计算逻辑。

**US-4.2 声明计算属性**
> 作为**数据工程师**，我想声明计算属性（如 `is_high_risk = risk_score > 80`）挂载在 ObjectType 上并自动保持更新，以便像访问原生属性一样访问派生值。

**US-4.3 调用带参 Function**
> 作为**业务分析师**，我想在查询时调用带参数的 Function（如 `find_affected_contracts(since="2024-01-01")`），以便在不同条件下复用同一计算逻辑。

---

### 五、业务操作（Action 层）

**US-5.1 执行 Action**
> 作为**采购经理**，我想对高风险供应商执行"暂停合同"操作并填写原因，以便状态变更被记录并通知相关方。
- 操作前显示前置条件检查
- 操作成功后自动更新对象状态、发送通知、生成审计记录（操作人、时间、原因）

**US-5.2 查看可用 Action**
> 作为**业务用户**，我想查看某对象上当前可执行哪些 Action，以及每个 Action 是否满足前置条件（不满足时说明原因），以便明确知道下一步能做什么。

**US-5.3 批量 Action**
> 作为**风险专员**，我想对一批筛选出的对象批量执行同一 Action，以便高效处理大量同类情况。

---

### 六、流程编排（Workflow 层）

**US-6.1 事件自动触发 Workflow**
> 作为**风险专员**，当供应商风险分超过阈值时，我希望系统自动启动审查流程并通知我，以便高风险情况不被遗漏。
- Ontology 状态变更自动触发 Workflow
- 通知包含对象概况和触发原因

**US-6.2 我的待处理任务**
> 作为**任务处理人**，我想看到所有分配给我的待处理 Workflow 任务（含上下文：为什么触发、当前第几步），以便优先处理紧急任务。

**US-6.3 Workflow 执行历史**
> 作为**流程设计师**，我想查看一个 Workflow 实例的完整执行历史（每步时间、执行人、输入输出），以便追溯流程执行过程。

**US-6.4 超时自动升级**
> 作为**业务用户**，当 Workflow 等待审批超过 48 小时，我希望系统自动升级给上级，以便紧急流程不因人员缺席而卡住。

---

### 七、AI 问答（Agent 层）

**US-7.1 自然语言查询**
> 作为**采购经理**，我想用自然语言问"这个季度有哪些高风险供应商，各自影响哪些合同？"并得到结构化回答，以便无需手动写查询。

**US-7.2 推理过程可解释**
> 作为**风险专员**，我想问 Agent"供应商 S-042 当前风险高的原因是什么？"并得到带有推理依据的解释（引用了哪些属性和 Function），以便判断分析是否可信。

**US-7.3 建议转 Workflow**
> 作为**决策者**，我想让 Agent 给出处置建议（含影响分析），并能一键将建议转化为 Workflow 执行，以便减少人工判断时间。
- Agent 不能绕过人工确认直接执行 Action

---

### 八、审计与溯源（Compliance 层）

**US-8.1 Action 审计日志**
> 作为**合规审计员**，我想查询某段时间内所有 Action 的执行记录（操作人、时间、对象、变更内容），以便进行合规审查。

**US-8.2 数据血缘追踪**
> 作为**合规审计员**，我想追踪计算属性的数据血缘（risk_score 来自哪些原始字段，经过了哪些 Function），以便验证计算结果准确性。

**US-8.3 历史快照查询**
> 作为**合规审计员**，我想查看某个 Object 在任意历史时间点的状态快照，以便还原事发当时的数据上下文。

---

### 优先级排序

```
P0 — 核心可用（MVP）
  US-2.1  TOML 配置接入数据源
  US-3.1  属性过滤查询
  US-3.2  Link 图遍历
  US-5.1  执行 Action（含审计记录）
  US-5.2  查看可用 Action 及前置条件

P1 — 业务完整
  US-1.1  定义 ObjectType / LinkType
  US-1.3  Schema 版本管理
  US-2.3  同步状态监控
  US-2.4  映射结果预览
  US-4.2  声明计算属性
  US-6.1  事件触发 Workflow
  US-6.2  我的待处理任务
  US-8.1  Action 审计日志

P2 — 智能化 & 合规
  US-7.1  自然语言查询
  US-7.3  Agent 建议 → Workflow 执行
  US-8.2  数据血缘追踪
  US-3.4  属性变更历史
  US-8.3  历史快照查询
```

---

## 技术储备：角色权限控制设计

> Ontology 系统的权限控制比普通系统复杂——数据是图，权限需要在操作、数据、属性三个维度同时生效，且图遍历时权限需逐跳传播。

### 三个维度

```
普通系统：  角色 → 操作权限
Ontology：  角色 → 操作权限         （能做什么）
                 → 数据权限          （能看哪些对象）
                 → 属性权限          （同一对象能看哪些字段）
```

---

### 维度一：操作权限

| 操作 | 数据工程师 | 业务分析师 | 采购经理 | 风险专员 | 合规审计 | 平台管理员 |
|------|:---------:|:---------:|:-------:|:-------:|:-------:|:---------:|
| Schema Write | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Data Read    | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Data Write   | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Action Execute | ❌ | ❌ | ✅ 部分 | ✅ 部分 | ❌ | ✅ |
| Workflow Start/Approve | ❌ | ❌ | ✅ | ✅ | ❌ | ✅ |
| Audit Read   | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| Admin        | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |

---

### 维度二：数据权限（ObjectType 级 + 实例级）

**ObjectType 级**：角色能访问哪些类型

```
采购经理  → 可见: Supplier, Contract, Transaction
风险专员  → 可见: Supplier, Contract, Employee(部分), RiskReport
合规审计  → 可见: 所有类型 + 审计日志，不可写任何内容
```

**实例级（Row-Level Security）**：同一 ObjectType，只能看自己管辖范围内的实例

```
采购经理 Alice 只能看她负责的供应商：

  方式A（属性条件）：
    condition: supplier.manager_id == current_user.id

  方式B（图关系，更自然）：
    condition: current_user -[Manages]→ supplier
```

方式B 即 **ReBAC（Relationship-Based Access Control）**——用 Ontology 图中已有的关系来表达权限，无需维护单独的权限表。

---

### 维度三：属性权限（字段级安全）

同一个对象，不同角色看到的字段不同：

```
Supplier 对象：

  name            → 所有角色可见
  contact_email   → 采购经理、风险专员可见
  risk_score      → 风险专员、合规审计可见
  contract_value  → 财务分析师、CFO 可见
  internal_notes  → 风险专员专属
  bank_account    → 财务部门专属，其他人显示 ****
```

---

### 图遍历时的权限传播

Ontology 特有问题：沿 Link 遍历时，每一跳都要检查权限。

```
查询：Employee → [BelongsTo] → Department → [HasBudget] → Budget

步骤1: 能看 Employee？        Yes → 继续
步骤2: 能看 BelongsTo 边？    Yes → 继续
步骤3: 能看 Department？      Yes → 继续
步骤4: 能看 HasBudget 边？    No  → 停止

结果：返回 Employee + Department，Budget 被静默过滤
      不报错，不暴露"该节点存在但无权限"的信息
```

**关键原则**：权限不足时静默过滤，不泄露受限数据的存在。

---

### Action 与 Workflow 的权限绑定

```rust
struct ActionDef {
    required_roles:     Vec<Role>,        // 哪些角色能执行
    instance_condition: Option<Expr>,     // 实例级条件
    // 例：采购经理只能暂停自己管辖的供应商
    // "current_user -[Manages]→ target_object"
}

struct WorkflowDef {
    can_start:  Vec<RoleOrTrigger>,       // 谁能启动
    // 每个 HumanTask 有独立的 assign_to 逻辑
    // 例：审批步骤只能由 target 对象的上级经理处理
}
```

---

### 三层叠加的权限模型

```
RBAC（基础层）— 角色 → 操作权限
  role: ProcurementManager
  permissions: [read:Supplier, execute:suspend_supplier]

ABAC（条件层）— 在 RBAC 基础上加属性条件
  read:Supplier WHERE supplier.region == current_user.region

ReBAC（图关系层）— 用图中已有关系表达权限，最适合 Ontology
  current_user -[Manages]→ supplier → 可执行 suspend_supplier
```

**数据结构：**

```rust
struct Permission {
    role:      Role,
    action:    PermAction,     // Read | Write | Execute | Admin
    resource:  Resource,       // ObjectType | ActionDef | WorkflowDef
    condition: Option<Expr>,   // 实例级条件（可引用图关系）
    fields:    FieldPolicy,    // 属性级控制
}

enum FieldPolicy {
    All,
    Allow(Vec<AttrId>),        // 白名单：只能看这些字段
    Deny(Vec<AttrId>),         // 黑名单：除这些字段外都可见
    Masked(Vec<AttrId>),       // 脱敏：字段存在但值显示为 ****
}
```

---

### 完整示例：采购经理 Alice 的权限全景

```
用户: Alice（角色: ProcurementManager）

操作权限：
  ✅ 读取 Supplier（条件：current_user -[Manages]→ supplier）
  ✅ 读取 Contract（条件：linked to her suppliers）
  ✅ 执行 suspend_supplier（条件：she manages target）
  ✅ 参与 supplier_risk_response Workflow（审批步骤）
  ❌ 读取 Employee 薪资字段
  ❌ 修改 Schema
  ❌ 查看审计日志

可见字段（Supplier 对象）：
  ✅ name, status, contact_email, risk_score
  ❌ internal_notes（风险专员专属，静默隐藏）
  👁 bank_account → 显示为 ****（脱敏）

图遍历行为：
  Supplier → [SignedWith] → Contract   ✅ 可遍历（她管辖的合同）
  Contract → [ApprovedBy] → Employee   ❌ 无权限，静默过滤
                                           不返回 Employee 节点
```

---

### 对 User Story 的影响

每个 US 需要绑定权限声明，以下为补充示例：

```
US-5.1 执行 Action（暂停供应商）
  权限绑定：
    - 可执行角色：ProcurementManager、RiskOfficer
    - 实例条件：采购经理只能操作自己管辖的供应商
    - 审计：操作记录自动写入审计日志

US-3.2 图遍历查询
  权限绑定：
    - 每跳自动做权限过滤
    - 无权限节点静默过滤，不报错
    - 无权限字段在结果中不出现或脱敏

US-7.1 Agent 自然语言查询
  权限绑定：
    - Agent 只能访问当前用户权限范围内的数据
    - Agent 调用 Function 时继承用户权限上下文
    - Agent 不能通过"绕路"查询获取超出权限的数据
```

---

### Rust 实现参考

| 用途 | 方案 |
|------|------|
| 基础 RBAC | 自实现（权限表存 SQLite） |
| 策略引擎 | [`casbin`](https://crates.io/crates/casbin)（支持 RBAC/ABAC） |
| 细粒度策略 | [OPA](https://www.openpolicyagent.org/)（Rego 语言，sidecar 部署） |
| 图关系权限（ReBAC） | 基于 Ontology 图自实现（`current_user -[Manages]→ target` 查询） |

### 设计原则

1. **最小权限原则** — 默认无权限，显式授权才可访问
2. **权限随图传播** — 遍历每跳检查，不因起点有权限就传递到终点
3. **静默过滤** — 无权限时过滤数据，不暴露受限数据的存在
4. **ReBAC 优先** — 用 Ontology 图中已有的业务关系表达权限，减少单独维护的权限表
5. **权限即数据** — 权限规则本身也是 Ontology 中的对象，可被查询和审计
6. **Agent 权限继承** — AI Agent 的数据访问权限不超过发起查询的用户权限

---

## 概念深化：AIP 与 Ontology Knowledge Graph 的结合

> AIP（AI Platform）= 把 LLM 的推理能力接在 Ontology 的业务数据上。
> 数据已语义化，缺的是自然语言入口和推理层。

### 核心问题

```
LLM 很聪明，但它不认识你的公司
  → 不知道你的供应商叫什么、合同金额多少、上周发生什么风险事件

Ontology 有你的数据，但它不会"思考"
  → 知道所有事实，但不能回答"我该怎么办"

AIP = LLM 推理能力  ×  Ontology 业务数据
```

### 三种接法的本质差异

| 方案 | 机制 | 问题 |
|------|------|------|
| RAG | 检索文档片段 → LLM 读文本 | 非结构化，LLM 不知道 ACME 是 Supplier 对象，不知道它的关联关系 |
| Fine-tuning | 把数据烧进模型权重 | 数据过时，昨天新增的供应商模型不知道，重训代价极高 |
| **AIP** | LLM 调用 Ontology Functions 获取结构化数据，在真实数据上推理 | 数据永远最新，每个结论可追溯到具体对象 |

---

### 三个连接点

**连接点 1：Function 作为 LLM 的工具（Tool Use）**

LLM 不直接查数据库，调用已注册的 Ontology Function：

```
用户："这周哪些供应商值得关注？"

LLM 调用：
  tool: get_high_risk_suppliers(threshold=80)
    → ObjectSet<Supplier> [ACME(87), BETA(82)]

  tool: get_recent_incidents(days=7)
    → [ACME: 3次延迟交付, BETA: 刚完成整改]

  tool: navigate(ACME, "SignedWith", "Contract")
    → [合同C-001($1.2M), 合同C-003($0.8M)]

LLM 推理：
  "ACME 风险最高(87)，$2M 敞口，近期无改善
   BETA 风险次之(82)，已完成整改，压力下降
   → 优先关注 ACME"
```

LLM 不需要知道 SQL 或图查询语法，只需知道有哪些 Function 可以调用。

**连接点 2：Action 作为 LLM 的输出**

LLM 推理后提议 Action，人确认后才执行：

```
LLM 输出：
  建议执行：suspend_supplier
  目标：ACME (supplier-001)
  原因：风险分 87，7 天内 3 次延迟，Q4 敞口 $2M
  影响：受影响合同 C-001、C-003；替代供应商 S-042 可接手 C-001

       ↓ 人工确认

系统执行：OntologyEvent 写入 → 邮件通知 → 审计日志
```

LLM 永远不跳过人工确认直接写 Ontology。

**连接点 3：Workflow 中的 AI 步骤**

```
模式 A：AI 作为 Workflow 中的一步
  Step 1: 事件触发（风险分 > 80）
  Step 2: [AI Step] 分析影响范围，生成处置建议   ← AIP 在这里
  Step 3: HumanTask 审阅建议，选择方案
  Step 4: 执行对应 Action

模式 B：AI 决定触发哪个 Workflow
  用户："帮我处理 ACME 的风险"
  LLM 分析 → 触发 supplier_risk_response Workflow
           → 传入 { supplier_id: "acme-001", urgency: "high" }
```

---

### 完整运作循环

```
用户自然语言输入
        │
        ▼
┌─── AIP Orchestration ───┐
│  LLM（推理引擎）         │
│    ↕ tool_call           │
│  Tool Registry           │
│  （Ontology Functions）  │
└─────────┬────────────────┘
          │
          ▼
┌─── Ontology / Knowledge Graph ───┐
│  ObjectSet Query                  │
│  Graph Traversal                  │
│  Computed Properties（Function）  │
└─────────┬─────────────────────────┘
          │ 结构化数据返回
          ▼
LLM 在真实数据上推理
          │
          ▼
输出：建议 + 理由 + 影响分析
          │
          ▼
人工确认（Human in the Loop）
          │
          ▼
执行 Action / 触发 Workflow
          │
          ▼
Ontology 状态更新 ──────────────► 循环（新状态触发新分析）
```

---

### Grounding（接地）机制

AIP 最核心的设计：每个 AI 结论都可追溯到具体 Ontology 对象。

```
普通 LLM：
  "ACME 供应商有风险"  ← 从哪来的？可能是幻觉

AIP grounded LLM：
  "ACME(id:supplier-001) 风险分 87
   来源：Function supplier_risk_score
   依据：payment_late_count=3, delivery_rate=0.72
   关联：合同C-001($1.2M), C-003($0.8M)
   数据时间戳：2026-03-19T14:23:00Z"

每个结论有 Ontology 对象 ID 支撑，用户可点击查看原始数据
```

---

### 系统实现层次

```
┌──────────────────────────────────────────┐
│              AIP Layer                   │
│                                          │
│  ① Tool Registry                        │
│     Ontology Functions → LLM 可调用工具 │
│                                          │
│  ② Context Builder                      │
│     ObjectSet 结果 → LLM 可读格式       │
│                                          │
│  ③ Action Proposer                      │
│     LLM 输出 → 结构化 Action 提议       │
│                                          │
│  ④ Safety Validator                     │
│     验证提议是否在权限范围内             │
│     继承当前用户的 Ontology 权限         │
│                                          │
│  ⑤ Feedback Writer                     │
│     人工决策结果写回 Ontology            │
└──────────────────┬───────────────────────┘
                   │
┌──────────────────▼───────────────────────┐
│       Ontology + Knowledge Graph         │
│                                          │
│  Schema：ObjectType / LinkType           │
│  数据：  ObjectInstance / LinkInstance   │
│  计算：  Functions（注册为 AI 工具）     │
│  操作：  Actions（AI 提议，人确认）      │
│  流程：  Workflows（AI 可触发）          │
└──────────────────────────────────────────┘
```

---

### AIP vs 传统 BI

```
传统 BI：
  人 → 写查询 → 得到数据 → 人解读 → 人决策

AIP + Ontology：
  人 → 自然语言 → AI 调用 Functions
              → AI 解读并给出建议（含依据）
              → 人确认
              → 系统执行

从"人查数据"变成"AI 用数据帮人决策"

Ontology 是这一切的地基——没有语义化的数据，AI 只能说废话
```

### 设计原则

1. **AI 不直接写数据** — LLM 只提议，Action 执行必须经过人工确认
2. **权限继承** — AI 查询数据时继承发起用户的 Ontology 权限，不可越权
3. **结论可追溯** — 每个 AI 建议都附带 Ontology 对象 ID 和数据时间戳
4. **Function 是桥梁** — Ontology Functions 是 LLM 和数据之间的唯一接口，不允许 LLM 直接构造原始查询
5. **Workflow 提供护栏** — 高风险操作必须通过预定义 Workflow 执行，不能由 AI 自由编排
6. **反馈闭环** — 人工决策结果写回 Ontology，新状态触发新一轮 AI 分析

---

## 概念深化：AIP Agent 低延迟设计

> 用户不能等——把等待时间从请求链路移到后台事件处理链路。

### 延迟来源分析

```
用户问："ACME 供应商怎么处理？"（Reactive 模式）

Step 1: LLM 理解意图            200–500ms
Step 2: 决定调用哪些 Function    100ms
Step 3: 调用 Function 查数据    100–2000ms
Step 4: 等结果返回再推理         500–1500ms
Step 5: 再调用下一个 Function   （又一轮）
Step 6: 生成最终回答             500–1000ms

串行执行总计：2–8 秒，用户等不了
根本原因：Reactive 模式——用户问了才开始跑
```

---

### 解法一：Reactive → Proactive（最重要）

```
Reactive（慢）：用户问 → Agent 运行 → 查数据 → 推理 → 回答
Proactive（快）：数据变化 → Background Agent 自动分析 → 缓存好
                用户问   → 命中缓存 → <100ms 回答
```

接在现有 OntologyEvent 体系上：

```rust
// 订阅 Ontology 变更事件，提前触发分析
pub struct ProactiveAgent {
    triggers: Vec<EventPattern>,  // 监听哪些事件
    analysis: AnalysisFn,         // 预计算哪些分析
    cache:    Arc<AnalysisCache>, // 结果存哪里
}

// 示例：供应商风险分变化 → 立刻预计算分析报告
ProactiveAgent {
    triggers: [OntologyEvent::Upsert {
        entity_type:    "Supplier",
        changed_fields: ["risk_score"],
    }],
    analysis: |supplier, graph| {
        let contracts  = graph.navigate(supplier, "SignedWith");
        let exposure   = contracts.sum("value");
        let alternates = graph.navigate(supplier, "CanBeReplacedBy");
        AnalysisResult { supplier_id, exposure, recommendation, .. }
    },
    cache: redis_cache(ttl = 30min),
}
```

时序对比：

```
t=0    ACME.risk_score 变为 87 → OntologyEvent 发出
t=0.5  Background Agent 收到 → 开始分析（用户不感知）
t=2    分析完成，存入 Redis

t=60   用户问："ACME 怎么处理？"
t=60.1 命中缓存 → 立即返回    ← <100ms
```

---

### 解法二：并行执行（Plan-then-Execute）

```
ReAct 串行（慢）：
  think → call F1 → wait → think → call F2 → wait → answer
  总时间 = F1 + F2 + F3 + 推理

Plan-then-Execute 并行（快）：
  Step 1: LLM 生成执行计划（不调用任何 Function）
    plan = [get_risk(ACME), get_contracts(ACME), get_alternates(ACME)]

  Step 2: 并发执行
    tokio::join!(F1, F2, F3)

  Step 3: LLM 拿全部结果一次性推理 → 输出

  总时间 = max(F1, F2, F3) + 推理    而非 F1+F2+F3+推理
```

```rust
pub async fn plan_then_execute(
    query: &str, context: &OntologyContext, llm: &dyn LLM,
) -> AgentResult {
    // Step 1: 规划（极快，不执行 Function）
    let plan: Vec<ToolCall> = llm.plan(query, context.schema()).await?;

    // Step 2: 并发执行所有 Function
    let results = futures::future::join_all(
        plan.iter().map(|call| context.execute(call))
    ).await;

    // Step 3: 一次性推理
    llm.synthesize(query, results).await
}
```

---

### 解法三：分层缓存

```
L1 — 内存缓存  (<5ms)
  热点 Object 属性快照
  近 5 分钟内 Function 结果
  computed properties

L2 — Redis 缓存  (<20ms)
  Background Agent 预计算分析
  高频图遍历结果（top-100 高风险供应商）
  TTL 与 OntologyEvent 联动精准失效

L3 — Ontology 实时查询  (100ms–2s)
  复杂图遍历 / 低频查询
  仅 Cache miss 时走这里
```

事件驱动精准失效（不全量清除）：

```rust
async fn on_ontology_event(event: &OntologyEvent) {
    match event {
        OntologyEvent::Upsert { object } => {
            cache.invalidate_by_object_id(&object.id).await;
            cache.invalidate_by_pattern(
                format!("navigate:*:{}:*", object.id)
            ).await;
        }
        _ => {}
    }
}
```

---

### 解法四：流式输出（降低感知延迟）

```
不流式：用户等 4 秒，然后一次性看到全文

流式：
  t=0.5s  "根据当前数据，ACME 供应商风险分为 87——"
  t=1.0s  "近 7 天有 3 次延迟交付，"
  t=1.5s  "关联合同 2 份，总金额 $2M。"
  t=2.5s  "建议优先暂停，替代供应商 S-042 可接手 C-001。"

感知延迟：0.5s（虽然总时间仍是 2.5s）
```

同步展示推理步骤：

```
🔍 正在查询 ACME 供应商信息...
📊 正在计算关联合同影响...
🤔 正在分析替代供应商可行性...
✅ 分析完成：建议立即暂停，原因如下...
```

---

### 解法五：分级模型路由

```
用户问题
    │
    ▼
意图分类（轻量模型，<50ms）
    │
    ├── 简单查询："ACME 的风险分是多少？"
    │     → 直接查缓存，不走 LLM            <100ms
    │
    ├── 中等查询："高风险供应商列表"
    │     → 小模型 + Function 调用          <1s
    │
    └── 复杂推理："ACME 应该怎么处理？"
          → 大模型 + Plan-then-Execute      2–4s
            Background Agent 可能已预计算
```

---

### 完整低延迟架构

```
              用户查询
                 │
                 ▼
        ┌── 意图路由 (<50ms) ──┐
        │                      │
     简单查询              复杂查询
        │                      │
        ▼                      ▼
   L1/L2 缓存         Background Agent 预计算？
   直接返回                 │
   (<100ms)        ┌────────┴────────┐
                 命中缓存         未命中
                   │                 │
              取缓存(<20ms)   Plan-then-Execute
                              并发 Function 调用
                              流式输出(体感<1s)

Background Agent（持续后台运行）：
  监听 OntologyEvent → 提前分析 → 存 Redis → 等用户来取
```

---

### 与现有体系的接入点

```
现有：
  OntologyEvent → OntologyManager → 持久化

扩展：
  OntologyEvent → OntologyManager  → 持久化
               └→ ProactiveAgentBus → Background Agents
                                    → AnalysisCache (Redis)
                                    ← 用户查询命中缓存
```

### 设计原则

1. **Proactive 优先** — 数据变化时主动分析，不等用户触发
2. **并发替代串行** — Plan-then-Execute，Function 并发执行
3. **精准缓存失效** — OntologyEvent 驱动，按 object_id 失效，不全量清除
4. **流式优先** — 第一个 token 尽快出现，感知延迟远比总延迟重要
5. **分级路由** — 简单问题走缓存，复杂问题走大模型，不一刀切
6. **后台承担等待** — 把用户等待时间转移到 OntologyEvent 的后台处理链路

---

## 概念深化：AIP Agent 进阶改进

### 一、Agent 智能质量

#### 1.1 多 Agent 协作（Specialist + Supervisor）

```
用户查询
    │
    ▼
Supervisor Agent（意图理解 + 任务拆解）
    ├── Risk Agent       → 风险分析
    ├── Contract Agent   → 合同条款提取
    ├── Compliance Agent → 合规检查
    └── Finance Agent    → 财务指标计算
          │
          ▼
     结果聚合 → 统一输出
```

- Supervisor 负责路由和汇总，不直接执行
- 每个 Specialist 只关注自己领域的 Function 集合
- 并发执行，结果回归 Supervisor 合并

#### 1.2 Self-Reflection（Critic Agent）

```
Agent 输出
    │
    ▼
Critic Agent（质量审核）
    ├── 是否遗漏关键数据？ → 补充查询
    ├── 结论与 Ontology 是否矛盾？ → 标记
    └── 置信度是否足够？ → 降级或拒绝
```

**Rust 草图：**

```rust
async fn reflect_and_improve(
    draft: &AgentOutput,
    ontology: &dyn OntologyReader,
) -> AgentOutput {
    let issues = critic_check(draft, ontology).await;
    if issues.is_empty() {
        return draft.clone();
    }
    // 针对每个 issue 补充查询，重新生成
    let supplemented = fetch_missing(issues, ontology).await;
    regenerate(draft, supplemented).await
}
```

#### 1.3 置信度评分

每次 Agent 输出附带置信度：

| 置信区间 | 动作 |
|---------|------|
| ≥ 0.85 | 直接展示 |
| 0.6–0.85 | 展示 + 标注"AI 建议，请核实" |
| < 0.6 | 触发 Critic Agent 或人工介入 |

---

### 二、缓存与性能

#### 2.1 语义缓存（Semantic Cache）

传统缓存：精确 key 匹配  
语义缓存：**相似查询复用结果**

```
用户问："2024年Q3风险最高的合同"
    │
    ▼
Embed(query) → ANN 搜索缓存库
    │
    ├── similarity > 0.92 → 返回缓存（含命中依据）
    └── similarity < 0.92 → 走 Agent，结果写入缓存

缓存失效：对应 Ontology ObjectType 有 Upsert/Delete 事件时清除
```

**Rust 草图：**

```rust
struct SemanticCache {
    store: Vec<(Vec<f32>, AgentOutput)>,  // (embedding, result)
    threshold: f32,
}

impl SemanticCache {
    fn lookup(&self, query_emb: &[f32]) -> Option<&AgentOutput> {
        self.store.iter()
            .filter_map(|(emb, out)| {
                let sim = cosine_similarity(emb, query_emb);
                (sim > self.threshold).then_some((sim, out))
            })
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .map(|(_, out)| out)
    }
}
```

#### 2.2 自适应预计算

系统自动学习"高频查询模式"，主动预热缓存：

```rust
struct QueryTracker {
    freq: BTreeMap<String, u32>,  // intent → count
}

impl QueryTracker {
    fn should_precompute(&self, intent: &str) -> bool {
        self.freq.get(intent).copied().unwrap_or(0) > 50
    }
}
```

#### 2.3 Speculative Execution

用户还没输完查询，系统已基于当前输入前缀开始推断并预执行：

```
用户输入："2024年Q3..." (正在输入)
           │
           ▼
    意图推断（Top-3 候选）
           │
      并发预执行 Top-3
           │
    用户完成输入 → 精确匹配到候选 → 直接返回预执行结果
                              （节省完整 LLM 调用时间）
```

---

### 三、Agent 记忆系统

三层记忆，对应不同时间跨度：

| 记忆类型 | 存储位置 | 时间跨度 | 用途 |
|---------|---------|---------|------|
| Short-term | 会话上下文 (in-memory) | 当前对话 | 上下文连贯 |
| Long-term | Ontology（AgentMemory 对象） | 跨会话持久 | 用户偏好、过往结论 |
| Episodic | Ontology（AgentTrace 对象） | 永久归档 | 审计、复盘、few-shot |

**AgentMemory 作为 Ontology 一等对象：**

```toml
# AgentMemory mapping
version = "v1"
entity = "AgentMemory"
[from]
ns = "agent.memory"
[id]
field = "id"
[map]
user_id     = "user_id|str"
intent      = "intent|str"
summary     = "summary|str"
confidence  = "confidence|float"
created_at  = "created_at|str"
```

---

### 四、可靠性与安全

#### 4.1 Prompt Injection 检测

```
用户输入
    │
    ▼
InputGuard
    ├── 检测 "忽略之前指令"、"你现在是..." 等模式
    ├── 检测异常长度、特殊字符
    └── 通过 → 进入 Agent Pipeline
        拒绝 → 返回安全提示
```

#### 4.2 幻觉检测（Hallucination Detection）

以 Ontology 为 Ground Truth，逐条校验 LLM 输出：

```rust
fn detect_hallucination(
    output: &str,
    ontology: &dyn OntologyReader,
) -> Vec<HallucinationFlag> {
    extract_claims(output)
        .into_iter()
        .filter_map(|claim| {
            let evidence = ontology.lookup(&claim.subject, &claim.predicate);
            evidence.is_none().then_some(HallucinationFlag {
                claim,
                reason: "No supporting Ontology fact found".into(),
            })
        })
        .collect()
}
```

#### 4.3 Circuit Breaker（四级降级）

```
正常路径：LLM 全量推理

Level 1：LLM 超时 → Retry（最多 2 次）
Level 2：仍失败 → 小模型（本地轻量模型）
Level 3：小模型不可用 → 从缓存取最近一次结果（标注"可能过时"）
Level 4：无缓存 → 规则引擎（基于 Ontology Function 直接计算）
```

**Rust 草图：**

```rust
async fn resilient_query(q: &Query, agent: &Agent, cache: &Cache) -> Response {
    for attempt in 0..2 {
        if let Ok(r) = timeout(Duration::from_secs(5), agent.run(q)).await {
            return r;
        }
    }
    if let Some(cached) = cache.get_stale(q) {
        return cached.with_disclaimer("数据可能有延迟");
    }
    rule_engine_fallback(q).await
}
```

---

### 五、持续学习

#### 5.1 人工反馈闭环

用户对 Agent 输出的每次修正，都写回 Ontology：

```
用户拒绝/修正 Agent 建议
    │
    ▼
FeedbackEvent → OntologyManager
    │
    ├── 写入 Ontology（UserFeedback 对象）
    └── 触发 Few-shot 更新（下次查询注入示例）
```

**UserFeedback 作为 Ontology 对象：**

```
entity: UserFeedback
attrs:
  user_id, query, agent_output, corrected_output,
  rating (-1/0/+1), timestamp
```

#### 5.2 Few-shot 动态注入

每次 Agent 推理前，从历史中检索相似案例：

```rust
async fn build_prompt_with_fewshot(
    intent: &str,
    memory: &dyn OntologyReader,
) -> String {
    let examples = memory
        .query_similar_feedback(intent, limit = 3)
        .await;
    format!(
        "以下是类似问题的正确处理示例：\n{}\n\n当前问题：{}",
        examples.join("\n---\n"),
        intent
    )
}
```

#### 5.3 推理链路追踪（Reasoning Trace）

每次 Agent 执行生成完整追踪，存入 Ontology：

```rust
struct AgentTrace {
    trace_id:     Uuid,
    user_id:      String,
    intent:       String,
    plan:         Vec<String>,         // LLM 生成的步骤
    function_calls: Vec<FunctionCall>, // 实际执行的 Function
    raw_output:   String,
    final_output: String,
    hallucination_flags: Vec<HallucinationFlag>,
    confidence:   f32,
    latency_ms:   u64,
    timestamp:    OffsetDateTime,
}
```

---

### 六、可观测性

质量指标看板（实时监控）：

| 指标 | 说明 | 目标 |
|------|------|------|
| 接受率 | 用户未修改直接采纳 | ≥ 80% |
| 幻觉率 | 被 Ontology 核查标记 | ≤ 5% |
| P50 延迟 | 50% 查询响应时间 | < 1s |
| P99 延迟 | 99% 查询响应时间 | < 5s |
| 缓存命中率 | 语义缓存命中 | ≥ 40% |
| Critic 触发率 | 需要 Self-Reflection 比例 | < 20% |

---

### 优先级矩阵

| 优先级 | 功能 | 理由 |
|--------|------|------|
| P0 | 幻觉检测 + Grounding | 数据产品最低可信度要求 |
| P0 | Circuit Breaker | 生产可靠性基线 |
| P0 | AgentTrace 存储 | 审计合规必须 |
| P1 | 语义缓存 | 直接降低 P50 延迟 |
| P1 | Multi-Agent 路由 | 复杂查询质量提升 |
| P1 | 人工反馈闭环 | 持续改进飞轮 |
| P2 | Self-Reflection | 进一步提升质量 |
| P2 | Speculative Execution | 极致体验优化 |
| P2 | 自适应预计算 | 精细化性能 |

