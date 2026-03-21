# ADR-04: 多租户 — SaaS 组织层级模型

> 状态：✅ 已决策（原暂缓，本次补全）| 日期：2026-03-19
> 关联：ADR-26（四粒度权限）、ADR-33（模块化部署）

---

## 问题

SaaS 场景下，系统需要支持多个企业客户（租户）同时使用，且每个企业内部有复杂的组织结构：

```
企业 A（租户）
├── 市场部（部门）
│   ├── Alice（分析师，参与项目 X）
│   └── Bob（运营，参与项目 Y）
├── 法务部（部门）
│   └── Carol（法务，参与项目 X + 项目 Z）
└── 项目 X（跨部门协作）
    ├── Alice — Editor
    ├── Carol — Viewer
    └── 合同数据（只有项目成员能看）

企业 B（租户）—— 完全隔离，A 的任何人看不到 B 的任何数据
```

**三个核心需求：**
1. **租户隔离**：企业 A 和企业 B 的数据物理/逻辑完全隔离
2. **部门归属**：员工属于某个部门，数据可以归属于部门
3. **项目协作**：一个人可以参与多个项目，项目是跨部门的协作边界；项目里的数据只有项目成员能看
4. **企业主全览**：企业主能看到本企业所有部门、所有项目的所有数据

---

## 决策

**三层模型：Tenant（强隔离）→ Department（组织归属）→ Project（协作范围）**

这与 Palantir Foundry 官方模型完全对应：
- Tenant ↔ Palantir **Organization**（强制隔离，Mandatory Control）
- Department ↔ Palantir **OrgUnit**（组织归属，影响 ReBAC）
- Project ↔ Palantir **Project**（资源容器，自主控制，Discretionary Control）

---

## 1. 三层结构定义

```
┌────────────────────────────────────────────────────────────┐
│  Layer 1：Tenant（强制隔离层）                              │
│  每个企业客户是一个独立 Tenant，数据绝对不互通             │
│  实现：所有 DB 查询自动注入 tenant_id 过滤条件             │
└──────────────────────┬─────────────────────────────────────┘
                       │  一个 Tenant 内
        ┌──────────────┴──────────────┐
        ▼                             ▼
┌───────────────┐           ┌─────────────────────┐
│ Layer 2：      │           │ Layer 3：            │
│ Department     │           │ Project              │
│（组织归属）    │           │（协作范围）          │
│               │           │                     │
│ 树形结构       │           │ 资源容器             │
│ 影响 EnrichedId│           │ 成员有角色           │
│ 用于 ABAC 默   │           │ 数据归属 Project     │
│ 认过滤         │           │ 成员可跨部门         │
└───────────────┘           └─────────────────────┘
```

### Layer 1：Tenant（强制隔离）

```
Tenant {
  id:           TenantId            # 全局唯一
  name:         String              # 企业名称
  slug:         String              # URL 标识（如 acme → acme.palantir.io）
  edition:      Edition             # Lite / Standard / ...
  admin_user_id: UserId             # 企业主（TenantAdmin）
  status:       Active | Suspended | Trial
  trial_ends_at: Option<DateTime>
  created_at:   DateTime
}
```

**隔离实现：**
- NebulaGraph：每个 Tenant 一个独立 Space（`nebula://tenant_{id}`）
- TiDB：每个 Tenant 一个独立 Database schema（`palantir_tenant_{id}`）
- NATS Subject 前缀：`t.{tenant_id}.ontology.events.*`
- Redis 键前缀：`t:{tenant_id}:authz:...`
- RustFS Bucket：`tenant-{tenant_id}`

### Layer 2：Department（组织归属）

Department 复用已有 `OrganizationalUnit`，新增 `tenant_id`：

```
Department {
  id:        DeptId
  tenant_id: TenantId           # 归属租户
  name:      String
  parent_id: Option<DeptId>     # 树形，支持多级
  type:      Company | Division | Department | Team
}

图关系（NebulaGraph 内）：
  User -[BELONGS_TO]-> Department
  Department -[PARENT_OF]-> Department
```

**Department 的数据作用：**
默认 ABAC 策略（全租户生效）：
```
# 普通成员：只能看自己部门（及子部门）的数据
subject.departments.contains(object.attrs.dept_id)
  OR subject.roles.contains('tenant_admin')
  OR subject.roles.contains('dept_admin') AND subject.manages_depts.contains(object.attrs.dept_id)
```

### Layer 3：Project（协作范围）

```
Project {
  id:          ProjectId
  tenant_id:   TenantId
  name:        String
  description: String
  visibility:  Private | DeptShared | TenantShared
  owner_id:    UserId           # Project Owner
  created_at:  DateTime
}

ProjectMembership {
  project_id:  ProjectId
  user_id:     UserId
  role:        Owner | Editor | Viewer | Discoverer
  granted_by:  UserId
  granted_at:  DateTime
}

OntologyObject（扩展）：
  project_id:  Option<ProjectId>  # 归属项目（None = 租户共享数据）
```

**Project 的数据作用：**
- `project_id != null` 的对象：只有 Project 成员（+ TenantAdmin）能访问
- `project_id == null`（租户共享数据）：按 Department ABAC 过滤
- Discoverer：能看到 Project 存在，但看不到数据（可申请加入）

---

## 2. 角色层级

```
TenantAdmin（企业主）
  │  可看所有部门、所有项目的所有数据
  │  可管理所有用户、部门、项目、权限
  │
  ├── DeptAdmin（部门长）
  │     可看本部门（及子部门）的所有项目和数据
  │     可管理本部门成员的权限和项目加入
  │
  └── ProjectOwner（项目负责人）
        可管理项目内的成员（增减 Editor/Viewer）
        可看项目内所有数据
        │
        ├── ProjectEditor（项目成员-编辑）
        │     可读写项目内数据
        └── ProjectViewer（项目成员-只读）
              只读项目内数据
```

每个真人用户同时持有：
- **1 个组织归属**（BELONGS_TO 部门）
- **1 个 Persona**（Analyst / Data Engineer 等，决定功能入口）
- **N 个项目成员身份**（每个项目一个角色）

---

## 3. 四粒度权限评估（ADR-26）在三层模型中的位置

```
Step 0  Tenant 隔离（强制，在 DB 查询层）
  → tenant_id 不匹配 → 直接 404（对象在当前租户不存在）
  ↓
Step 1  EntityType RBAC（Persona 层）
  → 该 Persona 无此 EntityType 权限 → Deny
  ↓
Step 2  Project 成员资格（新增）
  → object.project_id != null AND user 不在此 Project → Deny
  → object.project_id == null → 跳过此步，进入 Step 3
  ↓
Step 3  ReBAC（图关系）
  → 对象级关系检查（MANAGES 等）
  ↓
Step 4  ABAC（部门过滤）
  → subject.departments ∩ object.dept_id（仅 project_id == null 的对象）
  ↓
Step 5  Field Classification
  → 计算 hidden_fields + masked_fields
```

---

## 4. EnrichedIdentity 扩展

新增 `tenant_id` 和 `project_memberships`：

```
EnrichedIdentity {
  tenant_id:           TenantId              # 必须
  user_id:             UserId
  is_tenant_admin:     bool                  # 跳过 Step 2-4 的快速路径
  static_roles:        Vec<RoleId>           # Persona 角色
  departments:         Vec<DeptId>           # 所属部门（含父部门）
  managed_depts:       Vec<DeptId>           # 作为 DeptAdmin 管理的部门
  manages:             Vec<OntologyId>       # MANAGES 关系
  project_memberships: HashMap<ProjectId, ProjectRole>  # 参与的项目及角色
}
```

---

## 5. 典型场景权限推导

### 场景 A：Alice（市场部 Analyst）查看项目 X 中的合同

```
tenant_id 匹配 ✅
RBAC：Analyst 有 Contract.Read ✅
Project：Alice 是 Project X 的 Editor ✅（project_id = X）
ReBAC：无 MANAGES 要求，跳过
ABAC：project_id != null，跳过部门过滤
Field：amount 字段 Confidential，Analyst 不可见 → hidden

结果：AllowWithMask（隐藏 amount 字段）
```

### 场景 B：Bob（法务部）查看项目 X 中的合同（Bob 不是 X 的成员）

```
tenant_id 匹配 ✅
RBAC：Bob 有 Contract.Read ✅
Project：object.project_id = X，Bob 不在 Project X → Deny

结果：Deny（返回 404，不泄露对象存在）
```

### 场景 C：TenantAdmin（企业主）查看任意对象

```
is_tenant_admin = true → 跳过 Step 2/3/4 所有过滤
Field：TenantAdmin 可见所有字段（包括 PII，需加密解密）

结果：Allow（全字段可见）
```

### 场景 D：Carol 同时参与项目 X（Viewer）和项目 Z（Editor）

```
查看 Project X 的对象 → ProjectRole = Viewer → 只读
查看 Project Z 的对象 → ProjectRole = Editor → 可读写
查看自己部门的非项目对象 → ABAC 部门过滤
```

---

## 6. 事件总线隔离

NATS Subject 规范：

```
t.{tenant_id}.ontology.events.{entity_type}.{action}
t.{tenant_id}.ontology.events.project.{project_id}.{action}

示例：
  t.acme.ontology.events.Contract.upsert
  t.acme.ontology.events.project.proj_x.member_added
```

各服务订阅时只订阅自己 Tenant 的 Subject，隔离 Workflow 触发和 Agent 预计算。

---

## 7. 与 ADR-26 的变更点

| 变更点 | ADR-26 原设计 | ADR-04 新增 |
|-------|-------------|------------|
| 隔离层 | 无租户概念 | Tenant 作为 Layer 0，所有查询前置过滤 |
| 数据归属 | 仅 ABAC 部门过滤 | 新增 Project 成员资格检查（Step 2）|
| EnrichedIdentity | 无 tenant_id | 新增 tenant_id + is_tenant_admin + project_memberships |
| Deny 的表现 | 403 Forbidden | 跨租户 → 404；跨 Project → 404（不泄露对象存在）|

---

## 待讨论

- [ ] 对于私有化部署（单租户），Tenant Layer 是否简化为 `tenant_id = "default"`（无多租户开销）？
- [ ] EntityType 是否可以跨租户共享（如"系统内置 EntityType"）？建议：系统 EntityType 属于 `tenant_id = "system"`
- [ ] 项目成员上限是否需要按 Edition 限制？

---

---

## 8. 存储层的多租户隔离（Iter-2 补充，2026-03-20）

### 8.1 背景与设计决策

平台采用**方案 B — 客户数据不出客户边界**：

- 客户（Tenant）使用自己的 S3 / RustFS bucket
- 平台在客户 bucket 内用 `platform_datasets/` 前缀管理版本化数据
- 版本元数据（schema、行数、manifest_path）存在平台 DB（SQLite → MySQL）

```
客户 bucket（mybucket1）
  raw/                          ← 客户原始数据（只读，平台不写）
    employees.csv
    products.csv
  platform_datasets/            ← 平台托管区（只有平台写）
    {tenant_id}/
      {dept_id}/                ← 部门级隔离（多部门时区分归属）
        {dataset_id}/
          v1/
            manifest.json       ← schema / 行数 / SHA256 / 时间戳
            data/
              part-00000.csv    ← 版本化快照（50k 行/part）
          v2/
            ...
```

> **当前占位（Iter-1/2）**：路径前缀暂用 `project_id`，等 `tenant_id` 落地后迁移。
> 迁移时只需修改 `write_to_platform_storage()` 的 `prefix` 参数，文件内容不变。

### 8.2 org_storage_configs 表

每个 Tenant 一条存储配置，AK/SK 加密存储（Iter-4 实现 AES-256-GCM 加密）：

```sql
CREATE TABLE org_storage_configs (
    tenant_id        TEXT PRIMARY KEY REFERENCES tenants(id),
    backend_type     TEXT NOT NULL DEFAULT 'local',  -- local | s3
    endpoint         TEXT,          -- RustFS/MinIO: http://host:port；AWS S3 留空
    bucket           TEXT,
    access_key_enc   TEXT,          -- AES-256-GCM 加密，Iter-4 前明文存储
    secret_key_enc   TEXT,
    region           TEXT DEFAULT 'us-east-1',
    path_prefix      TEXT DEFAULT 'platform_datasets',
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);
```

### 8.3 存储路径规范（完整版）

```
Phase 2a（Iter-1，当前）：  project_id 占位
  local: {PALANTIR_DATA_DIR}/{project_id}/{dataset_id}/v{version}/

Phase 2b（Iter-2，当前）：  S3 + project_id 占位
  s3:    {bucket}/platform_datasets/{project_id}/{dataset_id}/v{version}/

Phase 2b → ADR-04 落地后：  tenant_id + dept_id
  s3:    {bucket}/platform_datasets/{tenant_id}/{dept_id}/{dataset_id}/v{version}/
```

### 8.4 dataset_versions 字段变更

```sql
-- 已在 Iter-1 添加（idempotent）
ALTER TABLE dataset_versions ADD COLUMN manifest_path TEXT;

-- ADR-04 落地时补充
ALTER TABLE dataset_versions ADD COLUMN tenant_id TEXT;
ALTER TABLE dataset_versions ADD COLUMN dept_id   TEXT;
```

---

## 9. 数据集成角色（Ingest Workflow 专项）

数据集成工作流中，角色在 **Department + Project** 两个粒度都有体现：

### 9.1 角色定义

| 角色 | 层级 | 权限范围 |
|------|------|---------|
| `tenant_admin` | Tenant | 管理所有部门、所有项目、存储配置、用户 |
| `dept_admin` | Department | 管理本部门成员权限；可查看本部门所有数据集 |
| `data_engineer` | Project | 创建/配置数据源、触发同步、查看版本历史 |
| `viewer` | Project | 只读查看数据集记录、版本列表 |
| `approver` | Project | 审核数据集版本发布（Iter-3+，Breaking schema 变更时触发） |

### 9.2 数据集成场景的权限推导

```
场景：data_engineer 触发 sync（POST /api/sources/:id/sync）

Step 0  tenant_id 匹配 ✅
Step 2  Project 成员资格：user 是 Project 的 data_engineer ✅
Step 5  操作权限：data_engineer 有 DataSource.Sync 权限 ✅
结果：允许

场景：viewer 尝试触发 sync

Step 2  Project 成员资格：user 是 Project 的 viewer ✅
Step 5  操作权限：viewer 无 DataSource.Sync 权限 → Deny
结果：403 Forbidden
```

### 9.3 存储配置管理权限

```
org_storage_configs 读写：仅 tenant_admin
数据源 S3 config 明文字段（AK/SK）：
  - 写入时：只有 data_engineer（配置者）和 tenant_admin 可见
  - 读取时：AK/SK 对 viewer 不可见（Iter-4 加密后由平台代理请求）
```

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始占位，暂缓 |
| v2.0 | 2026-03-19 | 全面落地：三层模型（Tenant/Department/Project），四粒度权限扩展，EnrichedIdentity 扩展，事件隔离规范 |
| v2.1 | 2026-03-20 | 补充存储层多租户隔离（§8）、数据集成角色（§9）、org_storage_configs 表、路径规范演进路线 |
