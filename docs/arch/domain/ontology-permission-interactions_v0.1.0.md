# Ontology 身份与权限 — 交互流程图

> 版本：v0.1.0 | 日期：2026-03-19 | 关联：ADR-26、domain/ontology-permission-domain_v0.1.0.md

---

## Flow 1：用户登录与身份增强

> JWT 颁发 + 从 Ontology 图派生 EnrichedIdentity

```mermaid
sequenceDiagram
    actor Client
    participant GW   as api-gateway
    participant Auth as auth-svc
    participant Onto as ontology-svc
    participant DB   as SurrealDB
    participant Cache as Redis

    Client->>GW: POST /auth/login { username, password }
    GW->>Auth: forward credentials

    Auth->>DB: SELECT User WHERE email = ?
    DB-->>Auth: User { id, password_hash, status }
    Auth->>Auth: verify password_hash

    Note over Auth,DB: 图遍历：派生 EnrichedIdentity
    Auth->>Onto: GET /internal/identity/{user_id}/context
    Onto->>DB: MATCH (u:User)-[:BELONGS_TO]->(ou:OrgUnit)\nMATCH (u)-[:MANAGES]->(reports)\nMATCH (u)-[:MEMBER_OF]->(groups)\nMATCH (u)-[:HAS_ROLE]->(roles)
    DB-->>Onto: departments[], manages[], groups[], roles[]
    Onto-->>Auth: EnrichedIdentity

    Auth->>Auth: 生成 Access Token（15min, RS256）\n生成 Refresh Token（7天, 单次）
    Auth->>Cache: SET refresh_token:{jti} TTL 7d

    Auth-->>GW: Access Token（body）\nRefresh Token（Set-Cookie: HttpOnly）
    GW-->>Client: 200 OK
```

---

## Flow 2：对象读取（四粒度权限评估）

> 核心流程，逐层短路评估

```mermaid
sequenceDiagram
    actor Client
    participant GW    as api-gateway
    participant Onto  as ontology-svc
    participant Auth  as auth-svc
    participant DB    as SurrealDB
    participant Cache as Redis

    Client->>GW: GET /v1/objects/Employee:456\nAuthorization: Bearer {token}

    GW->>GW: JWT 验证（签名 + exp + 黑名单）
    GW->>Cache: GET blacklist:{jti}
    Cache-->>GW: nil（未吊销）
    GW->>GW: 提取 static_roles, user_id
    GW->>Onto: forward + X-User-Id / X-Roles header

    Onto->>Auth: authorize(\n  subject: user-123,\n  object: Employee:456,\n  op: Read\n)

    rect rgb(240, 248, 255)
        Note over Auth,DB: Step 1 — EntityType RBAC
        Auth->>DB: SELECT permissions FROM entity_type\nWHERE name='Employee'
        DB-->>Auth: [{role:'analyst', ops:['read']}...]
        Auth->>Auth: role='analyst' ∈ Read ops? ✅
    end

    rect rgb(240, 255, 240)
        Note over Auth,DB: Step 2 — Object ReBAC
        Auth->>Cache: GET authz:{user-123}:{Employee:456}:read
        Cache-->>Auth: nil（未缓存）
        Auth->>DB: MATCH (u:User {id:'user-123'})\n-[:MANAGES]->(e:Employee {id:'456'})
        DB-->>Auth: Edge found ✅
        Auth->>Cache: SET authz:{user-123}:{Employee:456}:read TTL 60s
    end

    rect rgb(255, 255, 240)
        Note over Auth,DB: Step 3 — Row ABAC
        Auth->>DB: SELECT * FROM abac_policy\nWHERE entity_type='Employee'
        DB-->>Auth: [policy: dept IN subject.depts]
        Auth->>Auth: CEL eval:\n  Employee:456.dept ∈ user.depts? ✅
    end

    rect rgb(255, 240, 240)
        Note over Auth: Step 4 — Field Classification
        Auth->>Auth: FieldVisibilityMatrix:\n  analyst 不见 Confidential/PII\n  hidden: ['salary','id_number']
    end

    Auth-->>Onto: AccessDecision {\n  decision: AllowWithMask,\n  hidden_fields: ['salary','id_number']\n}

    Onto->>DB: SELECT * FROM Employee WHERE id='456'
    DB-->>Onto: OntologyObject { all fields }
    Onto->>Onto: 过滤 hidden_fields

    par 异步写审计
        Onto->>DB: INSERT audit_log {\n  who, ip, what:Read,\n  target:Employee:456,\n  fields:visible_fields\n}
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

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本：5 个核心交互流程 |
