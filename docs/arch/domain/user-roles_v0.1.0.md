# Palantir — 用户角色体系

> 版本：v0.1.0 | 日期：2026-03-19
> 关联：ADR-26（四粒度权限）、ontology-permission-domain_v0.1.0.md

---

## 一、角色分类原则

系统角色按两个维度划分：

```
维度 A：是否真人操作
  Human Role   — 真实用户，通过前端登录操作
  System Role  — 内部服务账号，程序调用，无前端界面

维度 B：职责范围
  平台治理层   — 管理平台本身（Schema / 用户 / 权限 / 部署）
  数据生产层   — 负责数据进入 Ontology（摄入 / 映射 / 清洗）
  数据消费层   — 基于数据做分析、提问、报告
  技术开发层   — 开发 Function / Workflow / 集成
  合规监管层   — 只读审计，不能修改任何数据
```

---

## 二、Human Roles（真人角色）

### R-01 平台超级管理员（Platform Admin）

```
英文标识：platform_admin
典型人物：IT 部门负责人 / 平台运维负责人
人数：    极少（1–3 人），高度受限
```

**职责：**
- 管理所有用户账号（创建 / 停用 / 重置密码）
- 创建和管理角色、用户组、组织单元
- 查看全平台审计日志
- 配置全局参数（Token 有效期、合规策略、部署配置）
- 紧急情况下强制吊销任意用户 Token

**不能做：**
- 直接读取业务数据（需要另外赋予数据角色）
- 绕过审计记录操作

**权限模型映射：**
- RBAC：`platform_admin` 角色，对 EntityType:User / Role / OrgUnit 有 Admin 权限
- 不涉及 ReBAC / ABAC（这两层是业务数据层权限）

---

### R-02 数据管理员（Data Admin）

```
英文标识：data_admin
典型人物：数据架构师 / 数据治理专员
人数：    少（3–10 人）
```

**职责：**
- 定义 / 更新 EntityType Schema（TBox）
- 设置字段分类（Public / Internal / Confidential / PII）
- 注册外部数据源（DataSource）
- 配置字段映射（FieldMapping）
- 触发 / 暂停 / 监控摄入任务（IngestJob）
- 配置数据保留策略（RetentionPolicy）

**不能做：**
- 管理用户账号和角色（那是 Platform Admin 的职责）
- 直接修改业务对象数据（只能管理 Schema 和摄入）

**权限模型映射：**
- RBAC：对所有 EntityType 有 Admin 权限（Schema 级操作）
- RBAC：对 DataSource / FieldMapping 有 Write 权限

---

### R-03 业务分析师（Analyst）

```
英文标识：analyst
典型人物：数据分析师 / 商业智能工程师 / 报表人员
人数：    中等（10–50 人）
```

**职责：**
- 查询指定 EntityType 的 OntologyObject（Read）
- 使用 Agent 提自然语言问题
- 查看 Internal 及以下字段（Confidential / PII 不可见）
- 浏览 Ontology 图结构（只读）
- 导出查询结果

**不能做：**
- 创建 / 修改 / 删除业务数据
- 查看 Confidential / PII 字段
- 管理 Schema / 数据源

**权限模型映射：**
- RBAC：对被授权 EntityType 有 Read 权限
- ABAC：行级过滤（只能看与自己部门相关的数据）
- Field：最高可见 Internal

---

### R-04 业务用户（Business User）

```
英文标识：business_user
典型人物：普通员工 / 业务经理 / 客户服务人员
人数：    大（50–500 人）
```

**职责：**
- 查看自己直接管理的对象（ReBAC MANAGES 关系）
- 使用 Agent 提问（受权限限制）
- 查看 Public 字段
- 上传文件（Document 类型）

**不能做：**
- 看 Internal 及以上字段（除非有额外角色授权）
- 查看他人数据（除非显式被 MANAGES 关系覆盖）
- 修改 Schema 或业务数据

**权限模型映射：**
- RBAC：对部分 EntityType 有 Read 权限（仅 Public 字段）
- ReBAC：MANAGES 关系赋予对被管理对象的读权限
- Field：最高可见 Public

---

### R-05 HR 专员（HR Officer）

```
英文标识：hr
典型人物：人力资源专员 / HRBP
人数：    少（5–20 人）
```

**职责：**
- 读写 Employee 类型的 OntologyObject
- 可以查看 PII 字段（如身份证号、手机号），字段加密后解密
- 管理组织关系（BELONGS_TO / MANAGES 边）
- 使用 Agent 查询员工相关数据

**不能做：**
- 查看非 HR 域的 Confidential 数据（如合同金额）
- 管理 Schema 或权限配置

**权限模型映射：**
- RBAC：对 Employee EntityType 有 Read + Write 权限
- ABAC：`subject.roles.contains('hr') && object.attrs.type == 'employee'`
- Field：可见 PII（hr 角色 + Employee EntityType 条件下解密）

---

### R-06 法务 / 合规专员（Legal Officer）

```
英文标识：legal
典型人物：法务人员 / 合规官
人数：    少（2–10 人）
```

**职责：**
- 读写 Contract 类型数据
- 查看合同相关 Confidential 字段（如金额）
- 使用 Agent 查询合同相关问题

**权限模型映射：**
- RBAC：对 Contract EntityType 有 Read + Write 权限
- Field：可见 Confidential（仅 Contract 域）

---

### R-07 技术开发者（Developer）

```
英文标识：developer
典型人物：后端开发工程师 / 平台集成开发者
人数：    少（5–20 人）
```

**职责：**
- 注册 / 修改 / 测试 Function（Rust / CEL / NL）
- 设计 / 调试 Workflow 及触发器
- 配置外部 API 集成（OutboundConfig）
- 查看 Function 执行历史和 Workflow 运行记录
- 调用 Agent（用于测试 Function 工具调用链路）

**不能做：**
- 管理用户账号和角色
- 直接修改业务 OntologyObject（只能通过 Function 间接操作）

**权限模型映射：**
- RBAC：对 FunctionDef / WorkflowDef / OutboundConfig 有 Write 权限
- RBAC：对 FunctionExecution / WorkflowExecution 有 Read 权限

---

### R-08 运维操作员（Operator）

```
英文标识：operator
典型人物：DevOps 工程师 / SRE / 值班运维
人数：    少（3–10 人）
```

**职责：**
- 监控 Workflow 执行列表、状态、耗时
- 手动终止卡住的 Workflow 实例
- 监控摄入任务状态、重试失败任务
- 查看系统健康状态（服务发现 / 配置中心）
- 查看 Embedding 服务可用性

**不能做：**
- 修改业务数据 / Schema / Function / Workflow 定义
- 查看业务数据内容（只看运行状态元数据）

**权限模型映射：**
- RBAC：对 WorkflowExecution / IngestJob 有 Read + 操作（Cancel/Retry）权限
- 无业务数据读权限

---

### R-09 合规审计员（Compliance Auditor）

```
英文标识：compliance_auditor
典型人物：内审人员 / 外部审计机构账号
人数：    极少（1–5 人）
```

**职责：**
- 只读审计日志（AuditLog）
- 只读数据访问记录（谁、何时、读了什么字段）
- 导出审计报告（CSV / PDF）
- 查看 Schema 历史变更记录

**不能做：**
- 修改任何数据（包括审计日志本身，WORM 保护）
- 查看业务对象实际内容

**权限模型映射：**
- RBAC：对 AuditLog 有 Read 权限（只读）
- 无其他数据访问权限

---

## 三、System Roles（服务账号，非真人）

| 标识 | 服务 | 用途 | 权限范围 |
|------|------|------|---------|
| `svc_ingest` | ingest-svc | 代表系统写入摄入数据 | 所有 EntityType 的 Write 权限（受 FieldMapping 约束）|
| `svc_agent` | agent-svc | 代理用户查询，透传用户身份 | **无独立权限**，每次携带原始 user_id，以用户身份评估 |
| `svc_workflow` | workflow-svc | 执行 Workflow 步骤写回结果 | 指定 EntityType 的 Write 权限（Workflow 定义中声明）|
| `svc_function` | function-svc | 执行 Function 内的 Ontology 查询 | **继承调用方身份**（Agent 调用 → 用户身份；Workflow 调用 → workflow 身份）|

> **关键规则：** `svc_agent` 没有独立数据权限。它只是透传通道，所有权限评估以**真实用户 user_id** 为主体。

---

## 四、角色关系与层级

```
平台治理层                    数据消费层
  │                             │
  ├── Platform Admin            ├── Analyst          ← 最高 Internal 字段
  └── Data Admin                ├── Business User    ← 最高 Public 字段
                                ├── HR Officer       ← 可见 PII（仅 Employee）
                                └── Legal Officer    ← 可见 Confidential（仅 Contract）

技术开发层                    合规监管层
  │                             │
  └── Developer                 ├── Operator         ← 看运行状态，不看业务数据
                                └── Compliance Auditor ← 只看审计日志
```

**角色叠加原则：** 一个真实用户可以同时持有多个角色（如 `analyst + hr`），权限取最宽松的并集。

---

## 五、角色 × 功能模块权限矩阵

> ✅ = 完全权限　　▶ = 有限权限（见备注）　　— = 无权限

| 功能模块 | Plat Admin | Data Admin | Analyst | Biz User | HR | Legal | Developer | Operator | Auditor |
|---------|:----------:|:----------:|:-------:|:--------:|:--:|:-----:|:---------:|:--------:|:-------:|
| 用户 / 角色管理 | ✅ | — | — | — | — | — | — | — | — |
| EntityType Schema 管理 | — | ✅ | — | — | — | — | — | — | — |
| 字段分类配置 | — | ✅ | — | — | — | — | — | — | — |
| 摄入数据源 / 映射 / 任务 | — | ✅ | — | — | — | — | — | ▶ 监控 | — |
| OntologyObject 读（Public）| ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — | — |
| OntologyObject 读（Internal）| ✅ | ✅ | ✅ | — | ✅ | ✅ | — | — | — |
| OntologyObject 读（Confidential）| ✅ | ✅ | — | — | — | ▶ 合同域 | — | — | — |
| OntologyObject 读（PII）| ✅ | ✅ | — | — | ▶ 员工域 | — | — | — | — |
| OntologyObject 写 | — | ✅ | — | ▶ 自有 | ▶ 员工域 | ▶ 合同域 | — | — | — |
| Ontology 关系（Link）管理 | — | ✅ | — | — | ▶ 员工域 | — | — | — | — |
| Agent 对话 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| Function 注册 / 编辑 | — | — | — | — | — | — | ✅ | — | — |
| Function 执行历史 | — | — | — | — | — | — | ✅ | — | — |
| Workflow 定义 / 设计 | — | — | — | — | — | — | ✅ | — | — |
| Workflow 执行监控 / 终止 | — | — | — | — | — | — | ▶ 只读 | ✅ | — |
| 审计日志 | ✅ 全量 | — | — | — | — | — | — | — | ✅ 只读 |
| 系统健康 / 服务发现 | ✅ | — | — | — | — | — | — | ✅ | — |

---

## 六、角色对应的前端入口

| 角色 | 主要使用页面 | 不可见页面 |
|------|------------|----------|
| Platform Admin | 用户管理、角色管理、审计日志、系统设置 | Agent 对话（可选）|
| Data Admin | Schema 管理、数据源管理、摄入任务、Ontology 图 | 审计日志、系统健康 |
| Analyst | Agent 对话、Ontology 图（只读）、数据查询 | 管理类所有页面 |
| Business User | Agent 对话、我的数据（ReBAC 范围）| 管理类、Schema 类所有页面 |
| HR Officer | Agent 对话、员工数据管理（写）、组织关系图 | 合同域、Schema 管理 |
| Legal Officer | Agent 对话、合同数据管理（写）| 员工 PII、Schema 管理 |
| Developer | Function 编辑器、Workflow 设计器、执行历史 | 用户管理、业务数据（直接）|
| Operator | Workflow 监控、摄入任务监控、系统健康 | 业务数据、Schema、Function 编辑 |
| Compliance Auditor | 审计日志查看、导出 | 几乎所有功能页面 |

---

## 七、待确认问题

- [ ] 是否需要**租户管理员**（多租户 ADR-04 暂缓，待确认）
- [ ] `HR Officer` 和 `Legal Officer` 是内置系统角色，还是由 `Data Admin` 在后台自定义？（推荐后者，更灵活）
- [ ] `Business User` 是否应该能直接写自己的数据（如更新自己的联系方式），还是必须通过 Workflow？
- [ ] `Developer` 是否需要沙箱环境执行 Function（避免测试代码影响生产数据）？

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本：9 个 Human Role + 4 个 System Role，权限矩阵，前端入口映射 |
