# Ontology 身份与数据权限 — 领域模型

> 版本：v0.1.0 | 日期：2026-03-19 | 关联 ADR：ADR-26

---

## 1. 聚合边界划分

```
┌─────────────────────────┐   ┌──────────────────────────────┐
│     Identity 聚合        │   │       Ontology 聚合           │
│                         │   │                              │
│  User                   │   │  EntityType（TBox）           │
│  Role                   │   │  FieldDefinition             │
│  Group                  │   │  OntologyObject（ABox）       │
│  OrganizationalUnit     │   │  OntologyRelationship        │
└────────────┬────────────┘   └──────────────┬───────────────┘
             │                               │
             └──────────────┬────────────────┘
                            │
             ┌──────────────▼───────────────┐
             │       Permission 聚合         │
             │                              │
             │  EntityTypePermission        │
             │  RelationshipRule（ReBAC）    │
             │  AbacPolicy                  │
             │  FieldClassification         │
             │  AccessDecision              │
             │  AuditLog                    │
             └──────────────────────────────┘
```

---

## 2. Identity 聚合

### User（用户，ABox 对象）

```
User {
  id:           UserId              # "User:{uuid}"，SurrealDB ABox 对象
  external_id:  String              # OAuth sub / LDAP DN
  name:         String
  email:        String（PII）
  status:       Active | Suspended | Deleted
  created_at:   DateTime
}

关系（SurrealDB RELATE）：
  User -[BELONGS_TO]-> OrganizationalUnit
  User -[HAS_ROLE]->   Role
  User -[MANAGES]->    User
  User -[MEMBER_OF]->  Group
```

### Role（角色）

```
Role {
  id:           RoleId
  name:         String              # "hr" / "analyst" / "manager"
  description:  String
  is_system:    bool                # 系统内置角色不可删除
}
```

### Group（用户组）

```
Group {
  id:     GroupId
  name:   String
  type:   Static | Dynamic          # Dynamic：ABAC 条件自动计算成员
  condition: Option<AbacExpr>       # Dynamic Group 的成员条件
}
```

### OrganizationalUnit（组织单元）

```
OrganizationalUnit {
  id:        OrgUnitId
  name:      String
  parent_id: Option<OrgUnitId>      # 树形结构，支持多级
  type:      Department | Team | Company
}

关系：
  OrganizationalUnit -[PARENT_OF]-> OrganizationalUnit
```

### EnrichedIdentity（运行时，非持久化）

```
EnrichedIdentity {
  user_id:      UserId
  static_roles: Vec<RoleId>         # 来自 JWT
  departments:  Vec<OrgUnitId>      # 来自图遍历
  manages:      Vec<OntologyId>     # 来自 MANAGES 关系
  groups:       Vec<GroupId>        # 来自 MEMBER_OF 关系
  attributes:   HashMap             # 其他动态属性
}
```

---

## 3. Ontology 聚合

### EntityType（TBox，Schema 定义）

```
EntityType {
  id:           EntityTypeId
  name:         String              # "Employee" / "Contract"
  description:  String
  fields:       Vec<FieldDefinition>
  permissions:  Vec<EntityTypePermission>   # 内嵌权限配置
  retention:    Option<RetentionPolicy>     # 数据保留策略（ADR-09）
  version:      u32
}
```

### FieldDefinition（字段定义，TBox 内嵌）

```
FieldDefinition {
  name:           String
  field_type:     String | Number | Boolean | DateTime | Reference
  classification: Public | Internal | Confidential | PII
  encrypted:      bool              # PII 字段是否加密存储
  required:       bool
  description:    Option<String>
}
```

### OntologyObject（ABox，数据实例）

```
OntologyObject {
  id:           OntologyId          # "{EntityType}:{uuid}"
  entity_type:  EntityTypeId
  attrs:        HashMap<String, Value>   # 字段值
  valid_from:   DateTime            # 双时态
  valid_to:     Option<DateTime>
  tx_time:      DateTime
  version:      u64
  provenance:   Provenance          # 数据来源（source + ingest job）
  owner_id:     Option<UserId>      # 数据归属（影响 ReBAC 评估）
}
```

### OntologyRelationship（ABox 边）

```
OntologyRelationship {
  id:       RelationshipId
  from:     OntologyId
  to:       OntologyId
  rel_type: String                  # "MANAGES" / "BELONGS_TO" / "LINKED_TO"
  attrs:    HashMap                 # 边属性（权重、有效期等）
  valid_from: DateTime
  valid_to:   Option<DateTime>
}
```

---

## 4. Permission 聚合

### EntityTypePermission（EntityType 级，RBAC）

```
EntityTypePermission {
  entity_type_id: EntityTypeId
  role_id:        RoleId
  ops:            Vec<Operation>    # Read | Write | Delete | Admin
}

Operation {
  Read   # 查询对象
  Write  # 创建/更新对象
  Delete # 删除对象
  Admin  # 修改 EntityType Schema
}
```

### RelationshipRule（Object 级，ReBAC）

```
RelationshipRule {
  id:          RuleId
  name:        String
  description: String
  condition:   RebacCondition       # 图关系条件表达式
  effect:      Allow | Deny
  ops:         Vec<Operation>
  priority:    u32                  # 优先级，高优先级先评估
}

RebacCondition（示例）：
  SubjectManagesObject              # subject MANAGES object
  SubjectInSameDept                 # subject 和 object 同属一个 OU
  SubjectOwnsObject                 # subject.id == object.owner_id
  SubjectInGroup(GroupId)           # subject MEMBER_OF group
```

### AbacPolicy（Row 级，ABAC）

```
AbacPolicy {
  id:          PolicyId
  name:        String
  entity_type: Option<EntityTypeId>  # None = 全局策略
  condition:   AbacExpr              # CEL 表达式
  effect:      Allow | Deny
  priority:    u32
}

AbacExpr 示例（CEL）：
  "subject.departments.contains(object.attrs.department)"
  "subject.roles.contains('hr') && object.attrs.classification == 'PII'"
```

### FieldVisibilityMatrix（Field 级）

由 FieldDefinition.classification + Role 运行时计算，非持久化：

```
FieldVisibilityMatrix {
  role:           RoleId
  classification: FieldClassification
  visible:        bool
  masked:         bool               # true = 返回但脱敏（如 ***-1234）
}

默认矩阵：
  Role      Public  Internal  Confidential  PII
  public    见      不见      不见          不见
  analyst   见      见        不见          不见
  hr        见      见        见            见（加密后解密）
  manager   见      见        不见          不见
  admin     见      见        见            见
```

### AccessDecision（评估结果）

```
AccessDecision {
  request_id:    RequestId
  subject:       UserId
  object:        OntologyId
  operation:     Operation
  decision:      Allow | Deny | AllowWithMask
  hidden_fields: Vec<String>         # AllowWithMask 时生效
  masked_fields: Vec<String>         # 部分可见但脱敏的字段
  reason:        String              # 评估依据（审计用）
  evaluated_at:  DateTime
}
```

### AuditLog（访问审计，ADR-09）

```
AuditLog {
  id:         AuditId
  who:        UserId
  ip:         IpAddr
  what:       Operation
  target:     OntologyId
  fields:     Vec<String>            # 实际读取了哪些字段
  decision:   Allow | Deny
  timestamp:  DateTime
  session_id: SessionId
}
```

---

## 5. 聚合关系总图

```
User ──HAS_ROLE──────────────► Role
 │                               │
 ├──BELONGS_TO──► OrgUnit        │
 │                               │
 ├──MANAGES──────► User          │
 │                               ▼
 └──MEMBER_OF────► Group    EntityTypePermission
                              (role_id → EntityType + ops)
                                  │
                                  ▼
                             EntityType（TBox）
                              │         │
                              │         └──► FieldDefinition
                              │                (classification)
                              ▼
                         OntologyObject（ABox）
                              │
                              └──► OntologyRelationship
                                        │
                                        ▼
                                  RelationshipRule（ReBAC）
                                  AbacPolicy
                                        │
                                        ▼
                                  AccessDecision
                                        │
                                        ▼
                                    AuditLog
```

---

## 6. 评估顺序与短路规则

```
Step 1  EntityType 级（RBAC）
  → Role 无权限？→ Deny（短路）
  ↓ 通过
Step 2  Object 级（ReBAC）
  → 无匹配关系规则？→ Deny（短路）
  ↓ 通过
Step 3  Row 级（ABAC）
  → Policy 条件不满足？→ Deny（短路）
  ↓ 通过
Step 4  Field 级（Classification）
  → 计算 hidden_fields + masked_fields
  → 返回 AllowWithMask

最终：任意一步 Deny → 整体 Deny，不再继续评估
```

---

## 7. SurrealDB 存储映射

```
TBox（Schema）：
  entity_type 表 → EntityType + FieldDefinition（内嵌数组）+ EntityTypePermission

ABox（数据）：
  {entity_type} 表 → OntologyObject（每个 EntityType 独立表）
  relationship 表 → OntologyRelationship（RELATE 边）

权限配置：
  relationship_rule 表 → RelationshipRule
  abac_policy 表       → AbacPolicy
  audit_log 表         → AuditLog（append-only）

Identity：
  User 表 → User ABox 对象（复用 Ontology ABox 机制）
  role 表 → Role
  group 表 → Group
  org_unit 表 → OrganizationalUnit
```

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始领域模型 |
