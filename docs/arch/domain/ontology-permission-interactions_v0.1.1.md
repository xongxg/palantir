# Ontology 身份与权限 — 交互流程图

> 版本：v0.1.1 | 日期：2026-03-19 | 关联：ADR-26、domain/ontology-permission-domain_v0.1.0.md

---

## Flow 1：用户登录与身份增强

> JWT 颁发 + 从 Ontology 图派生 EnrichedIdentity + 写入缓存

```mermaid
sequenceDiagram
    actor Client
    participant GW    as api-gateway
    participant Auth  as auth-svc
    participant Onto  as ontology-svc
    participant DB    as SurrealDB
    participant Cache as Redis

    Client->>GW: POST /auth/login { username, password }
    GW->>Auth: forward credentials

    Auth->>DB: SELECT User WHERE email = ?
    DB-->>Auth: User { id, password_hash, status }
    Auth->>Auth: verify password_hash

    Note over Auth,Cache: 检查 EnrichedIdentity 缓存
    Auth->>Cache: GET identity:{user_id}
    Cache-->>Auth: nil（首次登录，未缓存）

    Note over Auth,DB: 图遍历：派生 EnrichedIdentity
    Auth->>Onto: GET /internal/identity/{user_id}/context
    Onto->>DB: MATCH (u:User)-[:BELONGS_TO]->(ou:OrgUnit)\nMATCH (u)-[:MANAGES]->(reports)\nMATCH (u)-[:MEMBER_OF]->(groups)\nMATCH (u)-[:HAS_ROLE]->(roles)
    DB-->>Onto: departments[], manages[], groups[], roles[]
    Onto-->>Auth: EnrichedIdentity

    Auth->>Cache: SET identity:{user_id} TTL 5min
    Note over Cache: EnrichedIdentity 已缓存\n后续请求跳过图遍历

    Auth->>Auth: 生成 Access Token（15min, RS256）\n生成 Refresh Token（7天, 单次）
    Auth->>Cache: SET refresh_token:{jti} TTL 7d

    Auth-->>GW: Access Token（body）\nRefresh Token（Set-Cookie: HttpOnly）
    GW-->>Client: 200 OK
```

---

## Flow 2：对象读取（三级缓存 + 四粒度权限评估）

> 优先命中缓存短路，miss 时逐层评估

```mermaid
sequenceDiagram
    actor Client
    participant GW    as api-gateway
    participant Onto  as ontology-svc
    participant Auth  as auth-svc
    participant DB    as SurrealDB
    participant Cache as Redis

    Client->>GW: GET /v1/objects/Employee:456\nAuthorization: Bearer {token}

    GW->>GW: JWT 验证（签名 + exp）
    GW->>Cache: GET blacklist:{jti}
    Cache-->>GW: nil（未吊销）
    GW->>Onto: forward + X-User-Id / X-Roles header

    Onto->>Auth: authorize(user-123, Employee:456, Read)

    rect rgb(230, 245, 255)
        Note over Auth,Cache: 🚀 Level 1 — AccessDecision 整体缓存
        Auth->>Cache: GET authz:{user-123}:{Employee:456}:read
        Cache-->>Auth: HIT → AccessDecision(AllowWithMask)
        Note over Auth: 直接返回，跳过所有评估步骤 < 1ms
    end

    alt AccessDecision 缓存 miss
        rect rgb(230, 255, 230)
            Note over Auth,Cache: 🚀 Level 2 — EnrichedIdentity 缓存
            Auth->>Cache: GET identity:{user-123}
            Cache-->>Auth: HIT → EnrichedIdentity\n{ roles, depts, manages }
            Note over Auth: 跳过图遍历，直接进入评估
        end

        alt EnrichedIdentity 缓存 miss
            Auth->>Onto: GET /internal/identity/user-123/context
            Onto->>DB: 图遍历 MANAGES / BELONGS_TO / MEMBER_OF
            DB-->>Onto: departments[], manages[], groups[]
            Onto-->>Auth: EnrichedIdentity
            Auth->>Cache: SET identity:{user-123} TTL 5min
        end

        rect rgb(240, 248, 255)
            Note over Auth,Cache: Step 1 — EntityType RBAC
            Auth->>Cache: GET rbac:analyst:Employee:read
            Cache-->>Auth: HIT ✅
        end

        alt RBAC 缓存 miss
            Auth->>DB: SELECT permissions FROM entity_type WHERE name='Employee'
            DB-->>Auth: permissions[]
            Auth->>Cache: SET rbac:analyst:Employee:read TTL 30min
        end

        rect rgb(240, 255, 240)
            Note over Auth,DB: Step 2 — ReBAC（依赖 EnrichedIdentity.manages）
            Auth->>Auth: Employee:456 ∈ manages? ✅
            Note over Auth: manages 已在 EnrichedIdentity 里，无需额外查询
        end

        rect rgb(255, 255, 240)
            Note over Auth,DB: Step 3 — ABAC
            Auth->>DB: SELECT * FROM abac_policy WHERE entity_type='Employee'
            DB-->>Auth: policies[]
            Auth->>Auth: CEL eval: dept ∈ subject.depts? ✅
        end

        rect rgb(255, 240, 240)
            Note over Auth: Step 4 — Field Classification（内存矩阵，μs）
            Auth->>Auth: hidden: ['salary','id_number']
        end

        Auth->>Cache: SET authz:{user-123}:{Employee:456}:read\n  TTL 2min
    end

    Auth-->>Onto: AccessDecision { AllowWithMask, hidden_fields }

    Onto->>DB: SELECT * FROM Employee WHERE id='456'
    DB-->>Onto: OntologyObject { all fields }
    Onto->>Onto: 过滤 hidden_fields

    par 异步写审计
        Onto->>DB: INSERT audit_log { who, ip, Read, Employee:456 }
    end

    Onto-->>GW: OntologyObject（已脱敏）
    GW-->>Client: 200 OK { name, email, department }
```

---

## Flow 3：对象写入（含权限 + 事件发布）

```mermaid
sequenceDiagram
    actor Client
    participant GW    as api-gateway
    participant Onto  as ontology-svc
    participant Auth  as auth-svc
    participant DB    as SurrealDB
    participant Bus   as NATS

    Client->>GW: PUT /v1/objects/Employee:456\n{ department: "engineering" }

    GW->>GW: JWT 验证
    GW->>Onto: forward + identity headers

    Onto->>Auth: authorize(user-123, Employee:456, Write)

    Note over Auth: Step 1 RBAC: analyst 无 Write 权限
    Auth-->>Onto: AccessDecision { decision: Deny,\n reason: "role analyst has no Write on Employee" }

    Onto->>DB: INSERT audit_log { decision: Deny, ... }
    Onto-->>GW: 403 Forbidden
    GW-->>Client: 403 { code: PERMISSION_DENIED }

    Note over Client,Bus: --- 以 hr 角色重试 ---

    Client->>GW: PUT /v1/objects/Employee:456\nAuthorization: Bearer {hr_token}
    GW->>Onto: forward

    Onto->>Auth: authorize(hr-user, Employee:456, Write)
    Auth-->>Onto: AccessDecision { decision: Allow }

    Onto->>DB: BEGIN TRANSACTION
    Onto->>DB: UPDATE Employee:456 SET department='engineering',\n  version += 1, tx_time = now()
    DB-->>Onto: updated object

    Onto->>Bus: PUBLISH ontology.events.Employee.upsert\n{ object: Employee:456 }
    Bus-->>Onto: ack

    Onto->>DB: COMMIT
    Onto->>DB: INSERT audit_log { decision: Allow, op: Write }

    Onto-->>GW: 200 OK { updated object }
    GW-->>Client: 200 OK
```

---

## Flow 4：TBox Schema 定义（含权限配置）

> 管理员定义 EntityType，附带字段分类和角色权限

```mermaid
sequenceDiagram
    actor Admin
    participant GW   as api-gateway
    participant Onto as ontology-svc
    participant Auth as auth-svc
    participant DB   as SurrealDB
    participant Bus  as NATS

    Admin->>GW: POST /v1/schema/entity-types\n{\n  name: "Contract",\n  fields: [\n    {name:"title", classification:"Internal"},\n    {name:"amount", classification:"Confidential"},\n    {name:"party_id", classification:"PII"}\n  ],\n  permissions: [\n    {role:"legal", ops:["read","write"]},\n    {role:"analyst", ops:["read"]}\n  ]\n}

    GW->>GW: JWT 验证（需要 Admin 角色）
    GW->>Onto: forward

    Onto->>Auth: authorize(admin, EntityType, Admin)
    Auth-->>Onto: Allow

    Onto->>DB: INSERT entity_type {\n  name, fields, permissions, version:1\n}
    DB-->>Onto: EntityType:contract created

    Note over Onto,Bus: 通知各服务 Schema 变更
    Onto->>Bus: PUBLISH ontology.events.Contract.schema_updated\n{ entity_type: "Contract", version: 1 }

    par function-svc 订阅
        Bus->>FuncSvc: schema_updated → 重建 tool schema
    and agent-svc 订阅
        Bus->>AgentSvc: schema_updated → 更新 context injection
    end

    Onto-->>GW: 201 Created
    GW-->>Admin: 201 { entity_type_id }
```

---

## Flow 5：Agent 查询（身份感知 + 权限透传）

> Agent 查询时携带用户身份，权限评估在 ontology-svc 发生

```mermaid
sequenceDiagram
    actor User
    participant GW      as api-gateway
    participant Agent   as agent-svc
    participant Embed   as embedding-svc
    participant Cache   as Redis
    participant Func    as function-svc
    participant Onto    as ontology-svc
    participant LLM     as LLM API

    User->>GW: POST /v1/query\n{ query: "显示我管理的员工的部门分布" }\nAuthorization: Bearer {token}

    GW->>GW: JWT 验证 → EnrichedIdentity
    GW->>Agent: forward + X-User-Id / X-Roles / X-Identity-Context

    Note over Agent,Cache: Semantic Cache 检查
    Agent->>Embed: POST /v1/embed { text: query }
    Embed-->>Agent: vector[512]
    Agent->>Cache: GET sc:{hash(vector)}
    Cache-->>Agent: nil（miss）

    Note over Agent,LLM: Planner：生成执行计划
    Agent->>Func: GET /v1/functions（注入 Schema context）
    Func-->>Agent: tool_schemas[]
    Agent->>LLM: { messages, tools: tool_schemas,\n  system: "user manages: [456,789,...]" }
    LLM-->>Agent: tool_call: query_employees\n  { filter: "id IN manages" }

    Note over Agent,Onto: Executor：携带用户身份执行
    Agent->>Onto: GET /v1/objects?entity_type=Employee\n  &filter=id IN [456,789]\n  X-User-Id: user-123

    Note over Onto: 权限评估（同 Flow 2）\n每个对象独立评估字段可见性
    Onto-->>Agent: [Employee:456 (masked), Employee:789 (masked)]

    Note over Agent,LLM: Synthesizer：生成回答
    Agent->>LLM: { results, stream: true }
    LLM-->>Agent: stream chunks...

    Agent->>Cache: SET sc:{hash(vector)} TTL 5min

    Agent-->>GW: SSE stream
    GW-->>User: SSE stream { answer }
```

---

## Flow 6：缓存失效（事件驱动）

> OntologyEvent 触发对应缓存清除，保证一致性

```mermaid
sequenceDiagram
    participant Ingest as ingest-svc
    participant Onto   as ontology-svc
    participant Bus    as NATS
    participant Auth   as auth-svc
    participant Cache  as Redis

    Note over Ingest,Onto: 场景 A：新增 MANAGES 关系
    Ingest->>Onto: POST /v1/links\n{ from: User:user-123, to: Employee:999, rel: MANAGES }
    Onto->>Bus: PUBLISH ontology.events.User.link\n{ from: User:user-123, rel: MANAGES, to: Employee:999 }

    Bus->>Auth: event received
    Auth->>Cache: DEL identity:{user-123}
    Auth->>Cache: DEL authz:{user-123}:*（批量）
    Note over Cache: user-123 的 EnrichedIdentity 失效\n下次请求重新图遍历

    Note over Ingest,Onto: 场景 B：用户角色变更
    Ingest->>Onto: PUT /v1/objects/User:user-123\n{ roles: [..., "hr"] }
    Onto->>Bus: PUBLISH ontology.events.User.upsert\n{ id: User:user-123 }

    Bus->>Auth: event received
    Auth->>Cache: DEL identity:{user-123}
    Auth->>Cache: DEL authz:{user-123}:*
    Note over Cache: 角色变更，所有授权结果失效

    Note over Ingest,Onto: 场景 C：EntityType Schema 变更（权限矩阵更新）
    Ingest->>Onto: PUT /v1/schema/entity-types/Employee\n{ permissions: [...] }
    Onto->>Bus: PUBLISH ontology.events.Employee.schema_updated

    Bus->>Auth: event received
    Auth->>Cache: DEL rbac:*:Employee:*（批量）
    Note over Cache: RBAC 缓存失效\n下次请求重新查 EntityTypePermission

    Note over Ingest,Onto: 场景 D：业务对象属性变更（影响 ABAC）
    Ingest->>Onto: PUT /v1/objects/Employee:456\n{ department: "engineering" }
    Onto->>Bus: PUBLISH ontology.events.Employee.upsert\n{ id: Employee:456 }

    Bus->>Auth: event received
    Auth->>Cache: DEL authz:*:{Employee:456}:*（批量）
    Note over Cache: 对象属性变，相关授权结果失效\nEnrichedIdentity 不受影响
```

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本：5 个核心交互流程 |
| v0.1.1 | 2026-03-19 | Flow 1 加入 EnrichedIdentity 缓存写入；Flow 2 升级为三级缓存短路评估；新增 Flow 6 事件驱动缓存失效 |
