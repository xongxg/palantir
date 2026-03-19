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

## 领域模型

见 [../domain/ontology-permission-domain.md](../domain/ontology-permission-domain.md)

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
