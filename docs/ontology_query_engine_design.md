# Ontology 查询引擎设计：DataFusion + Arrow + NebulaGraph

> 版本：2026-03-27
> 来源：内存计算层架构讨论
> 目标：系统性比较查询引擎方案，对标 Palantir 官方实现

---

## 一、问题定义

Ontology 平台的查询需求分为两类，现有 SQLite 单点架构无法高效支撑：

| 查询类型 | 示例 | 瓶颈 |
|---------|------|------|
| **字段条件查询**（Object Set） | 找出早上 8 点前下单的所有订单 | 全表扫描，磁盘 IO |
| **图遍历查询**（关联查询） | 找出关联高风险供应商的未完成合同 | 多跳 JOIN，SQL 表达力不足 |
| **复合查询**（两者结合） | 早上 8 点前下单 + 关联风险供应商 | 无高效执行路径 |

---

## 二、提出方案：DataFusion + Arrow + NebulaGraph

### 2.1 架构总览

```
┌─────────────────────────────────────────────────────────┐
│                    应用层（Rust API）                     │
└──────────┬───────────────────────┬──────────────────────┘
           │ 字段条件查询           │ 图遍历查询
           ▼                       ▼
┌──────────────────┐   ┌──────────────────────────────────┐
│  DataFusion      │   │         NebulaGraph               │
│  （列式 SQL 引擎）│   │     （分布式图数据库）             │
│                  │   │                                   │
│  Arrow 内存表    │   │  Vertex: OntologyObject           │
│  ─────────────  │   │  Edge:   OntologyLink             │
│  order / invoice │   │  nGQL 多跳遍历                    │
│  supplier / ...  │   │                                   │
└────────┬─────────┘   └──────────────┬────────────────────┘
         │ Arrow 候选 ID 集合          │
         └──────────┬─────────────────┘
                    │ 复合查询：先收窄，再遍历
                    ▼
         ┌──────────────────┐
         │   查询结果合并    │
         └──────────────────┘
                    │ 写入路径（双写）
                    ▼
         ┌──────────────────┐
         │  SQLite / PgSQL  │
         │  （持久化存储）   │
         └──────────────────┘
```

### 2.2 各组件职责

#### Apache Arrow（内存数据格式）

- Ontology Object 的**通用内存表示格式**，列式存储
- 作为 DataFusion 和 NebulaGraph 之间的**数据交换协议**（Arrow Flight）
- 零拷贝传输，无序列化开销

```rust
// Object 加载到 Arrow RecordBatch
let schema = Schema::new(vec![
    Field::new("id",         DataType::Utf8,    false),
    Field::new("label",      DataType::Utf8,    false),
    Field::new("status",     DataType::Utf8,    true),
    Field::new("created_at", DataType::Utf8,    true),
    Field::new("amount",     DataType::Float64, true),
]);
```

#### DataFusion（字段条件查询引擎）

- 在 Arrow 内存表上执行 SQL，毫秒级响应
- 负责 **Object Set** 的字段过滤：单实体类型内的条件扫描
- 输出候选对象 ID 集合，传递给 NebulaGraph

```sql
-- Object Set：早上 8 点前下单
SELECT id FROM orders
WHERE EXTRACT(HOUR FROM CAST(created_at AS TIMESTAMP)) < 8
  AND status = 'pending';

-- Object Set：逾期未付发票
SELECT id FROM invoices
WHERE status = 'pending'
  AND due_date < CURRENT_DATE;
```

#### NebulaGraph（图遍历引擎）

- 负责**跨实体类型的关联查询**，沿 OntologyLink 边遍历
- 接收 DataFusion 输出的候选 ID 集合作为起点，避免全图扫描
- nGQL 语法表达多跳遍历

```ngql
-- 从候选订单出发，找关联的高风险供应商
GO FROM $candidates OVER supplies
WHERE $$.Supplier.risk_score > 80
YIELD $^.Order.order_no, $$.Supplier.name, $$.Supplier.risk_score;

-- 3 跳内找到所有关联实体
GO 1 TO 3 STEPS FROM "order-001" OVER * YIELD dst(edge);
```

### 2.3 复合查询执行流程（核心优势）

以"早上 8 点前下单且关联高风险供应商"为例：

```
总对象数：1,000,000 张订单

Step 1 — DataFusion（字段过滤，列式扫描）：
  SELECT id FROM orders WHERE hour(created_at) < 8
  → 候选集合：~50,000 张订单（毫秒级）
  → 输出：Arrow RecordBatch（ID 列）

Step 2 — NebulaGraph（图遍历，从候选集合出发）：
  GO FROM [50,000 IDs] OVER supplies
  WHERE supplier.risk_score > 80
  → 最终结果：~200 张订单
  → NebulaGraph 只需遍历 50,000 个起点，而非 1,000,000 个

性能对比：
  纯 SQL JOIN：  全表扫描 × 多跳 JOIN，O(n²) 复杂度
  纯 NebulaGraph：从全量 1M 对象出发，浪费大量遍历
  组合方案：     先收窄 20x，NebulaGraph 工作量减少 20x
```

**这是两个系统组合后涌现的能力，单用任何一个都做不到。**

---

## 三、备选方案对比

### 方案 B：PostgreSQL + Apache AGE（图扩展）

```sql
-- AGE 在 PostgreSQL 上提供 Cypher 查询
SELECT * FROM cypher('ontology_graph', $$
  MATCH (o:Order)-[:SUPPLIES]->(s:Supplier)
  WHERE o.created_at < '08:00' AND s.risk_score > 80
  RETURN o, s
$$) AS (order agtype, supplier agtype);
```

| | 优势 | 劣势 |
|--|------|------|
| **PostgreSQL + AGE** | 单一系统，运维简单 | 图查询性能远不如原生图 DB；无列式加速；AGE 社区活跃度一般 |

### 方案 C：Neo4j（图数据库）

| | 优势 | 劣势 |
|--|------|------|
| **Neo4j** | Cypher 表达力强；生态成熟 | Java 生态，Rust 集成复杂；无内置列式字段扫描；企业版昂贵 |

### 方案 D：TigerGraph

| | 优势 | 劣势 |
|--|------|------|
| **TigerGraph** | 图分析性能极强 | 商业闭源，授权费用高；无 Rust 原生客户端 |

### 方案 E：纯 Rust 自建（DashMap + petgraph）

```rust
// petgraph 做图遍历
let graph: Graph<ObjectId, LinkType> = Graph::new();
let result = algo::dijkstra(&graph, start, None, |_| 1);
```

| | 优势 | 劣势 |
|--|------|------|
| **DashMap + petgraph** | 零外部依赖；完全控制 | 需自建查询语言；无分布式支持；重复造轮子 |

### 方案对比总表

| | DataFusion + NebulaGraph | PostgreSQL + AGE | Neo4j | 自建 Rust |
|--|--------------------------|-----------------|-------|----------|
| 字段条件查询（Object Set） | ✅ 列式，毫秒级 | ⚠️ 行式，慢 | ❌ 不擅长 | ❌ 需自建 |
| 图遍历（多跳关联） | ✅ nGQL 原生 | ⚠️ Cypher 慢 | ✅ Cypher | ⚠️ petgraph |
| 复合查询（先过滤后遍历） | ✅ 组合优化 | ❌ 无 | ❌ 无 | ❌ 无 |
| Rust 集成 | ✅ DataFusion 原生 Rust | ✅ sqlx | ⚠️ 非官方 | ✅ |
| 水平扩展 | ✅ 独立扩展各层 | ⚠️ PG 扩展有限 | ⚠️ 企业版 | ❌ |
| 运维复杂度 | 中（两个系统） | 低（单系统） | 中 | 低 |
| 开源 / 成本 | ✅ Apache 2.0 | ✅ | ⚠️ 企业版收费 | ✅ |
| 生产验证 | ✅ DataFusion 高；Nebula 中 | ⚠️ AGE 较新 | ✅ 高 | — |

---

## 四、对标 Palantir 官方方案

### 4.1 Palantir 的技术栈（已公开部分）

Palantir Foundry 的 Ontology 查询体系由以下几层构成：

#### Phonograph（内存对象存储，核心专利）

- Palantir 自研的**内存对象存储引擎**
- 将所有 Ontology Object 实例保存在内存中，支持毫秒级随机访问
- 自定义索引结构，支持按字段值快速过滤（等价于我们的 DataFusion 层）
- Write-through 到底层数据湖（Parquet 格式，列式存储）

#### Foundry 数据湖（底层持久化）

- 数据以 **Parquet（列式格式）** 存储，与 Arrow 同源（Arrow 是 Parquet 的内存版本）
- 支持 Spark 批量计算和增量更新
- 这一层等价于我们的 SQLite → PostgreSQL 持久化层

#### 专有图引擎

- 维护 Ontology Object 之间的 LinkType 图，保存在内存中
- 支持多跳遍历，用于 Object 关联查询
- 等价于我们的 NebulaGraph 层，但完全自研

#### AIP（AI Platform）查询入口

- LLM 将自然语言翻译成 Ontology 查询
- 查询结果以类型化 Object 返回，不是原始 SQL 结果
- 等价于我们规划的 Phase 4 Rust-native AI Agent 层（见第七章）

### 4.2 架构对比图

```
Palantir Foundry                    本系统（规划）
─────────────────────────────────   ──────────────────────────────
自然语言查询                         自然语言查询
    │ AIP (LLM)                          │ rig + swarm-rs（Rust Agent）
    ▼                                    ▼  本地 LLM / 国内 API
Phonograph（内存对象存储）           DataFusion + Arrow（内存列式）
    │ 字段索引                            │ Object Set SQL 查询
    ▼                                    ▼
专有图引擎（内存图遍历）             NebulaGraph（分布式图遍历）
    │ LinkType 多跳                       │ nGQL 多跳
    ▼                                    ▼
Foundry 数据湖（Parquet）            SQLite → PostgreSQL（持久化）
```

### 4.3 核心差异分析

| 维度 | Palantir | 本系统 |
|------|---------|-------|
| **内存对象存储** | Phonograph（自研，专利保护） | DataFusion + Arrow（Apache 开源） |
| **存储格式** | Parquet（列式） | SQLite → Parquet/Arrow（演进中） |
| **图引擎** | 自研专有 | NebulaGraph（开源） |
| **查询语言** | 专有 API + AIP 自然语言 | SQL（DataFusion）+ nGQL（Nebula）+ 自然语言（rig Agent） |
| **层间集成** | 紧密（同一团队自研） | 松散（Arrow Flight 协议连接） |
| **工程投入** | 数百人年 | 组合开源方案，数人月可完成基础版 |
| **扩展性** | 水平扩展，但闭源 | 各层独立扩展，完全开源 |
| **成本** | 极高（Palantir 平台授权） | 开源，基础设施成本 |

### 4.4 我们的差异化优势

**1. Arrow 作为通用数据格式**

Palantir 在各层之间使用专有协议传输数据。我们使用 **Apache Arrow** 作为各层间的通用数据交换格式：DataFusion 原生输出 Arrow，NebulaGraph 支持 Arrow Flight 输入。零序列化开销，且与未来的 AI 框架（LlamaIndex、LangChain 等均支持 Arrow）天然兼容。

**2. 复合查询的"收窄-遍历"模式**

Palantir 的 Phonograph 和图引擎是紧耦合的，查询优化在内部完成，对外黑盒。我们的"DataFusion 先收窄 → NebulaGraph 再遍历"是显式的两阶段模式：

```
显式两阶段 vs 黑盒优化
优势：查询计划可观测、可调试、可优化
      AI Agent 可以理解并干预每个阶段
      中间结果（候选 ID 集合）本身有业务价值（Object Set）
```

**3. 与 AI Agent 的深度结合**

Palantir AIP 将 LLM 作为查询翻译层，输入自然语言，输出 Ontology 查询。我们的架构更进一步：

```
Palantir AIP：自然语言 → 查询 → 结果
本系统规划：  自然语言 → Object Set（DataFusion）→ 图遍历（NebulaGraph）→ 结果
                                    ↑
                          中间产物可保存、可命名、可订阅
                          AI 的推理过程可见、可审计
```

---

## 五、实施路径

### Phase 3（当前）：夯实基础

```
目标：SQLite 加索引，支撑数十万对象
工作：
  - 为 ontology_objects.fields（JSON）的热点字段添加 generated column + 索引
  - Schema 层全量 DashMap 缓存（EntityType / ActionType / StateDef）
  - 不引入新基础设施
```

### Phase 4：引入查询加速层

```
目标：支持 Object Set 字段查询 + 图遍历
工作：
  Step 1 — DataFusion 集成：
    - 启动时将热点 EntityType 的 Object 加载到 Arrow RecordBatch
    - 写入时同步更新内存表
    - 暴露 /api/ontology/object-sets/query 接口（POST body 为过滤条件）

  Step 2 — NebulaGraph 集成：
    - 将 ontology_links 数据同步到 NebulaGraph
    - 暴露图遍历接口（从对象出发，N 跳关联查询）

  Step 3 — 复合查询接口：
    - DataFusion 输出候选 ID → Arrow Flight 传给 NebulaGraph
    - 统一查询 API，调用方无感知底层分发
```

### Phase 4 同期：Rust-native AI Agent 接入

```
目标：自然语言 → Object Set 查询（AI Agent 层次 1）
工作：
  - 部署本地 LLM（Ollama + Qwen2.5:14b 或 DeepSeek-R1:14b）
    或接入国内合规 API（DeepSeek API / DashScope）
  - 用 rig 构建 Agent，注册 QueryObjectSet / GraphTraversal / ExecuteAction 三个 Tool
  - Agent 接收用户问题 + Ontology Schema 上下文
  - 输出结构化过滤条件（JSON），调用 DataFusion 执行
  - 复杂多步查询用 swarm-rs 做多 Agent 协作（路由 → 查询 → 图遍历 → 汇总）
详见：ontology_ai_agent_design.md（第七章）
```

### Phase 5：生产化与规模扩展

```
目标：支撑千万/亿级对象
工作：
  - SQLite 迁移到 PostgreSQL（或 TiKV 分布式 KV）
  - DataFusion 集群化（多节点并行扫描）
  - NebulaGraph 集群扩容
  - 向量 embedding 存储（qdrant）支持语义相似度搜索
```

---

## 七、Rust-native AI Agent 层设计（2026-03-27）

> 不引入 Claude API 等外部服务，完全基于 Rust 生态 + 本地 LLM / 国内合规 API 构建。

### 7.1 Agent 框架选型

| 框架 | 定位 | 适用场景 | 成熟度 |
|------|------|---------|-------|
| **rig** | 单 Agent + Tool use + RAG | 主力框架，覆盖 90% 场景 | ⭐⭐⭐⭐⭐ |
| **swarm-rs** | 多 Agent Handoff（OpenAI Swarm 模式） | 复杂多步查询的 Agent 协作 | ⭐⭐⭐ |
| **langchain-rust** | Chain / Memory / Tool（LangChain 移植） | 覆盖面广，不如 Python 版完整 | ⭐⭐⭐ |
| **kalosm** | 本地优先，结构化生成强 | 严格结构化输出场景 | ⭐⭐⭐ |
| **async-openai** | OpenAI 兼容 API 客户端 | 对接任意兼容接口，极稳定 | ⭐⭐⭐⭐⭐ |

**主线选择**：rig（主力）+ swarm-rs（多 Agent 协作）+ async-openai（后端对接层）

### 7.2 本地推理引擎选型

| 引擎 | 特点 | 推荐场景 |
|------|------|---------|
| **Ollama** | 最易用，OpenAI 兼容，模型生态最丰富 | 开发和生产首选 |
| **mistral.rs** | 纯 Rust，性能最强，内置 OpenAI 兼容服务器 | 对延迟敏感场景 |
| **candle** | HuggingFace 出品，推理引擎嵌入服务内部，零外部依赖 | 推理逻辑内嵌到 Rust 服务 |
| **llama.cpp（llama-cpp-rs）** | C++ 内核 + Rust 绑定，GGUF 硬件兼容性最广 | 特殊硬件环境 |

### 7.3 国内合规 LLM API（无本地 GPU 时）

全部 OpenAI 兼容，async-openai / rig 一行切换：

```rust
// 切换后端只需修改 api_base，业务代码不变
let client = Client::with_config(
    OpenAIConfig::new()
        .with_api_base("https://api.deepseek.com/v1")  // DeepSeek
        // .with_api_base("https://dashscope.aliyuncs.com/compatible-mode/v1")  // Qwen
        // .with_api_base("https://open.bigmodel.cn/api/paas/v4")  // GLM-4
        .with_api_key(&api_key)
);
```

| 服务 | 推荐模型 | 中文能力 | 工具调用 |
|------|---------|---------|---------|
| **DeepSeek API** | DeepSeek-V3 / R1 | ⭐⭐⭐⭐⭐ | ✅ |
| **DashScope（阿里/Qwen）** | Qwen-Max | ⭐⭐⭐⭐⭐ | ✅ |
| **智谱 GLM** | GLM-4 | ⭐⭐⭐⭐ | ✅ |

### 7.4 本地模型推荐

| 模型 | 显存 | 特点 |
|------|------|------|
| **Qwen2.5:14b** | 10GB | 中文最强，工具调用稳定 |
| **DeepSeek-R1:14b** | 10GB | 推理能力强，复杂查询翻译准确 |
| **Llama3.1:8b** | 6GB | 轻量，工具调用支持好 |

### 7.5 与 Ontology 平台的多 Agent 架构

swarm-rs 的 Handoff 模式天然适合 Ontology 查询路由：

```
用户："找出早上 8 点前下单且关联高风险供应商的订单"
              │
              ▼
       Router Agent（rig）
       理解意图，拆分任务，决定 Handoff
              │
       ┌──────┴──────┐
       ▼             ▼
 Query Agent    Graph Agent
 (DataFusion)   (NebulaGraph)
 字段条件过滤    图遍历关联查询
       │             │
       └──────┬──────┘
              ▼
       Merge Agent
       合并结果，生成自然语言摘要
```

**三个核心 Tool（rig Tool use）**：

```rust
// Tool 1：字段条件查询（调用 DataFusion）
QueryObjectSet { entity_type, filters: Vec<FieldFilter> }

// Tool 2：图遍历（调用 NebulaGraph）
GraphTraversal { from_ids: Vec<String>, edge_type, hops, conditions }

// Tool 3：触发 Action（调用 ActionType 执行引擎）
ExecuteAction { action_type_id, object_id, params: HashMap<String, Value> }
```

### 7.6 RAG / 向量搜索（Ontology 语义检索）

| 组件 | 职责 |
|------|------|
| **fastembed-rs**（Qdrant 出品） | 本地生成 embedding，零外部依赖 |
| **qdrant**（Rust 客户端） | 向量存储，Ontology 对象语义检索 |
| **sqlite-vec** | SQLite 轻量向量扩展，Phase 3 过渡方案 |

### 7.7 两条落地路线

**路线 A：API 优先（快速落地，无 GPU 要求）**
```
DeepSeek API / Qwen API
    + async-openai（Rust 客户端）
    + rig（Agent / Tool use）
    + fastembed-rs + qdrant（RAG）
```

**路线 B：完全本地（数据绝不出内网，适合金融/政务客户）**
```
Ollama（Qwen2.5:14b 或 DeepSeek-R1:14b）
    + rig（Agent 框架）
    + fastembed-rs（本地 embedding）
    + qdrant（向量存储）
```

> 两条路线的 Rust 业务代码完全相同，仅 `api_base` 配置不同，可做成客户可选的部署参数。

### 7.8 为什么不需要单独训练模型（关键洞察）

这是本方案最重要的架构优势之一：

**传统 AI 系统的问题**：
```
数据是非结构化的（文档/日志/表格混杂）
→ LLM 不理解业务语义
→ 需要大量标注数据 Fine-tune，才能让模型"懂业务"
→ 训练成本高，模型需要随业务变化持续更新
```

**Ontology 平台的解法**：
```
数据已经语义化：
  EntityType = Order / Supplier / Invoice（业务概念已命名）
  ActionType = confirm_order / ship_order（业务操作已声明）
  StateDef   = pending / confirmed / shipped（业务状态已定义）
  LinkType   = Order →[SUPPLIES]→ Supplier（业务关系已建模）

→ 把 Schema 作为上下文直接注入 LLM Prompt
→ 通用模型（Qwen / DeepSeek）已经理解"订单""供应商""发货"的含义
→ Agent 只需做"意图→结构化查询"的翻译，不需要学习业务知识
```

**实际 Prompt 结构**：
```
系统上下文：
  你有以下 EntityType 可以查询：
  - Order（字段：order_no, status, created_at, total_amount）
  - Supplier（字段：name, risk_score, category）
  - Invoice（字段：invoice_no, status, amount, due_date）
  可用 Action：confirm_order / ship_order / confirm_delivery / send_reminder
  关联关系：Order -[SUPPLIES]-> Supplier

用户：哪些订单在早上 8 点前下单？
Agent 输出（结构化 JSON）：
  { "entity_type": "Order", "filters": [{ "field": "created_at", "op": "hour_lt", "value": 8 }] }
```

**结论**：
- Ontology Schema 本身就是"业务知识的结构化表达"
- 把 Schema 注入 Prompt = 给 LLM 一份实时更新的业务字典
- 业务增加新 EntityType / ActionType → Schema 自动更新 → Agent 立即可用
- **零训练成本，零标注成本，业务变化自动跟进**

这是 Ontology-first 架构对 AI 集成的最大红利。

---

## 六、总结

**完整技术栈**：

```
自然语言层：  rig + swarm-rs（Rust Agent 框架）
              本地 LLM（Ollama）或国内合规 API（DeepSeek / Qwen）
              fastembed-rs + qdrant（RAG 语义检索）
                        │
字段查询层：  DataFusion + Arrow（列式内存 SQL）
                        │ Arrow Flight
图遍历层：    NebulaGraph（分布式图数据库，nGQL）
                        │
持久化层：    SQLite → PostgreSQL → TiKV（按规模演进）
```

**核心价值**：

1. **职责分明**：列式扫描 / 图遍历 / Agent 推理，各做最擅长的事
2. **Arrow 通用格式**：层间零拷贝传输，与 AI 框架天然兼容
3. **复合查询优化**：先收窄再遍历，性能倍增，查询计划可观测
4. **全 Rust 友好**：DataFusion 原生 Rust，rig/swarm-rs Rust，NebulaGraph 官方 Rust 客户端
5. **数据不出内网**：本地 LLM 部署，满足金融/政务最严合规要求
6. **对标 Palantir**：覆盖 Phonograph + 图引擎 + AIP 核心能力，工程成本仅为其百分之一
7. **无需单独训练模型**：Ontology Schema 语义化后作为上下文直接注入 LLM，通用模型即可完成查询翻译（详见第七章 7.8）
