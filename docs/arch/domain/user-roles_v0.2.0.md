# Palantir — 用户角色体系 v0.2.0

> 版本：v0.2.0 | 日期：2026-03-19
> 关联：ADR-26（四粒度权限）、ontology-permission-domain_v0.1.0.md
> 参考：Palantir Foundry 官方文档 — User Personas + Projects & Roles

---

## 一、Palantir 官方角色模型：两个正交维度

Palantir 官方把"角色"拆成**两个独立维度**，我们的系统完全复用这个设计：

```
维度 1 — 资源权限等级（Permission Level，RBAC 基础）
  Owner      → 最高权限，可转授任意等级
  Editor     → 可修改资源，可转授 Editor / Viewer / Discoverer
  Viewer     → 只读，可转授 Viewer / Discoverer
  Discoverer → 最低，仅可发现资源存在（看名称，不看内容）

维度 2 — 用户职能角色（Persona，组织职责）
  Platform Administrator  → 平台运维与配置
  Data Engineer           → 数据接入与管道
  Application Builder     → 应用与逻辑构建
  Analyst                 → 数据分析与消费
  Data Scientist          → 模型训练与推理
  Data Governance         → 数据治理与合规
```

> **关键设计**：Permission Level 决定"能做什么操作"，Persona 决定"通常在哪些资源上工作"。
> 一个用户持有一个或多个 Persona，每个 Persona 在不同资源类型上有预设的 Permission Level。

---

## 二、我们系统采用的 Persona（职能角色）

> 精简 Palantir 官方 6 种，结合我们的业务域，定为以下 6 种。

---

### Persona 1：Platform Administrator（平台管理员）

```
系统标识：platform_admin
对应官方：Platform Administrator
```

**是谁：** 负责平台本身运行的 IT / 运维人员，不是数据消费者。

**核心职责：**
- 管理用户账号、角色分配、组织单元（OrgUnit）
- 配置全局安全策略（Token 有效期、合规标记）
- 查看全量审计日志
- 监控服务健康（可观测性、告警）
- 管理部署配置（DeploymentProfile）

**在资源上的权限：**
| 资源类型 | 权限等级 |
|---------|---------|
| User / Role / Group / OrgUnit | Owner |
| AuditLog | Viewer |
| System Config / DeploymentProfile | Owner |
| 业务数据（EntityType / Object）| Discoverer（默认不看业务数据）|

---

### Persona 2：Data Engineer（数据工程师）

```
系统标识：data_engineer
对应官方：Data Engineer
```

**是谁：** 负责数据入湖的工程师，Palantir 官方定义为"构建高质量、高频更新数据集"的人。

**核心职责：**
- 定义 EntityType Schema（TBox）
- 注册外部数据源（DataSource）
- 配置字段映射（FieldMapping）
- 触发 / 监控 / 调试摄入任务（IngestJob）
- 配置字段分类（Classification）和保留策略（RetentionPolicy）

**在资源上的权限：**
| 资源类型 | 权限等级 |
|---------|---------|
| EntityType（Schema）| Owner |
| DataSource / FieldMapping | Owner |
| IngestJob / IngestCursor | Owner |
| OntologyObject（ABox）| Editor（通过 ingest 写入）|
| AuditLog | 无 |

---

### Persona 3：Application Builder（应用构建者）

```
系统标识：app_builder
对应官方：Application Builder
```

**是谁：** Palantir 官方定义为"基于 Ontology 层为最终用户构建应用"的开发者，我们这里对应函数/流程的开发者。

**核心职责：**
- 注册 / 调试 Function（Rust / CEL / NL）
- 设计 Workflow（步骤 + 触发器）
- 配置外部 API 集成（OutboundConfig）
- 查看 Function 执行历史、Workflow 运行记录
- 测试 Agent 工具调用链

**在资源上的权限：**
| 资源类型 | 权限等级 |
|---------|---------|
| FunctionDefinition | Owner |
| WorkflowDefinition | Owner |
| OutboundConfig | Owner |
| FunctionExecution / WorkflowExecution | Viewer |
| OntologyObject | Viewer（只读，用于测试）|

---

### Persona 4：Analyst（分析师）

```
系统标识：analyst
对应官方：Analyst
```

**是谁：** Palantir 官方定义为"探索数据集、进行开放式分析、产出报告"的人。是 Agent 的主要使用者。

**核心职责：**
- 通过 Agent 对话提自然语言问题
- 浏览 Ontology 图结构（只读）
- 过滤、查询 OntologyObject
- 导出分析结果

**在资源上的权限：**
| 资源类型 | 权限等级 |
|---------|---------|
| OntologyObject（被授权类型）| Viewer（Internal 及以下字段）|
| Ontology 图关系 | Viewer |
| Agent 会话 / 历史 | Owner（自己的）|
| Schema（EntityType）| Discoverer |

---

### Persona 5：Data Scientist（数据科学家）

```
系统标识：data_scientist
对应官方：Data Scientist
```

**是谁：** 开发 / 部署机器学习模型，将分析结果集成回 Ontology 的人。在我们系统里偏向"向量化、嵌入、语义模型"方向。

**核心职责：**
- 上传文件 / 数据集，触发向量化
- 配置 Embedding 模型策略（本地 ONNX 模型选择）
- 调试 Agent 的语义缓存与 Memory
- 注册 NL（自然语言）类型的 Function

**在资源上的权限：**
| 资源类型 | 权限等级 |
|---------|---------|
| OntologyObject（Document / Chunk）| Editor |
| FunctionDefinition（NL 类型）| Owner |
| AgentMemory | Owner（自己的）|
| EmbeddingConfig | Editor |

---

### Persona 6：Data Governance（数据治理）

```
系统标识：data_governance
对应官方：Data Governance
```

**是谁：** Palantir 官方定义为"领导数据安全流程和监督，保护敏感数据"的人。对应我们的合规 + 权限策略管理。

**核心职责：**
- 审查 / 配置字段分类（Classification Marking）
- 定义 ABAC 策略（行级过滤规则）
- 定义 ReBAC 规则（关系型访问控制规则）
- 查看审计日志（AuditLog）
- 导出合规报告
- 为 EntityType 配置角色权限（RBAC EntityTypePermission）

**在资源上的权限：**
| 资源类型 | 权限等级 |
|---------|---------|
| FieldClassification / AbacPolicy / RelationshipRule | Owner |
| EntityTypePermission（权限配置）| Owner |
| AuditLog | Viewer（只读，WORM 保护）|
| OntologyObject（内容）| Viewer（验证策略时需要看数据）|

---

## 三、资源权限等级（Permission Level）详细定义

> 应用于每一种资源类型（EntityType / DataSource / FunctionDef 等）

| 等级 | 可执行操作 | 可转授等级 |
|------|----------|----------|
| **Owner** | 创建、读、写、删除、修改权限配置、转授 | Owner / Editor / Viewer / Discoverer |
| **Editor** | 创建、读、写（不能删除、不能修改权限配置）| Editor / Viewer / Discoverer |
| **Viewer** | 只读（查看所有字段，受 Field Classification 约束）| Viewer / Discoverer |
| **Discoverer** | 知道资源存在（只看名称/ID），不能看内容 | Discoverer |

> **叠加 ADR-26 四粒度：** Permission Level 是 RBAC 层（EntityType 级）的操作符。
> Viewer 通过 RBAC 检查后，还会经过 ReBAC → ABAC → Field Classification 进一步限制。

---

## 四、Persona × 资源类型 全矩阵

> O=Owner　E=Editor　V=Viewer　D=Discoverer　—=无权限

| 资源类型 | Plat Admin | Data Engineer | App Builder | Analyst | Data Scientist | Data Governance |
|---------|:----------:|:-------------:|:-----------:|:-------:|:--------------:|:---------------:|
| User / Role / Group / OrgUnit | O | — | — | — | — | — |
| EntityType Schema | D | O | D | D | D | O（策略配置）|
| FieldClassification / AbacPolicy | — | E（定义分类）| — | — | — | O |
| DataSource / FieldMapping | — | O | — | — | — | — |
| IngestJob / IngestCursor | — | O | — | — | — | — |
| OntologyObject（业务数据）| D | E | V | V | E（Document）| V |
| OntologyRelationship | — | E | — | V | — | V |
| FunctionDefinition | — | — | O | — | O（NL）| — |
| FunctionExecution | — | — | V | — | V | — |
| OutboundConfig | — | — | O | — | — | — |
| WorkflowDefinition | — | — | O | — | — | — |
| WorkflowExecution | — | V（摄入任务）| V | — | — | — |
| AgentSession / Message | — | — | V（测试）| O（自己）| O（自己）| — |
| AgentMemory | — | — | — | O（自己）| O（自己）| — |
| AgentTrace | — | — | V | V（自己）| V（自己）| V |
| AuditLog | V（全量）| — | — | — | — | V（只读）|
| System Config | O | — | — | — | — | — |

---

## 五、System Account（服务账号）

服务账号不是真人，无 Persona，仅用于内部服务间 gRPC 调用：

| 账号标识 | 绑定服务 | 权限说明 |
|---------|---------|---------|
| `svc_ingest` | ingest-svc | 所有 EntityType 的 Editor（写入 OntologyObject）|
| `svc_agent` | agent-svc | **无独立权限**，完全透传调用者的 user_id |
| `svc_workflow` | workflow-svc | Workflow 定义中声明的 EntityType Editor |
| `svc_function` | function-svc | 继承调用方身份（Agent 调用 → 用户身份；Workflow 调用 → workflow 账号）|

---

## 六、Persona 对应的前端入口（页面可见性）

| Persona | 可见页面 | 入口说明 |
|---------|---------|---------|
| Platform Admin | 用户管理、角色管理、系统设置、审计日志、服务健康 | 独立管理控制台 |
| Data Engineer | Schema 管理、数据源管理、摄入任务、Ontology 图（Schema 视图）| 数据工程工作台 |
| App Builder | Function 编辑器（Monaco）、Workflow 设计器（React Flow）、执行历史 | 开发者工作台 |
| Analyst | Agent 对话、Ontology 图（数据视图）、数据查询与导出 | 分析工作台 |
| Data Scientist | 文件上传、Embedding 管理、Agent Memory 配置 | AI 工作台 |
| Data Governance | 权限策略配置、字段分类管理、审计日志、合规报告 | 治理工作台 |

---

## 七、待确认

- [ ] `Analyst` 默认只有 Viewer 权限。是否某些业务场景下 Analyst 需要写数据？（如打标签、添加备注）
- [ ] `Data Scientist` 和 `App Builder` 职责有重叠（都能写 Function）。是否合并为一个 Persona？
- [ ] `Data Governance` 是单独的人，还是 `Platform Admin` 的一部分职责？（Palantir 官方把两者分开）
- [ ] `Operator`（运维监控）职能是否需要独立 Persona，还是归入 `Platform Admin`？

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本（自定义 9 个角色）|
| v0.2.0 | 2026-03-19 | 对齐 Palantir 官方：两维度模型（Permission Level × Persona）；精简为 6 个 Persona + 4 级权限等级；补充 System Account |
