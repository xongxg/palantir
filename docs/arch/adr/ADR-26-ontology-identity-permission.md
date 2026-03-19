# ADR-26: Ontology 身份与数据权限配置方案

> 状态：✅ 已决策 | 日期：2026-03-19 | 实现阶段：P1

## 问题

Ontology 数据的访问控制如何设计？身份如何与 Ontology 图关联？权限如何在四个粒度上配置？

## 决策

**身份即 Ontology 对象，权限四粒度分层：RBAC（EntityType）→ ReBAC（Object）→ ABAC（Row）→ Classification（Field）。**

## 身份层

JWT claims 是起点，结合 Ontology 图派生丰富身份上下文：

```
JWT { sub: "user-123", roles: ["analyst"] }
  ↓
ontology-svc 查询图关系：
  User:user-123 → BELONGS_TO → Department:finance
  User:user-123 → MANAGES    → [Employee:456, ...]
  ↓
EnrichedIdentity {
  user_id, static_roles, departments, manages, ...
}
```

用户本身是 `User` EntityType 的 ABox 对象，通过图关系动态派生权限，无需硬编码。

## 四粒度权限

### 1. EntityType 级（RBAC）

TBox 定义时附带角色权限矩阵：

```json
{
  "entity_type": "Employee",
  "permissions": [
    { "role": "hr",      "ops": ["read","write","delete"] },
    { "role": "analyst", "ops": ["read"] },
    { "role": "manager", "ops": ["read"] }
  ]
}
```

### 2. Object 级（ReBAC）

图关系规则推导，不逐个对象配置：

```
rule "manager-can-read-reports" {
  subject MANAGES object → Allow(read)
}
rule "dept-member-can-read-peers" {
  subject BELONGS_TO dept AND object BELONGS_TO dept → Allow(read)
}
```

### 3. Row 级（ABAC）

基于对象属性的动态条件：

```
policy "analyst-own-dept-only" {
  subject.role == "analyst"
  AND object.department IN subject.departments
  → Allow(read)
}
```

### 4. Field 级（Classification）

TBox 字段打标签，角色映射可见范围：

```
字段标签：Public / Internal / Confidential / PII

角色可见性矩阵：
  Role      Public  Internal  Confidential  PII
  public    ✅      ❌        ❌           ❌
  analyst   ✅      ✅        ❌           ❌
  hr        ✅      ✅        ✅           ✅（加密后）
  manager   ✅      ✅        ❌           ❌
```

返回值：`AllowWithMask { hidden_fields: ["salary", "id_number"] }`

## 配置入口

| 配置项 | 管理端点 |
|--------|---------|
| 字段 Classification | ontology-svc `/v1/schema`（TBox 定义时）|
| Role → EntityType 权限 | auth-svc `/v1/roles` |
| ReBAC 关系规则 | auth-svc `/v1/relationships/rules` |
| ABAC Policy | auth-svc `/v1/policies` |

## 完整评估流程

```
请求 GET /v1/objects/Employee:456
  ↓
api-gateway：JWT 解析 → EnrichedIdentity
  ↓
ontology-svc 调 auth-svc.authorize()：
  Step 1 EntityType：analyst 能读 Employee？        → ✅
  Step 2 Object：user MANAGES Employee:456？         → ✅
  Step 3 Row：Employee.department ∈ user.depts？     → ✅
  Step 4 Field：返回 AllowWithMask { hidden: [...] }
  ↓
ontology-svc 过滤字段后返回
```

## 缓存策略

### 缓存什么

| 缓存项 | Key | TTL | 失效方式 |
|--------|-----|-----|---------|
| EnrichedIdentity | `identity:{user_id}` | 5min | 事件驱动 |
| RBAC 结果 | `rbac:{role}:{entity_type}:{op}` | 30min | schema 变更事件 |
| AccessDecision | `authz:{user_id}:{object_id}:{op}` | 1-2min | 对象/关系变更事件 |

### 不缓存什么

| 项目 | 原因 |
|------|------|
| ReBAC 边单独结果 | 关系变化频繁，失效代价 > 计算代价，AccessDecision 整体缓存已覆盖 |
| ABAC CEL 结果 | 对象属性随时变，失效代价高 |
| Field Visibility Matrix | 纯内存静态矩阵，μs 级计算，无需缓存 |

### 事件驱动失效（订阅 NATS）

```rust
match event.op {
    Upsert if entity_type == "User"  => del("identity:{id}", "authz:{id}:*"),
    Link   if rel == "MANAGES"       => del("identity:{from}", "authz:{from}:*"),
    Link   if rel == "BELONGS_TO"    => del("identity:{user_id}"),
    SchemaUpdated { entity_type }    => del("rbac:*:{entity_type}:*"),
    Upsert                           => del("authz:*:{id}:*"),
}
```

### 性能收益

```
无缓存：图遍历 ~10ms + 四层评估 ~5ms = ~15ms/请求
全缓存命中：Redis 单次查询 < 1ms
典型场景（用户连续操作同一批对象）：
  第 1 次 ~15ms（冷）→ 后续 < 1ms（热）
```

## 领域模型

见 [../domain/ontology-permission-domain_v0.1.0.md](../domain/ontology-permission-domain_v0.1.0.md)

## 交互流程

见 [../domain/ontology-permission-interactions_v0.1.1.md](../domain/ontology-permission-interactions_v0.1.1.md)

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
| v1.1 | 2026-03-19 | 补充缓存策略：EnrichedIdentity/RBAC/AccessDecision 三级缓存 + 事件驱动失效 |
