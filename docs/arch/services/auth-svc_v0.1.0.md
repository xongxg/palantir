# auth-svc 子架构

> 状态：设计阶段 | 日期：2026-03-19

## 职责

RBAC + ABAC + ReBAC 策略管理与评估，热路径 P99 < 5ms。

---

## API 端点

```
# 授权评估（热路径）
POST   /v1/authorize                  → 权限判断（同步，< 5ms）

# 角色管理（RBAC）
POST   /v1/roles                      → 创建角色
PUT    /v1/roles/{id}/permissions     → 分配权限
POST   /v1/users/{id}/roles           → 绑定角色

# 策略管理（ABAC）
POST   /v1/policies                   → 创建属性策略
GET    /v1/policies                   → 列出

# 关系管理（ReBAC）
POST   /v1/relationships              → 建立主体-资源关系
DELETE /v1/relationships/{id}         → 删除
```

---

## 评估流程

```
POST /v1/authorize { subject, action, resource }
  ↓
1. Redis 缓存命中？→ 直接返回（< 1ms）
  ↓ miss
2. RBAC：subject Role 是否包含 (action, resource_type)？
3. ABAC：resource 属性是否满足 Policy 条件？
4. ReBAC：subject 与 resource 图关系是否满足规则？
  ↓
Allow / Deny / AllowWithMask
  ↓
结果写 Redis（短 TTL）
```

---

## 返回值

```rust
pub enum AuthzResult {
    Allow,
    AllowWithMask { hidden_fields: Vec<String> },  // 字段级脱敏
    Deny { reason: String },
}
```

---

## 性能目标

- P99 < 5ms（含 Redis 缓存命中路径）
- Redis 缓存 Key：`authz:{subject_id}:{resource_type}:{resource_id}:{action}`

---

## 复用 crate

- `palantir-auth-core`（NEW）：Permission / Policy 类型 + PolicyEvaluator trait

---

## 逃生门

`PolicyEvaluator` trait 抽象，未来可换 OPA / Cedar。

---

## 待细化

- [ ] ReBAC 图规则 DSL 设计
- [ ] ABAC 属性条件表达式语法
- [ ] 与 ontology-svc 数据分类标签集成
- [ ] 多租户时的权限隔离边界（ADR-04 待定）

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本，架构设计阶段 |
