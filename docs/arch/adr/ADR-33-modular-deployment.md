# ADR-33: 模块化部署与产品版本分层

> 状态：✅ 已决策 | 日期：2026-03-19
> 关联：ADR-28（可插拔基础设施）、ADR-29（服务发现）

---

## 问题

不同客户对功能的需求差异巨大：

```
客户 A（小型团队）    → 只需 Ontology 数据治理，不需要 AI
客户 B（中型企业）    → 需要数据接入 + 基础自动化，不需要 AI Agent
客户 C（大型企业）    → 需要 AI 查询 + 工作流，但不需要自定义函数
客户 D（全功能）      → 需要完整平台（所有服务）
客户 E（私有定制）    → 在完整平台基础上接入私有 ERP、定制 UI、专属模型
```

现有 ADR-28 解决了"**基础设施怎么换**"（存储/消息/缓存插拔），但没有解决"**哪些服务需要部署**"。

两个层次的问题需要分开设计：
- **Infrastructure Pluggability**（ADR-28）：部署了相同的服务，但底层存储/消息不同
- **Module Pluggability**（本 ADR）：不同客户部署的服务集合本身不同

---

## 决策

**以 `ModuleProfile` 控制服务启停，以 `ProductEdition` 定义官方版本组合，以 `ExtensionPoint` 支持私有定制。三者独立配置，可自由组合。**

---

## 1. 模块依赖图

```
必选核心（Core）
  ┌─────────────────────────────────────────────┐
  │  api-gateway  ←→  auth-svc  ←→  ontology-svc│
  └─────────────────────────────────────────────┘
               ↑ 所有可选模块都依赖 Core

可选模块（Optional）及依赖关系：

  ingest-svc ────────────────→ ontology-svc (写)
                               [无其他依赖]

  function-svc ──────────────→ ontology-svc (读/写)
                               [无其他依赖]

  embedding-svc ──────────────[无服务依赖，纯计算]

  agent-svc ─────────────────→ function-svc (工具调用，硬依赖)
              ┄ ┄ ┄ ┄ ┄ ┄ ┄ ┄→ embedding-svc (语义缓存，软依赖，可降级)
              ────────────────→ ontology-svc (数据读取)

  workflow-svc ──────────────→ function-svc (步骤执行，硬依赖)
               ────────────────→ ontology-svc (结果写回)
```

**依赖规则：**

| 启用模块 | 必须同时启用 | 可选（降级运行）|
|---------|------------|---------------|
| `ingest-svc` | — | — |
| `function-svc` | — | — |
| `embedding-svc` | — | — |
| `agent-svc` | `function-svc` | `embedding-svc`（禁用时跳过语义缓存）|
| `workflow-svc` | `function-svc` | — |

> 启动时 `api-gateway` 校验依赖规则，违反时拒绝启动并输出明确错误信息。

---

## 2. ModuleProfile 配置格式

在 `deployment.toml` 中新增 `[modules]` 节（与现有 `[infrastructure]` 并列）：

```toml
[modules]
ingest_svc    = { enabled = true,  replicas = 2 }
function_svc  = { enabled = true,  replicas = 1 }
embedding_svc = { enabled = true,  replicas = 2 }
agent_svc     = { enabled = true,  replicas = 1 }
workflow_svc  = { enabled = false }            # 本次部署不启用

[modules.extensions]                           # 私有定制模块（详见第 5 节）
custom_erp_connector = { enabled = true, image = "registry.acme.com/erp-module:1.0" }
```

`api-gateway` 启动时读取 `ModuleProfile`，只注册已启用服务的路由，未启用的路由返回 `501 Not Implemented { "code": "MODULE_DISABLED" }`。

---

## 3. 产品版本（ProductEdition）

> ProductEdition = ModuleProfile 的官方命名组合，用于销售和授权。
> 技术上 ModuleProfile 可以任意组合，ProductEdition 是对外的"套餐"概念。

```
┌────────────────────────────────────────────────────────────────────┐
│  Edition Lite                                                       │
│  模块：api-gateway + auth-svc + ontology-svc                       │
│  场景：仅需结构化数据治理、Ontology 图管理、权限控制               │
│  代表用户：Data Engineer + Data Governance                          │
└────────────────────────────────────────────────────────────────────┘
         ↓ + ingest-svc + function-svc
┌────────────────────────────────────────────────────────────────────┐
│  Edition Standard                                                   │
│  模块：Lite + ingest-svc + function-svc                            │
│  场景：数据接入自动化 + 基础 Function 逻辑执行（含外部 API 集成）  │
│  代表用户：Data Engineer + Application Builder                      │
└────────────────────────────────────────────────────────────────────┘
         ↓ + embedding-svc + agent-svc
┌────────────────────────────────────────────────────────────────────┐
│  Edition Professional                                               │
│  模块：Standard + embedding-svc + agent-svc                        │
│  场景：AI 自然语言查询，语义缓存，AgentMemory，文件向量化          │
│  代表用户：全部 Persona（含 Analyst + Data Scientist）             │
└────────────────────────────────────────────────────────────────────┘
         ↓ + workflow-svc
┌────────────────────────────────────────────────────────────────────┐
│  Edition Enterprise                                                 │
│  模块：Professional + workflow-svc                                  │
│  场景：完整平台，事件驱动自动化，Saga 补偿，多触发器编排           │
│  代表用户：全部 6 个 Persona                                       │
└────────────────────────────────────────────────────────────────────┘
         ↓ + ExtensionModules + 私有定制
┌────────────────────────────────────────────────────────────────────┐
│  Edition Enterprise+                                                │
│  模块：Enterprise + 客户自定义扩展模块                             │
│  场景：私有 ERP 集成、专属 LLM 模型、定制 UI、专有数据连接器       │
│  代表用户：签署定制合同的大客户                                    │
└────────────────────────────────────────────────────────────────────┘
```

### 版本功能矩阵

| 功能 | Lite | Standard | Professional | Enterprise | Enterprise+ |
|------|:----:|:--------:|:------------:|:----------:|:-----------:|
| Ontology 图管理（TBox/ABox）| ✅ | ✅ | ✅ | ✅ | ✅ |
| 四粒度权限（RBAC/ReBAC/ABAC/Field）| ✅ | ✅ | ✅ | ✅ | ✅ |
| 审计日志 | ✅ | ✅ | ✅ | ✅ | ✅ |
| 数据源注册与摄入 | — | ✅ | ✅ | ✅ | ✅ |
| Function（CEL/NL/Rust）| — | ✅ | ✅ | ✅ | ✅ |
| 外部 API 集成（outbound）| — | ✅ | ✅ | ✅ | ✅ |
| AI 自然语言查询（Agent）| — | — | ✅ | ✅ | ✅ |
| 语义缓存 + AgentMemory | — | — | ✅ | ✅ | ✅ |
| 文件上传 + 向量化 | — | — | ✅ | ✅ | ✅ |
| Workflow 编排 | — | — | — | ✅ | ✅ |
| Saga 补偿 | — | — | — | ✅ | ✅ |
| 私有 LLM / 本地模型替换 | — | — | ▶ 标准配置 | ▶ 标准配置 | ✅ 完整替换 |
| 自定义扩展模块 | — | — | — | — | ✅ |
| 白标 UI（品牌定制）| — | — | — | — | ✅ |
| 专属 SLA + 支持 | — | — | — | ▶ 标准 | ✅ 专属 |

---

## 4. Gateway 模块感知路由

`api-gateway` 是唯一感知 `ModuleProfile` 的服务，其他服务无需关心哪些模块被启用。

```rust
// api-gateway 启动时：
fn build_router(profile: &ModuleProfile) -> Router {
    let mut router = Router::new()
        .merge(auth_routes())      // 永远注册（Core）
        .merge(ontology_routes()); // 永远注册（Core）

    if profile.is_enabled("ingest_svc") {
        router = router.merge(ingest_routes());
    }
    if profile.is_enabled("function_svc") {
        router = router.merge(function_routes());
    }
    if profile.is_enabled("agent_svc") {
        router = router.merge(agent_routes());
    }
    if profile.is_enabled("workflow_svc") {
        router = router.merge(workflow_routes());
    }
    // 未注册的路由统一返回 501
    router.fallback(module_disabled_handler)
}
```

**前端感知：**
- `api-gateway` 暴露 `GET /meta/modules`，返回已启用模块列表
- 前端启动时调用此接口，根据结果隐藏/显示对应工作台入口
- 各 Persona 的侧边栏菜单项按模块可用性动态渲染

---

## 5. Enterprise+ 私有定制扩展点

私有定制分四类扩展点，互相独立：

### 5.1 自定义 Function 插件（最常见）

> 场景：客户有私有 ERP/CRM，需要 Function 直接调用内部 gRPC 接口（不走 HTTP）

```
标准方式：Function → outbound HTTP → 外部 API
定制方式：Function 内嵌客户私有 Rust crate，直接 gRPC 调用内网服务
```

实现：`function-svc` 支持 `PluginRuntime` Trait，客户提供编译好的 `.so` 动态库或独立 gRPC sidecar。

```toml
[modules.extensions.erp_function_plugin]
enabled  = true
runtime  = "grpc_sidecar"
endpoint = "localhost:50055"   # 客户私有 Function sidecar
```

### 5.2 自定义存储实现（ADR-28 扩展）

> 场景：客户使用 OceanBase / 达梦 DB / TDengine，不在标准 Trait 实现列表中

客户提供实现了对应 Trait 的 Rust crate，通过 Cargo workspace 集成，在 `deployment.toml` 中激活：

```toml
[infrastructure.structured_store]
provider  = "custom"
crate     = "palantir-oceanbase-adapter"   # 客户提供
```

### 5.3 自定义 LLM / Embedding 模型

> 场景：金融/军工客户不能使用公有 LLM API，需要接入私有部署的大模型（如内部 ChatGLM / Qwen）

```toml
[ai]
llm_provider    = "openai_compatible"   # 所有兼容 OpenAI API 的模型均可接入
llm_base_url    = "https://llm.internal.acme.com/v1"
llm_api_key     = "${SECRET_LLM_KEY}"
llm_model       = "chatglm-6b"

embedding_provider = "local_onnx"       # 或 "http_api"（调用私有 embedding 服务）
embedding_model    = "/models/bge-m3"
```

`agent-svc` 和 `embedding-svc` 均通过 `LlmProvider` / `EmbeddingProvider` Trait 抽象，不依赖具体供应商。

### 5.4 自定义扩展模块（独立微服务）

> 场景：客户需要接入完全自研的业务模块（如内部审批流、自研 BI 平台）

客户开发独立微服务，在服务注册（Consul / Nacos）中注册后，`api-gateway` 自动发现并反向代理：

```toml
[modules.extensions.custom_approval_svc]
enabled      = true
service_name = "approval-svc"          # 在服务注册中心注册的名称
route_prefix = "/v1/approvals"         # gateway 反向代理前缀
auth_mode    = "jwt_passthrough"       # 透传 JWT，不做额外权限校验
```

**扩展模块可以订阅 OntologyEvent**，复用事件总线能力，与核心模块联动。

---

## 6. 模块启停的运维影响

### 6.1 新增模块（Scale Out）

无需停机。新服务启动后向注册中心自注册，`api-gateway` 通过 `watch()` 感知，动态添加路由。

### 6.2 移除模块（Scale Down）

1. 先从 `ModuleProfile` 将模块标记为 `draining = true`
2. `api-gateway` 停止向该模块路由新请求，返回 `503 Service Draining`
3. 等待进行中的请求完成（graceful drain，超时 30s）
4. 服务注销，`api-gateway` 返回 `501 Module Disabled`

### 6.3 依赖关系违反检测

```
启动检查序列（api-gateway bootstrap）：
  1. 读取 ModuleProfile
  2. 校验硬依赖：agent_svc=true 且 function_svc=false → ERROR，拒绝启动
  3. 校验软依赖：agent_svc=true 且 embedding_svc=false → WARN，以降级模式启动
  4. 校验扩展模块：extension 注册的 route_prefix 不能与内置路由冲突
```

---

## 7. 前端的模块感知

前端在应用初始化时获取模块状态，动态渲染导航：

```
GET /meta/modules
→ {
    "core": ["api-gateway", "auth-svc", "ontology-svc"],
    "enabled": ["ingest-svc", "function-svc", "agent-svc"],
    "disabled": ["workflow-svc", "embedding-svc"],
    "extensions": ["erp-connector"]
  }
```

各工作台入口的可见性规则：

| 工作台 | 依赖模块 | 部分可用时 |
|--------|---------|-----------|
| 数据工程工作台 | `ingest-svc` | 完整显示 |
| 开发者工作台 | `function-svc` | 仅显示 Function（无 Workflow 设计器）|
| 开发者工作台 | `workflow-svc` | Workflow 设计器显示（需同时有 function-svc）|
| 分析工作台 | `agent-svc` | 完整显示 |
| AI 工作台 | `embedding-svc` | 文件上传可用（如无 embedding 则无语义索引）|
| 治理工作台 | Core | 永远可用 |

---

## 8. 部署拓扑（SaaS / 私有化 / 混合）

模块化部署与部署拓扑是两个正交维度：
- **模块化**（第 2–7 节）：哪些服务运行
- **拓扑**（本节）：服务运行在哪里、谁的机器上

三种官方支持的拓扑，可与任意 Edition 和 ModuleProfile 组合：

---

### 拓扑 A：SaaS（多租户云端托管）

> 目标客户：小型团队，不想运维，快速上手

```
┌─────────────────────────────────────────────────┐
│                   Palantir Cloud                 │
│                                                  │
│  api-gateway ──→ auth-svc ──→ ontology-svc       │
│       │                           │              │
│   ingest-svc  agent-svc  workflow-svc            │
│       │                                          │
│   NebulaGraph  TiDB  NATS  Redis  RustFS         │
│                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │ 租户 A   │  │ 租户 B   │  │ 租户 C   │       │
│  │ 数据隔离 │  │ 数据隔离 │  │ 数据隔离 │       │
│  └──────────┘  └──────────┘  └──────────┘       │
└─────────────────────────────────────────────────┘
```

**关键特征：**
- 所有租户共享同一套服务实例，数据通过 `tenant_id` 隔离（namespace 级隔离）
- 基础设施共享（TiDB 按 schema 隔离，NebulaGraph 按 Space 隔离，NATS 按 Subject 前缀隔离）
- 客户通过浏览器访问，无需安装任何软件
- 计费按使用量（API 调用次数 / 存储量 / Token 消耗）

**租户隔离边界：**
- OntologyObject 带 `tenant_id` 字段，所有查询自动注入 `WHERE tenant_id = ?`
- NATS Subject 前缀：`ontology.{tenant_id}.events.Employee.upsert`
- 文件存储：RustFS bucket = `tenant-{tenant_id}`

> **注意：** ADR-04（多租户）曾暂缓，SaaS 拓扑要求其落地。SaaS 拓扑上线前，ADR-04 必须完成。

---

### 拓扑 B：私有化部署（单租户，客户机房）

> 目标客户：金融/政企/医疗/军工，数据不能离开自己的网络

```
┌─────────────────────────────────────────────────┐
│              客户机房 / 私有云                    │
│                                                  │
│  api-gateway ──→ auth-svc ──→ ontology-svc       │
│       │                           │              │
│   ingest-svc  agent-svc  workflow-svc            │
│                                                  │
│  ┌──────────────────────────────────────────┐    │
│  │        客户自选基础设施（ADR-28）          │    │
│  │  NebulaGraph / MySQL / Kafka / Redis     │    │
│  │  达梦DB / OceanBase / RocketMQ / MinIO  │    │
│  └──────────────────────────────────────────┘    │
│                                                  │
│   LLM API ──→ 客户私有大模型（ChatGLM / Qwen）   │
└─────────────────────────────────────────────────┘
         ↑
    外网完全隔离 (AirGapped Profile)
    或通过企业防火墙选择性放通
```

**关键特征：**
- 单租户，数据完全在客户控制范围内
- `DeploymentProfile = AirGapped | Standard | Aliyun | ...`（ADR-28 驱动）
- LLM 替换为客户私有模型（5.3 节扩展点）
- License 通过本地 License 文件校验（无需联网激活）

**部署方式：**
- Helm Chart（K8s）→ 通过 ConfigMap 注入 `deployment.toml`
- Docker Compose → 单机小规模部署（Lite / Standard Edition）
- 裸机二进制 → AirGapped 极端场景，通过 feature flags 编译期裁剪

---

### 拓扑 C：混合部署（核心私有 + AI 云端）

> 目标客户：有数据安全要求，但希望使用公有云 LLM 和向量能力

```
┌──────────────────────────────┐     ┌────────────────────────────┐
│        客户机房               │     │       Palantir Cloud / 公有云│
│                              │     │                            │
│  api-gateway                 │     │  embedding-svc             │
│  auth-svc                    │ ←── │  (fastembed-rs，无数据存储) │
│  ontology-svc    ────────────┼──── │                            │
│  ingest-svc      mTLS 专线   │     │  LLM API（OpenAI / Claude）│
│  function-svc                │     │                            │
│  workflow-svc                │     └────────────────────────────┘
│                              │
│  NebulaGraph + TiDB          │
│  NATS + Redis                │
└──────────────────────────────┘
```

**关键特征：**
- 业务数据（OntologyObject）**永不离开**客户机房
- embedding-svc 在云端：仅接收文本，返回向量，**不存储任何数据**
- Agent 在客户机房：调用 LLM API 时只发送**已脱敏的 / 权限过滤后的**摘要，不发送原始数据
- 客户与 Palantir Cloud 之间通过 mTLS 专线，流量加密

**数据流安全边界：**
```
OntologyObject（机房）→ 脱敏摘要 → LLM API（云端）
                                    ↓
文本片段（机房）────────────────→ embedding-svc（云端）→ vector（返回机房）

完整数据（机房）─────────────────────────────────────── 永不离开机房
```

---

### 三种拓扑对比

| 维度 | SaaS | 私有化 | 混合 |
|------|:----:|:------:|:----:|
| 数据主权 | Palantir 托管 | 完全客户控制 | 业务数据客户控制 |
| 运维成本 | 零（Palantir 负责）| 高（客户自运维）| 中 |
| AI 能力 | 最强（公有 LLM）| 依赖客户模型质量 | 强（公有 LLM，数据不出境）|
| 合规适配 | 需评估（PIPL/GDPR）| 天然满足（数据不出境）| 业务数据满足，AI 调用需评估 |
| 网络要求 | 客户只需浏览器 | 客户全量内网 | 部分出网（专线）|
| 适用 Edition | Lite / Standard | 全部 | Professional / Enterprise |

---

### 拓扑 × Edition × InfraProfile 组合示例

| 客户类型 | 拓扑 | Edition | InfraProfile |
|---------|------|---------|-------------|
| 小型互联网公司 | SaaS | Standard | Cloud（Palantir 管理）|
| 中型制造企业 | 私有化 | Professional | Standard（NebulaGraph+TiDB）|
| 大型金融机构 | 私有化 | Enterprise+ | AirGapped + 达梦DB |
| 政务云客户 | 私有化 | Enterprise | Huawei Cloud（GaussDB+OBS）|
| 跨国企业（中国区）| 混合 | Enterprise | Aliyun（数据）+ OpenAI（AI）|
| 医疗集团 | 混合 | Professional | Standard + 本地 ChatGLM |

---

## 9. 与 ADR-28 的关系

| 维度 | ADR-28 | ADR-33 |
|------|--------|--------|
| 解决的问题 | 同一套代码，底层存储/消息怎么换 | 哪些服务需要部署 |
| 配置方式 | `deployment.toml [infrastructure]` | `deployment.toml [modules]` |
| 变更影响 | 换存储，数据需迁移 | 启停服务，无数据迁移 |
| 典型场景 | 阿里云 vs AWS vs 离线 | Lite vs Enterprise |
| 相互关系 | 正交，可任意组合 | 正交，可任意组合 |

> 一个客户可以同时是：`Edition = Professional` + `InfrastructureProfile = Aliyun`

---

## 逃生门

- `ModuleProfile` 最终以 Rust `feature flags` 形式也支持**编译期裁剪**（AirGapped 极限场景，彻底去除未用服务的代码）
- 扩展模块接口保持向后兼容：`api-gateway` 的扩展代理协议版本化

---

## 待讨论

- [ ] License 校验：ProductEdition 是否与 License 文件绑定（防止 Lite 客户自行开启 Enterprise 模块）？
- [ ] 模块间数据迁移：从 Standard 升级到 Professional 时，历史 OntologyObject 是否需要补跑向量化？
- [ ] 多实例部署：同一个 ModuleProfile 下，各服务可以有不同副本数，是否需要统一 HPA 策略？

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始版本：5 个 ProductEdition，模块依赖图，ModuleProfile 配置格式，4 类扩展点，前端模块感知 |
