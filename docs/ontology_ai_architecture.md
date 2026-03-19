# Ontology × AI Agent — 架构思考

## 传统方案 vs Palantir 对比分析

### 传统方案的主要形态

| 方案 | 代表产品 | 核心思路 |
|------|----------|----------|
| ETL 管道 | Informatica, Airbyte | 抽取 → 转换 → 加载到中央库 |
| 数据仓库 | Snowflake, Redshift, BigQuery | SQL 聚合分析，结构化存储 |
| 数据湖 | S3 + Spark, Delta Lake | 原始数据存储，按需查询 |
| 点对点集成 | REST API, MQ | 系统间直接打通 |
| MDM 主数据管理 | SAP MDG, Informatica MDM | 单一实体 master record |

---

### 核心维度对比

| 维度 | 传统方案 | Palantir Ontology | 优势归属 |
|------|----------|-------------------|----------|
| **数据集成** | N² 条 ETL 管道，每对系统独立开发 | 统一语义层，所有系统映射到 Object/Link | ✅ Palantir |
| **安全控制** | 每系统独立 auth，边界之间存在盲区 | Object 级 RBAC，统一审计，零信任模型 | ✅ Palantir |
| **数据治理** | 手动维护血缘，合规报告耗时 | 自动 Lineage，Action 审计链完整 | ✅ Palantir |
| **Schema 演化** | 字段变更级联破坏下游 ETL | 版本化 Object Type，向后兼容 | ✅ Palantir |
| **时效性** | T+1 批处理为主，实时管道成本高 | 实时 Object 更新，Action 即时生效 | ✅ Palantir |
| **AI 集成** | 每个系统单独开发 AI 接口 | 原生 AIP，Ontology 直接作为 Agent 上下文 | ✅ Palantir |
| **跨系统查询** | 多系统 join 需要数据搬移 | 图遍历直接跨 Object Type 查询 | ✅ Palantir |
| **建设成本** | 初期低，长期随系统数量爆炸式增长 | 初期较高（平台建设），长期边际成本低 | ⚖ 取决于规模 |
| **技术门槛** | SQL/Python 即可上手 | 需要理解 Ontology 概念和 SDK | ❌ 传统方案 |
| **私有部署** | 各组件独立部署，灵活 | 平台整体部署，相对复杂 | ❌ 传统方案 |
| **开源生态** | Spark/Flink/dbt 生态成熟 | 核心闭源，社区有限 | ❌ 传统方案 |

---

### 安全维度深度对比

传统方案的安全痛点：
- **多系统多套 auth**：HR 用 LDAP，Finance 用 OAuth，Sales 用 API Key，安全策略无法统一
- **数据搬移即风险**：ETL 过程中数据落盘、中间件传输，每个节点都是攻击面
- **审计链断裂**：谁在何时访问了哪条数据？跨系统无法回答
- **最小权限难实施**：列级/行级权限在多系统中几乎无法一致管理
- **合规报告手工拼**：GDPR/SOC2 审计需要从多系统人工汇总日志

Palantir 的安全架构优势：
- **Object 级权限**：对具体 Object Instance 授权，而不是表级/库级
- **统一审计日志**：所有 Action（写操作）有完整的 who/when/what 记录
- **数据不动，查询动**：不搬移数据，在原地查询，减少数据暴露面
- **Action 作为安全门**：所有写操作必须经过定义好的 Action，无法绕过
- **Lineage 即合规证据**：数据血缘自动生成，合规审计直接导出

### 成本维度深度对比

传统方案的隐性成本：
- **ETL 维护税**：每次业务系统字段变更，下游所有 ETL 管道需人工修复
- **数据重复存储**：同一份数据在 DW、DM、报表层各存一份，存储成本 3-5x
- **数据工程人力**：大型企业数据工程团队 20-50 人，年均人力成本 $3-5M
- **工具授权费用**：Informatica + Snowflake + Tableau + ... 叠加授权费高昂
- **时效 SLA 成本**：实时管道 vs 批处理，实时成本是批处理的 5-10x

Palantir 的成本优势：
- **管道数量从 N² 降到 N**：每个系统只需一个 Adapter 接入 Ontology
- **消除中间层数据存储**：减少数据冗余，存储成本下降
- **AI 降低分析人力**：NL 查询代替人工写 SQL，分析效率 10x
- **合规成本内化**：审计/合规能力平台内置，无需额外工具采购

---

## 核心理解

Palantir Ontology 本质上是一个**语义知识图谱**，AI Agent 通过理解这个图谱的 schema 语义，将自然语言意图转化为结构化的 Function 调用或 Action 执行。

---

## 架构分层

### Layer 0 — NL Interface（自然语言层）

- 用户以自然语言描述查询意图或操作指令
- AI Agent 加载 Ontology Schema（Object Types、Link Types、Properties）作为上下文
- LLM 进行语义解析，将 NL 映射为结构化意图，选择合适的 Tool

### Layer 1 — Agent Tools（工具层）

| Tool | 类型 | 说明 |
|------|------|------|
| **Function** | Query Tool | 只读计算逻辑，对 Object Set 执行过滤、聚合、图遍历 |
| **Action** | Write Tool | 标准化写操作：Create / Edit / Delete Object，附带权限控制与审计 |
| **Graph Query** | Traversal Tool | 多跳图遍历，沿 Link 关系跨 Object Type 执行复杂关联查询 |
| **Aggregation** | Compute Tool | 大规模数据统计计算，返回数值型摘要 |

### Layer 2 — Ontology Core（本体核心层）

- **Object Type**：实体定义，类型名 + Properties + Primary Key，对应业务概念（员工、项目、资产…）
- **Link Type**：关系定义，两个 Object Type 之间的有向/无向边，构成语义图结构
- **Object Instance**：运行时数据，具体实体记录与关联关系，构成完整知识图谱
- **Permission Model**：Action 级权限控制，记录谁在何时对哪个 Object 做了什么操作

### Layer 3 — Data & Side Effects（数据与副作用层）

- **下游 Pipeline**：Action 修改 Object 后自动触发 Foundry Transform，生成衍生数据集
- **价值数据产出**：业务指标、报告、预测结果等高价值衍生数据，反哺 Ontology Object
- **外部系统写回**：Action Webhook / API 回调，将操作结果同步到 ERP、CRM 等外部系统
- **Audit Trail**：完整操作日志，支持合规审计与变更溯源

---

## Function vs Action 对比

| 维度 | Function | Action |
|------|----------|--------|
| 操作类型 | 只读 (Read) | 写入 (Write) |
| 输出 | 结果集 / 数值 | Object 变更 |
| 副作用 | 无 | 触发 Pipeline |
| 权限要求 | Read 权限 | Action 权限 |
| AI Agent 角色 | Query Tool | Write Tool |

---

## 完整调用链

```
自然语言输入
    │
    ▼  Semantic Parsing（AI 理解 Ontology schema 语义）
结构化意图
    │
    ▼  Tool Selection（Function / Action / Graph Query）
工具调用
    │
    ├─ Query → ObjectSet Filter + Link Traversal → 结果集
    │
    └─ Action → Object Create/Edit/Delete → 触发 Pipeline → 价值数据
    │
    ▼  NL Response（AI 将结果转化为自然语言回答）
用户答案
```

---

## 图查询与语义的关系

Ontology 天然是图结构（节点 = Object，边 = Link），复杂查询需要**多跳图遍历**：

```
示例：找出所有参与过"项目 X 相关资产"的员工

Employee --[worksOn]--> Project --[usesAsset]--> Asset
```

AI 能理解这条路径，因为 Object Type 和 Link Type 都有**语义标注**，LLM 可以将 NL 问题映射到正确的遍历路径，而不需要用户了解底层图查询语法。

---

## 关键洞察

1. **Ontology 是语义图，不只是数据模型** — Object/Link 的语义标注使 AI 能够"理解"业务概念
2. **Action 的价值不只是产生数据** — 核心在于业务操作的标准化：权限 + 审计 + 下游联动
3. **AI 的作用是语义桥梁** — 将 NL 意图 → 图遍历路径 → Function/Action 调用
4. **图查询是大数据查询的必然形态** — 当数据关系复杂时，关系数据库无法有效表达多跳查询

---

## 待深入探索

- [ ] Ontology Function 的 TypeScript/Python SDK 实现细节
- [ ] AI Agent 如何动态发现可用的 Function/Action（Tool Discovery）
- [ ] 图查询的性能边界：何时应该退回到预聚合数据集
- [ ] Action 的事务性保证与失败回滚机制

---

## 技术储备：图查询 + Clustering Rust Crate 方案

> 以下为未来开工时的选型参考，按使用场景分类。

### 场景一：图内聚类（Graph Clustering）

在 Ontology 图拓扑中找社区/分组，例如"哪些 Object 之间关联最紧密"。

#### `petgraph` — 图结构与拓扑算法
```toml
petgraph = { version = "0.6", features = ["graphmap"] }
```
内置算法直接对应 Ontology Object/Link 结构：

| 算法 | API | 适用场景 |
|------|-----|----------|
| 连通分量 | `connected_components` | 找孤立子图 / 分组 |
| 强连通分量 | `tarjan_scc` / `kosaraju_scc` | 有向图循环依赖检测 |
| 最短路径 | `dijkstra` / `astar` | Object 间关系距离 |
| 最小生成树 | `min_spanning_tree` | 稀疏化图结构 |
| 拓扑排序 | `toposort` | Action 依赖顺序 |

```rust
use petgraph::graph::Graph;
use petgraph::algo::{connected_components, tarjan_scc};

// Ontology 图：节点 = ObjectId，边 = LinkType
let mut g: Graph<ObjectId, LinkType> = Graph::new();
let emp = g.add_node(obj_employee);
let proj = g.add_node(obj_project);
g.add_edge(emp, proj, LinkType::WorksOn);

let clusters = tarjan_scc(&g); // 返回强连通分量列表
```

#### `linfa` + `linfa-clustering` — 基于属性的 ML 聚类
```toml
linfa = "0.7"
linfa-clustering = "0.7"   # DBSCAN, K-Means, OPTICS
ndarray = "0.15"
```
适合将 Object Properties（数值特征）向量化后聚类，而非纯图拓扑聚类：

```rust
use linfa::prelude::*;
use linfa_clustering::Dbscan;

// 将 Object 属性转成 ndarray，按特征相似度聚类
let dataset = Dataset::from(feature_matrix);
let clusters = Dbscan::params(3)
    .tolerance(0.5)
    .fit(&dataset)?;
```

#### 社区发现（Community Detection / Louvain）
Rust 目前无成熟 Louvain 实现，备选方案：
- **小图**：基于 `petgraph` 手写模块度优化
- **大图**：通过 `pyo3` 调用 Python `networkx` / `igraph`
- **离线预处理**：Python 跑 Louvain，结果存回 Ontology Object 的属性字段

---

### 场景二：分布式集群（Distributed Cluster）上的图查询

Ontology 数据量超出单机内存时，需要分布式执行。

#### `datafusion` — 分布式 SQL / Arrow 查询引擎
```toml
datafusion = "37"
```
- 把 Object Instance 存为 Parquet 文件
- 用 SQL 表达 join 遍历（有限跳数）
- 支持自定义 UDF，可嵌入图遍历逻辑

```rust
use datafusion::prelude::*;

let ctx = SessionContext::new();
ctx.register_parquet("employees", "data/employees.parquet", Default::default()).await?;
ctx.register_parquet("projects",  "data/projects.parquet",  Default::default()).await?;

// 用 SQL 模拟一跳 Link 遍历
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
DataFusion 的集群版，调度多节点并行执行，适合超大规模 Ontology 数据集。

---

### 场景三：语义图查询（RDF / SPARQL）

Ontology 的 Object/Link 语义与 RDF 的 Subject/Predicate/Object 天然对应。

#### `oxigraph` — 嵌入式 RDF 图数据库
```toml
oxigraph = "0.3"
```
- 支持 SPARQL 1.1 查询语言
- 单机嵌入式，零外部依赖
- 可直接将 Ontology schema 映射为 RDF triple

```rust
use oxigraph::store::Store;
use oxigraph::sparql::QueryResults;

let store = Store::new()?;
// 插入 triple: Employee --worksOn--> Project
store.insert(&triple!(
    NamedNode::new("ex:emp_001")?,
    NamedNode::new("ex:worksOn")?,
    NamedNode::new("ex:proj_X")?
))?;

// SPARQL 图查询
if let QueryResults::Solutions(solutions) = store.query(
    "SELECT ?emp WHERE { ?emp ex:worksOn ex:proj_X }"
)? { ... }
```

#### `neo4rs` — Neo4j Rust 异步驱动
```toml
neo4rs = "0.7"
```
如果将 Ontology 图存入 Neo4j，可使用 Cypher 查询，并调用 **Neo4j GDS 插件**的内置算法：
- Louvain 社区发现
- PageRank / Betweenness Centrality
- 最短路径 / K 近邻

```rust
use neo4rs::*;

let graph = Graph::new("bolt://localhost:7687", "neo4j", "password").await?;

// Cypher 多跳遍历
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
| 复杂图算法（PageRank等）| 大图 | `neo4rs` + Neo4j GDS | 需外部 Neo4j 服务 |

---

### 与 Ontology 架构的集成思路

```
Ontology Core (Object + Link)
        │
        ├─ 小图/实时遍历  ──→  petgraph (内存图)
        │
        ├─ 语义查询       ──→  oxigraph (RDF/SPARQL)
        │
        ├─ 大数据聚合      ──→  datafusion (Parquet + SQL)
        │
        ├─ 复杂图算法      ──→  neo4rs + Neo4j GDS
        │
        └─ AI Agent 调用  ──→  上述任意一层封装为 Function Tool
```

核心原则：**查询层对 AI Agent 透明**，Agent 只看到 Function 接口，底层切换图引擎不影响上层 NL 调用链。

---

## 海量 Ontology 存储管理：面向中小/大企业的技术方案

> 背景：作为 SaaS 平台向企业开放 Ontology 能力时，面临多租户隔离、海量对象存储、Schema 演化、查询性能四大核心挑战。

---

### 挑战分析

| 挑战 | 中小企业 | 大企业 |
|------|----------|--------|
| Ontology 规模 | 数千 Object / 数万 Link | 数亿 Object / 数十亿 Link |
| Schema 复杂度 | 10~50 Object Types | 数百 Object Types，跨部门 |
| 并发写入 | 低 | 极高（实时数据接入） |
| 隔离要求 | 逻辑隔离即可 | 物理隔离 / 私有部署 |
| Schema 演化 | 随意变更 | 版本管控、向后兼容 |

---

### 多租户存储隔离策略（三种模式）

#### 模式 A — Shared Schema（共享表 + tenant_id）
```
objects 表: id | tenant_id | type_id | properties(JSONB) | created_at
links   表: id | tenant_id | src_id  | dst_id | link_type | props(JSONB)
```
- **优点**：运维简单，资源利用率高，适合中小企业
- **缺点**：大租户可能影响邻居（noisy neighbor），行级安全策略复杂
- **适用**：SME SaaS，租户数量多但单租户数据量小

#### 模式 B — Schema Per Tenant（独立 Schema/Namespace）
```
tenant_abc.objects | tenant_abc.links | tenant_abc.object_types
tenant_xyz.objects | tenant_xyz.links | tenant_xyz.object_types
```
- **优点**：索引独立，查询无 tenant_id 过滤开销，Schema 可独立演化
- **缺点**：租户数量多时 DDL 管理复杂
- **适用**：中大型企业，租户数 < 1000

#### 模式 C — Database Per Tenant（物理隔离）
```
tenant_abc_db (独立 PostgreSQL / Neo4j 实例)
tenant_xyz_db (独立 PostgreSQL / Neo4j 实例)
```
- **优点**：完全隔离，支持私有部署，满足数据主权要求
- **缺点**：运维成本最高，资源浪费
- **适用**：金融、医疗、政府等强合规大企业

---

### 存储引擎选型

#### 方案 1：PostgreSQL + Apache AGE（推荐起步方案）
```toml
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio"] }
```
- PostgreSQL 本身用 JSONB 存 Object Properties，递归 CTE 做图遍历
- Apache AGE 扩展为 PostgreSQL 添加 Cypher 查询能力
- 运维成熟，支持 Schema Per Tenant，RLS 做行级安全

```sql
-- 递归 CTE 多跳遍历（纯 PostgreSQL）
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
- 原生支持图关系（`->` 语法），无需额外扩展
- 内置多租户 Namespace / Database 二级隔离
- 支持 SQL-like 查询语法，学习成本低
- 纯 Rust 实现，嵌入式或独立服务均可

```rust
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Ws;

let db = Surreal::new::<Ws>("127.0.0.1:8000").await?;
db.use_ns("tenant_abc").use_db("ontology").await?;

// 图遍历：Employee -> Project -> Asset
let result: Vec<Asset> = db.query(
    "SELECT ->works_on->project->uses_asset->asset.* FROM employee:emp_001"
).await?.take(0)?;
```

#### 方案 3：ScyllaDB / Cassandra — 超高并发写入
```toml
scylla = "0.12"   # ScyllaDB Rust 驱动
```
- 邻接表模型存储 Object 和 Link（宽列存储）
- 适合实时数据流高频写入场景（IoT、日志、交易流水）
- 读取多跳需要应用层拼接，不适合深度遍历
- 结合 `datafusion` 做离线分析

```
// 邻接表存储模型
partition key: (tenant_id, src_object_id)
clustering key: link_type, dst_object_id
```

#### 方案 4：TiKV + 自定义图层 — 超大规模分布式
```toml
tikv-client = "0.3"
```
- 分布式 KV，水平无限扩展
- 需要自己在上层实现图语义（adjacency list 编码）
- 适合自研图数据库内核，门槛最高
- PingCAP 的 TiDB 底层即用此方案

---

### Schema 管理：Object Type 版本控制

企业场景下 Ontology Schema 会频繁演化，需要版本管控：

#### Schema Registry 设计
```
object_type_schemas 表:
  id         | UUID
  tenant_id  | UUID
  type_name  | TEXT          -- "Employee"
  version    | INT           -- 递增版本号
  schema     | JSONB         -- { properties: [...], required: [...] }
  status     | ENUM          -- draft | active | deprecated
  created_at | TIMESTAMPTZ
```

#### 版本演化规则
```
兼容变更（无需迁移）：
  + 新增可选 Property
  + 新增 Link Type

破坏性变更（需迁移）：
  - 删除 Property
  - 重命名 Property
  - 修改 Property 类型
```

相关 Rust crate：
```toml
apache-avro = "0.16"   # Schema 序列化，支持 schema evolution
jsonschema  = "0.17"   # JSON Schema 验证
```

---

### 分层存储架构（冷热分离）

```
┌─────────────────────────────────────────────────┐
│  Hot Layer  (最近 7 天 / 高频访问)                │
│  Redis / DragonflyDB                            │
│  - Object 缓存，Link 邻接表缓存                   │
│  - TTL 自动淘汰                                  │
├─────────────────────────────────────────────────┤
│  Warm Layer  (运营数据 / 图查询)                  │
│  PostgreSQL+AGE 或 SurrealDB                    │
│  - Object Instance + Link                       │
│  - Schema Registry                              │
│  - 支持实时图遍历                                 │
├─────────────────────────────────────────────────┤
│  Cold Layer  (历史 / 分析 / 审计)                 │
│  Parquet on S3/MinIO + DataFusion               │
│  - 按 tenant_id + date 分区                     │
│  - Action Audit Log                             │
│  - 离线聚合分析                                  │
└─────────────────────────────────────────────────┘
```

相关 Rust crate：
```toml
fred        = "8"     # Redis 异步客户端（比 redis crate 更现代）
object_store = "0.9"  # S3/MinIO/GCS 统一抽象
parquet     = "51"    # Apache Parquet 读写（arrow 生态）
```

---

### 向量索引：语义搜索支持

Ontology 对象的 AI 语义查询需要向量相似度搜索：

```toml
qdrant-client = "1"    # Qdrant 向量数据库客户端
```

```
存储模型：
  每个 Object → 嵌入向量（LLM 对 Properties 做 embedding）
  向量索引：按 tenant_id 隔离 collection
  查询：NL 问题 → embedding → ANN 搜索 → 候选 Object Set → 精确图遍历
```

这是**语义查询**的核心链路：向量粗筛 + 图精筛。

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
5. **向量 + 图双引擎** — 语义粗筛（Qdrant）+ 关系精筛（图遍历），AI 查询标配架构
