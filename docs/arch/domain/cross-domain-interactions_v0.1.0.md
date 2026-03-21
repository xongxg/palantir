# 跨领域交互清单与交互图

> 版本：v0.1.0 | 日期：2026-03-19
> 关联：user-stories-and-interactions_v0.1.0.md、ontology-permission-interactions_v0.1.1.md

---

## 一、领域交互总览

> 系统共有 7 个核心领域（限界上下文）。下图标出所有存在交互的边。
> 实线 = 同步（gRPC / HTTP）；虚线 = 异步（NATS Event Bus）；双箭头 = 双向。

```
                        ┌─────────────────────┐
                        │      Identity        │
                        │  User/Role/Group/OU  │
                        └──────┬──────┬────────┘
                     BELONGS   │      │ HAS_ROLE / MANAGES
                     图遍历    │      │ （图边，存 Ontology）
                               ▼      ▼
┌──────────────┐   写对象   ┌──────────────────┐   发布事件   ┌──────────────────┐
│   Ingest     │──────────▶│    Ontology       │─ ─ ─ ─ ─ ─▶│   Event Bus      │
│ DataSource   │           │ EntityType(TBox)  │   OntologyEvent  │  NATS JetStream  │
│ FieldMapping │           │ OntologyObject    │              └────────┬─────────┘
│ IngestJob    │           │ OntologyRel.      │                       │ 订阅
│ IngestCursor │           └────────┬──────────┘            ┌─────────┼─────────┐
└──────┬───────┘                    │                        ▼         ▼         ▼
       │                    ◀───────┤ authorize          Workflow   Agent    Permission
       │ embed              │ 每次读写                   (触发)   (预计算)  (缓存失效)
       ▼                    ▼
┌──────────────┐   ◀─── ┌──────────────────┐
│  Embedding   │        │   Permission      │
│  fastembed   │        │ EntityTypePerm.   │
│  BGE-small   │        │ RelationshipRule  │
└──────────────┘        │ AbacPolicy        │
       ▲                │ AccessDecision    │
       │ embed          │ AuditLog          │
       │                └──────────────────┘
┌──────┴───────┐
│    Agent     │──gRPC──▶┌──────────────────┐──HTTP──▶ External API
│ AgentSession │         │    Function       │
│ AgentMessage │◀────────│ FunctionDef.      │◀──────── Workflow
│ AgentMemory  │  结果   │ FunctionExec.     │  步骤执行
│ SemanticCache│         │ OutboundConfig    │
└──────────────┘         └──────────────────┘
       │                          │
       └──────────────────────────┘
              均通过 Ontology 读写业务数据
```

---

## 二、跨领域交互明细

### 交互矩阵

| 发起方 | 接收方 | 方向 | 通信方式 | 触发 Story | 参考 Flow |
|--------|--------|------|---------|-----------|----------|
| Identity | Ontology | → | 图存储（User 是 ABox 对象） | US-23/24 | — |
| Identity | Permission | → | 同步 gRPC（图遍历派生 EnrichedIdentity） | US-20/31 | Flow 1/2 |
| Ingest | Ontology | → | 同步 HTTP（写 OntologyObject） | US-02/03/04 | Flow 7 |
| Ingest | Embedding | → | 同步 gRPC（文件分片向量化） | US-70/71 | Flow 10 |
| Ontology | Permission | ↔ | 同步 gRPC（每次读写调用 authorize） | US-30~35 | Flow 2/3 |
| Ontology | Event Bus | → | 异步 NATS Publish（每次 mutation） | US-12/14/34 | Flow 3/6/7 |
| Event Bus | Workflow | → | 异步 NATS Subscribe（事件触发） | US-62 | Flow 8 |
| Event Bus | Agent | → | 异步 NATS Subscribe（缓存失效/预计算） | US-42 | Flow 6/7 |
| Event Bus | Permission | → | 异步 NATS Subscribe（权限缓存失效） | US-34 | Flow 6 |
| Agent | Embedding | → | 同步 gRPC（query 向量化，语义缓存） | US-42 | Flow 5/9 |
| Agent | Function | → | 同步 gRPC（工具调用） | US-51 | Flow 9 |
| Agent | Ontology | → | 同步 HTTP（携带用户身份读数据） | US-40/43 | Flow 5 |
| Workflow | Function | → | 同步 gRPC（步骤执行） | US-60/63 | Flow 8 |
| Workflow | Ontology | → | 同步 HTTP（步骤结果写回） | US-60 | Flow 8 |
| Function | Ontology | → | 同步 HTTP（CEL 查询/写入 Ontology） | US-50 | Flow 9 |
| Function | External | → | 同步 HTTP（outbound 集成） | US-52 | Flow 8/9 |
| Permission | Identity | → | 同步 gRPC（查 EnrichedIdentity） | US-31/34 | Flow 2 |

---

## 三、有交互的领域对 — 独立交互图

下面对每一个有实质交互的领域对，画出专项交互图。

---

### Pair 1：Identity ↔ Ontology

> **交互本质**：User 本身就是 ABox 对象，MANAGES / BELONGS_TO / MEMBER_OF 是图边，存在 NebulaGraph 中。
> **方向**：Identity 写身份数据到 Ontology；Ontology 图遍历反过来为 Permission 提供 EnrichedIdentity。

```mermaid
sequenceDiagram
    participant AuthSvc  as auth-svc (Identity)
    participant Onto     as ontology-svc
    participant Graph    as NebulaGraph
    participant Bus      as NATS

    %% 写：管理员创建用户 + 分配关系
    AuthSvc->>Onto: POST /v1/objects { entity_type:"User", attrs:{name,email,...} }
    Onto->>Graph: UPSERT vertex User:{user_id}
    Onto->>Bus: PUBLISH ontology.events.User.upsert

    AuthSvc->>Onto: POST /v1/links\n{ from:User:alice, to:OrgUnit:hr, rel:"BELONGS_TO" }
    Onto->>Graph: INSERT EDGE BELONGS_TO User:alice → OrgUnit:hr
    Onto->>Bus: PUBLISH ontology.events.User.link { rel:BELONGS_TO }

    AuthSvc->>Onto: POST /v1/links\n{ from:User:alice, to:User:bob, rel:"MANAGES" }
    Onto->>Graph: INSERT EDGE MANAGES User:alice → User:bob
    Onto->>Bus: PUBLISH ontology.events.User.link { rel:MANAGES }

    %% 读：派生 EnrichedIdentity（图遍历）
    Note over AuthSvc,Graph: 登录时 / L2 缓存 miss 时
    AuthSvc->>Onto: GET /internal/identity/alice/context
    Onto->>Graph: MATCH (u:User {id:'alice'})\n-[:BELONGS_TO*1..3]->(ou:OrgUnit),\n-[:MANAGES]->(reports),\n-[:MEMBER_OF]->(groups),\n-[:HAS_ROLE]->(roles)
    Graph-->>Onto: { departments:[hr], manages:[bob], groups:[], roles:[manager] }
    Onto-->>AuthSvc: EnrichedIdentity { departments, manages, groups, roles }
```

---

### Pair 2：Identity ↔ Permission

> **交互本质**：Permission 评估依赖 EnrichedIdentity（来自 Identity/Ontology 图遍历）；Identity 变更触发权限缓存失效。
> **方向**：双向。Permission 查 Identity；Identity 变更通知 Permission。

```mermaid
sequenceDiagram
    participant Client  as Client
    participant AuthSvc as auth-svc (Identity + Permission)
    participant Onto    as ontology-svc
    participant Cache   as Redis
    participant Bus     as NATS

    %% 方向 A：Permission 评估时读 Identity
    Client->>AuthSvc: authorize(alice, Employee:789, Read)

    AuthSvc->>Cache: GET identity:alice
    Cache-->>AuthSvc: HIT → EnrichedIdentity { manages:[789], roles:[manager] }
    Note over AuthSvc: Step1 RBAC: manager 有 Employee.Read ✅
    Note over AuthSvc: Step2 ReBAC: 789 ∈ manages ✅ (无需再查图)
    AuthSvc-->>Client: AccessDecision { Allow }

    %% 方向 B：Identity 变更 → Permission 缓存失效
    Note over Bus,Cache: 管理员把 alice 从 hr 部门移到 finance
    Bus->>AuthSvc: ontology.events.User.link\n{ from:alice, rel:BELONGS_TO, to:OrgUnit:finance }
    AuthSvc->>Cache: DEL identity:alice
    AuthSvc->>Cache: DEL authz:alice:*（批量）
    Note over Cache: alice 的 EnrichedIdentity 失效\n下次请求重新图遍历
```

---

### Pair 3：Ontology ↔ Permission

> **交互本质**：Ontology 的每次读/写都同步调用 auth-svc.authorize()；Permission 配置变更（Schema 修改）发布事件触发 RBAC 缓存失效。
> **方向**：双向。Ontology 每次操作调 Permission；Permission 通过 Event Bus 反向通知。

```mermaid
sequenceDiagram
    participant Caller  as 任意调用方
    participant Onto    as ontology-svc
    participant Auth    as auth-svc (Permission)
    participant Graph   as NebulaGraph
    participant Cache   as Redis
    participant Bus     as NATS

    %% 读路径：对象读取时的权限检查
    Caller->>Onto: GET /v1/objects/Contract:101\nX-User-Id: alice

    Onto->>Auth: gRPC Authorize { subject:alice, object:Contract:101, op:Read }
    Auth->>Cache: GET authz:alice:Contract:101:read → miss
    Auth->>Cache: GET identity:alice → HIT
    Note over Auth: Step1 RBAC → Step2 ReBAC → Step3 ABAC → Step4 Field
    Auth->>Cache: SET authz:alice:Contract:101:read TTL 2min
    Auth-->>Onto: AccessDecision { AllowWithMask, hidden:[amount] }

    Onto->>Graph: MATCH (o:Contract {id:'101'}) RETURN o
    Graph-->>Onto: Contract { title, amount, party_id }
    Onto->>Onto: 过滤 hidden_fields → 移除 amount
    Onto-->>Caller: Contract { title, party_id }

    %% 写路径：Schema 变更 → 权限缓存失效（反向通知）
    Note over Onto,Bus: 管理员修改 EntityType Contract 权限配置
    Onto->>Bus: PUBLISH ontology.events.Contract.schema_updated\n{ permissions:[{role:analyst, ops:[read]}] }
    Bus->>Auth: schema_updated 事件
    Auth->>Cache: DEL rbac:*:Contract:*（批量）
    Note over Cache: RBAC 层缓存全部失效\n下次请求重新查 EntityTypePermission
```

---

### Pair 4：Ingest → Ontology

> **交互本质**：ingest-svc 是外部数据进入 Ontology 的唯一入口；写入时带幂等 key 防止重复；失败时游标回退续传。
> **方向**：单向写。Ingest → Ontology。

```mermaid
sequenceDiagram
    participant Ingest  as ingest-svc
    participant Auth    as auth-svc
    participant Onto    as ontology-svc
    participant Graph   as NebulaGraph
    participant SQL     as TiDB
    participant Bus     as NATS

    Note over Ingest: JobRunner 拉取一批（500条）外部数据

    loop 每条 row（幂等写）
        Ingest->>Ingest: apply FieldMapping（CEL transform）
        Ingest->>Auth: authorize(system_agent, Employee, Write)
        Auth-->>Ingest: Allow

        Ingest->>Onto: POST /v1/objects\n{\n  entity_type: Employee,\n  attrs: {...},\n  idempotency_key: "src:{source_id}:row:{row_id}"\n}

        alt 对象已存在（idempotency_key 命中）
            Onto-->>Ingest: 200 { status:"skipped", id:"Employee:xxx" }
            Note over Ingest: 跳过，游标照常前进
        else 新对象
            Onto->>Graph: UPSERT vertex Employee:{uuid}
            Onto->>SQL: INSERT employee（结构化投影）
            Onto->>Bus: PUBLISH ontology.events.Employee.upsert
            Onto-->>Ingest: 201 { id:"Employee:{uuid}" }
        end
    end

    Note over Ingest: 批次处理完成，更新游标
    Ingest->>SQL: UPDATE ingest_cursor\nSET last_position = max_row_id

    alt 中途失败（网络/DB 异常）
        Note over Ingest: 游标未更新 → 下次从断点续传\n幂等 key 确保已写的数据不重复
        Ingest->>SQL: UPDATE ingest_job SET status=Failed, error=...
    end
```

---

### Pair 5：Ontology → Event Bus → 下游三方

> **交互本质**：Ontology 每次 mutation（写/删/链接/Schema变更）发布 OntologyEvent 到 NATS；Workflow / Agent / Permission 三方独立订阅、独立消费。
> **方向**：Ontology 单向发布；下游各自独立消费（扇出）。

```mermaid
sequenceDiagram
    participant Onto   as ontology-svc
    participant Bus    as NATS JetStream
    participant Wf     as workflow-svc
    participant Agent  as agent-svc
    participant Auth   as auth-svc (Permission)
    participant Cache  as Redis

    Note over Onto: 任意 mutation 发生
    Onto->>Bus: PUBLISH ontology.events.{EntityType}.{action}\n{\n  id: OntologyId,\n  entity_type: String,\n  action: upsert | delete | link | schema_updated,\n  version: u64,\n  timestamp: DateTime\n}

    par workflow-svc 订阅 ontology.events.>
        Bus->>Wf: event received
        Wf->>Wf: 遍历 EventTrigger 列表\nCEL filter: event.action=='upsert'\n&& event.attrs.status=='signed'
        alt 有匹配触发器
            Wf->>Wf: 创建 WorkflowExecution
        else 无匹配
            Wf->>Wf: 忽略
        end
    and agent-svc 订阅 ontology.events.>
        Bus->>Agent: event received
        Agent->>Cache: DEL sc:{entity_type}:*（语义缓存失效）
        Note over Agent: 可选：主动预计算常见查询的 AgentMemory
    and auth-svc 订阅 ontology.events.>
        Bus->>Auth: event received
        alt action == upsert && entity_type == User（身份变化）
            Auth->>Cache: DEL identity:{user_id}
            Auth->>Cache: DEL authz:{user_id}:*
        else action == schema_updated（Schema权限变化）
            Auth->>Cache: DEL rbac:*:{entity_type}:*
        else action == upsert（业务对象属性变化，影响 ABAC）
            Auth->>Cache: DEL authz:*:{object_id}:*
        end
    end
```

---

### Pair 6：Agent ↔ Function

> **交互本质**：Agent 是 Function 的最大消费方。LLM 规划阶段选择工具；执行阶段同步 gRPC 调用 function-svc；结果返回给 LLM 合成回答。
> **方向**：Agent 单向调用 Function（同步）；Function 把工具列表变更通知 Agent（异步）。

```mermaid
sequenceDiagram
    participant Agent  as agent-svc
    participant LLM    as LLM API
    participant Func   as function-svc
    participant SQL    as TiDB
    participant Bus    as NATS

    %% 工具发现（启动 + 变更时）
    Func->>Bus: PUBLISH functions.updated { function_id, version }
    Bus->>Agent: functions.updated
    Agent->>Func: gRPC ListFunctions { filter: active }
    Func->>SQL: SELECT * FROM function_definition WHERE status='active'
    SQL-->>Func: [FunctionDef...]
    Func-->>Agent: tool_schemas (JSON Schema 列表)
    Agent->>Agent: 更新本地 tool_schema 缓存

    %% 推理 + 工具调用循环
    Note over Agent,LLM: 用户提问进入
    Agent->>LLM: { messages, tools: tool_schemas, system: identity_context }
    LLM-->>Agent: tool_call: { name:"query_contracts", input:{owner_id:alice} }

    Agent->>Func: gRPC ExecuteFunction\n{ function_id, input:{owner_id:alice}, caller_identity }
    Note over Func: 执行 CEL / Rust / NL 逻辑
    Func->>SQL: INSERT function_execution { status:Running }
    Func->>Func: eval logic（可能进一步调用 Ontology / External）
    Func->>SQL: UPDATE function_execution { status:Success, output:... }
    Func-->>Agent: FunctionResult { output:[Contract:101, Contract:102] }

    Agent->>LLM: { tool_result, stream: true }
    LLM-->>Agent: answer stream chunks
    Agent-->>Agent: 继续或结束（max_turns 判断）

    Note over Agent: 多轮 tool_call 直到 LLM 不再调用工具
```

---

### Pair 7：Agent ↔ Embedding

> **交互本质**：Agent 在两个场景依赖 Embedding：① 语义缓存（query 向量化 → 近邻搜索）；② AgentMemory 检索（历史记忆向量化）。
> **降级策略**：Embedding 不可用时，Agent 跳过语义缓存，直接走 LLM。

```mermaid
sequenceDiagram
    participant Agent  as agent-svc
    participant Embed  as embedding-svc
    participant Cache  as Redis (SemanticCache)
    participant Vec    as VectorStore (TiDB Vector)
    participant LLM    as LLM API

    %% 场景 A：语义缓存命中
    Note over Agent: 用户提问："显示我管理的员工"
    Agent->>Embed: gRPC Embed { text: query, model: BGE-small-zh }

    alt embedding-svc 正常
        Embed-->>Agent: vector[512]
        Agent->>Cache: cosine 近邻搜索 sc:{hash(vector)} threshold=0.92
        Cache-->>Agent: HIT → cached_response
        Agent-->>Agent: 直接返回缓存结果，跳过 LLM
    else embedding-svc 不可用（Circuit Breaker open）
        Embed-->>Agent: ServiceUnavailable
        Note over Agent: 降级：跳过语义缓存，直接调 LLM
        Agent->>LLM: { messages, tools }
        LLM-->>Agent: response（无语义缓存加速）
    end

    %% 场景 B：AgentMemory 检索（多轮对话上下文）
    Note over Agent: 新一轮提问，检索相关历史记忆
    Agent->>Embed: gRPC Embed { text: new_query }
    Embed-->>Agent: query_vector[512]
    Agent->>Vec: SELECT * FROM agent_memory\n  ORDER BY cosine_sim(embedding, query_vector) DESC\n  WHERE user_id=alice LIMIT 5
    Vec-->>Agent: [MemoryItem...] (top-5 相关记忆)
    Agent->>LLM: { messages, memory_context: top5_memories, tools }
```

---

### Pair 8：Workflow ↔ Function

> **交互本质**：Workflow 是 Function 的另一大消费方。每个 WorkflowStep 对应一次 Function 调用；Saga 补偿也通过调用补偿 Function 实现。
> **方向**：Workflow 单向调用 Function（同步）。

```mermaid
sequenceDiagram
    participant Wf     as workflow-svc
    participant Func   as function-svc
    participant Onto   as ontology-svc
    participant Ext    as External API
    participant SQL    as TiDB

    Note over Wf: WorkflowExecution 启动，当前 step = Step A

    %% 正常步骤执行
    Wf->>SQL: UPDATE workflow_execution { current_step: A, status:Running }
    Wf->>Func: gRPC ExecuteFunction\n{ function_id: step_A.function_id,\n  input: map(context, step_A.input_mapping) }
    Func->>Onto: 查询 / 写入 Ontology（Function 逻辑需要）
    Onto-->>Func: data
    Func-->>Wf: FunctionResult { output }
    Wf->>Wf: 将 output 合并入 context
    Wf->>SQL: UPDATE step_A status=Success

    %% Step B 执行失败
    Wf->>Func: gRPC ExecuteFunction { function_id: step_B.function_id, ... }
    Func->>Ext: HTTP outbound call
    Ext-->>Func: 500 Error
    Func-->>Wf: FunctionError { reason: "ERP unavailable" }
    Wf->>SQL: UPDATE step_B status=Failed

    %% Saga 补偿：回调已成功步骤的 compensation function
    Note over Wf: 触发 Saga，倒序补偿
    Wf->>SQL: INSERT saga_log { compensations:[A] }
    Wf->>Func: gRPC ExecuteFunction\n{ function_id: step_A.compensation, input: context.step_A_output }
    Func->>Ext: 撤销操作（DELETE /emails/xxx）
    Ext-->>Func: 200 OK
    Func-->>Wf: OK
    Wf->>SQL: UPDATE saga_log.A status=Done
    Wf->>SQL: UPDATE workflow_execution status=Failed
```

---

### Pair 9：Ingest → Embedding（文件向量化）

> **交互本质**：文件摄入时，ingest-svc 对文本分片后调用 embedding-svc 向量化，写入 VectorStore 供语义搜索。
> **方向**：Ingest 单向调用 Embedding（同步，带断路器降级）。

```mermaid
sequenceDiagram
    participant Ingest  as ingest-svc
    participant Store   as RustFS
    participant Embed   as embedding-svc
    participant Onto    as ontology-svc
    participant Vec     as VectorStore
    participant SQL     as TiDB

    Ingest->>Store: GET file bytes（report.pdf）
    Store-->>Ingest: bytes
    Ingest->>Ingest: PDF → text chunks（每块 ≤512 tokens）

    loop chunk[0..N]
        Ingest->>Embed: gRPC Embed { text: chunk[i] }

        alt 正常响应
            Embed-->>Ingest: vector[512]
            Ingest->>Onto: POST /v1/objects\n{ entity_type:DocumentChunk,\n  attrs:{chunk_index, text},\n  embedding: vector }
            Onto->>Vec: UPSERT { id:chunk_id, vector, metadata:{doc_id,owner_id} }
        else Circuit Breaker open（embedding-svc 不可用）
            Embed-->>Ingest: ServiceUnavailable
            Ingest->>SQL: INSERT retry_queue { chunk_id, priority:low }
            Note over Ingest: 跳过本 chunk，继续处理其他\n重试队列稍后批量处理
        end
    end

    Note over Ingest: 后台 RetryWorker 定期消费 retry_queue
    loop retry_queue 中的 chunk
        Ingest->>Embed: gRPC Embed { text: chunk }
        Embed-->>Ingest: vector（已恢复）
        Ingest->>Vec: UPSERT { id:chunk_id, vector }
        Ingest->>SQL: DELETE FROM retry_queue WHERE chunk_id = ?
    end
```

---

## 四、关键交互规则总结

| 规则 | 说明 |
|------|------|
| **Ontology 是数据写入的唯一入口** | 任何服务（Ingest / Workflow / Function）不能绕过 ontology-svc 直接写 NebulaGraph / TiDB |
| **Permission 是 Ontology 读写的守门人** | ontology-svc 每次操作前同步调用 auth-svc.authorize()，无例外 |
| **User 是 Ontology 的一等公民** | User 存储为 ABox 对象，图关系在 NebulaGraph，ReBAC 才能统一图遍历 |
| **Event Bus 是唯一的跨领域异步通道** | 领域间异步通知不通过 HTTP 回调，全部走 NATS OntologyEvent |
| **Function 是出站集成的唯一出口** | HTTP outbound 请求只能通过 function-svc，不允许其他服务直连外网（ADR-22）|
| **Embedding 是可降级的非关键路径** | Embedding 不可用时，Agent 降级跳过语义缓存，核心查询链路不中断 |
| **缓存失效由事件驱动，非轮询** | 所有 Redis 缓存失效通过 NATS 事件主动触发，auth-svc 订阅消费 |

---

## 五、与既有 Flow 对应关系

| 本文 Pair | 对应已有 Flow（ontology-permission-interactions_v0.1.1.md）| 对应新 Flow（user-stories-and-interactions_v0.1.0.md）|
|-----------|------------------------------------------------------|-----------------------------------------------------|
| Pair 1 Identity ↔ Ontology | Flow 1（登录时图遍历） | — |
| Pair 2 Identity ↔ Permission | Flow 1（EnrichedIdentity 缓存）/ Flow 6（缓存失效） | — |
| Pair 3 Ontology ↔ Permission | Flow 2（读取三级缓存）/ Flow 3（写入 Deny）/ Flow 4（Schema 变更）| — |
| Pair 4 Ingest → Ontology | — | Flow 7（摄入全链路）|
| Pair 5 Ontology → Event Bus → 三方 | Flow 6（缓存失效）| Flow 7（下游消费）|
| Pair 6 Agent ↔ Function | Flow 5（Agent 查询工具调用）| Flow 9（Function 注册）|
| Pair 7 Agent ↔ Embedding | Flow 5（语义缓存检查）| Flow 9（语义缓存写入）|
| Pair 8 Workflow ↔ Function | — | Flow 8（Saga 补偿）|
| Pair 9 Ingest → Embedding | — | Flow 10（文件向量化）|

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本：9 个跨领域交互对，领域交互总览图，交互规则总结 |
