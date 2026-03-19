# Palantir Architecture — Overview

> 版本：v0.1.6 | 日期：2026-03-19 | 状态：设计阶段
>
> 更新规则：每日 refine → patch 版本递增；服务新增/删除 → minor；底层存储/协议更换 → major

---

## 1. 核心原则

| 原则 | 说明 |
|------|------|
| 领域边界清晰 | 每个服务对应一个有界上下文，不跨域直接访问数据库 |
| 事件驱动骨干 | 写操作产生 OntologyEvent → Event Bus → 订阅方异步消费 |
| 同步调用极简 | 只有"需要立即返回结果"的场景才同步（Agent → Function）|
| Library / Service 分离 | 核心逻辑在 `crates/`（可测试），服务壳在 `services/`（薄封装）|
| Trait 优先 | 每个基础设施依赖都有 trait 抽象，换实现不改调用方 |
| 成本感知 | 优先本地计算（ONNX embedding、SurrealDB 内置向量），按需引入外部服务 |

---

## 2. Workspace 结构

```
palantir/
├── crates/                          # 核心库（可测试，无 HTTP 依赖）
│   ├── palantir-ontology-manager/
│   ├── palantir-domain/
│   ├── palantir-persistence/
│   ├── palantir-pipeline/
│   ├── palantir-agent/
│   ├── palantir-event-bus/          # NEW
│   ├── palantir-function-core/      # NEW
│   └── palantir-auth-core/          # NEW
├── services/                        # 服务壳（薄封装，HTTP / gRPC）
│   ├── api-gateway/
│   ├── embedding-svc/               # NEW
│   ├── ontology-svc/
│   ├── ingest-svc/
│   ├── function-svc/
│   ├── agent-svc/
│   ├── workflow-svc/
│   └── auth-svc/
└── frontend/                        # React + TypeScript + Vite
```

---

## 3. 服务职责速览

| 服务 | 职责 | 子架构文档 |
|------|------|-----------|
| `api-gateway` | JWT 解析 + 路由 + SSE 转发 | [services/api-gateway.md](services/api-gateway.md) |
| `ontology-svc` | TBox/ABox CRUD、事件发布、离线同步 | [services/ontology-svc.md](services/ontology-svc.md) |
| `ingest-svc` | Source/Mapping 管理、摄入调度、游标续传 | [services/ingest-svc.md](services/ingest-svc.md) |
| `function-svc` | Function/Logic 注册与执行，CEL 引擎 | [services/function-svc.md](services/function-svc.md) |
| `agent-svc` | LLM 推理、Multi-Agent、语义缓存、AgentTrace | [services/agent-svc.md](services/agent-svc.md) |
| `embedding-svc` | 集中式向量化（fastembed-rs + BGE-small-zh）| [services/embedding-svc.md](services/embedding-svc.md) |
| `workflow-svc` | Workflow 编排、Cron/事件触发、Saga 补偿 | [services/workflow-svc.md](services/workflow-svc.md) |
| `auth-svc` | RBAC + ABAC + ReBAC 策略管理与评估 | [services/auth-svc.md](services/auth-svc.md) |

---

## 4. 系统闭环

```
外部数据 → ingest-svc → ontology-svc（写 OntologyEvent）
                              ↓ Event Bus
                   ┌──────────┴───────────┐
              workflow-svc            agent-svc
              （触发 Action）        （Proactive 预计算）
                    ↓
              function-svc（Logic 只读推导）
                    ↓
              ontology-svc（写回，闭环）
```

---

## 5. ADR 决策索引

> 完整 ADR 见 [adr/](adr/) 目录

| ADR | 问题 | 决策 | 状态 |
|-----|------|------|------|
| [ADR-01](adr/ADR-01-no-cqrs.md) | CQRS | 不做；OntologyReader trait 留逃生门 | ✅ |
| [ADR-02](adr/ADR-02-function-model.md) | Function 执行模型 | Rust / CEL / 自然语言三层 | ✅ |
| [ADR-03](adr/ADR-03-bff.md) | BFF 边界 | Gateway 只路由+JWT，聚合在 Function | ✅ |
| [ADR-04](adr/ADR-04-multi-tenant.md) | 多租户 | 暂缓，待商量 | ⏸ |
| [ADR-05](adr/ADR-05-offline-sync.md) | 离线同步 | CRDT 内嵌 ontology-svc，palantir-sync-client 独立库 | ✅ |
| [ADR-06](adr/ADR-06-agent-memory.md) | Agent Memory 存储 | SurrealDB + 向量按阶段演进，MemoryStore trait | ✅ |
| [ADR-07](adr/ADR-07-surrealdb.md) | Ontology 存储 | SurrealDB（文档+图+向量），TiKV 扩容路径 | ✅ |
| [ADR-08](adr/ADR-08-file-storage.md) | 文件存储 | RustFS，object_store crate 抽象 | ✅ |
| [ADR-09](adr/ADR-09-compliance.md) | 合规架构 | 数据分类 → WORM → Crypto-Shredding → 字段加密 | ✅ |
| [ADR-10](adr/ADR-10-event-bus.md) | Event Bus 选型 | InProcessBus → Fluvio / NATS，Kafka 备选 | ✅ |
| [ADR-11](adr/ADR-11-workflow-trigger.md) | Workflow 触发器 | Cron + EventListener 统一 TriggerManager | ✅ |
| [ADR-12](adr/ADR-12-event-listener.md) | EventListener 复杂度 | tokio async 无状态过滤，有状态聚合 via Logic + SurrealDB | ✅ |
| [ADR-13](adr/ADR-13-vector-cost.md) | 向量成本控制 | 本地 ONNX + 分层检索 + 选择性 embedding | ✅ |
| [ADR-14](adr/ADR-14-compute-layers.md) | 计算分层模型 | 四层：L1 Redis / L2 本地内存 / L3 SurrealDB / L4 专项服务 | ✅ |
| [ADR-15](adr/ADR-15-event-sequence.md) | 事件序列粒度 | 按 EntityType 独立序列，NATS Subject 层级 | ✅ |
| [ADR-16](adr/ADR-16-frontend.md) | 前端选型 | React + TypeScript + Vite，utoipa 生成 OpenAPI | ✅ |
| [ADR-17](adr/ADR-17-streaming-protocol.md) | Agent 流式协议 | SSE → WebSocket → WebRTC 按需演进 | ✅ |
| [ADR-18](adr/ADR-18-arrow-datafusion.md) | L2 计算引擎 | Apache Arrow + DataFusion，Arrow IPC 序列化到 Redis | ✅ |
| [ADR-19](adr/ADR-19-embedding-svc.md) | Embedding 服务 | 独立 embedding-svc，fastembed-rs + BGE-small-zh | ✅ |
| [ADR-20](adr/ADR-20-internal-rpc.md) | 内部服务通信 | gRPC（tonic + protobuf），外部保持 HTTP + JSON | ✅ |
| [ADR-21](adr/ADR-21-service-discovery.md) | 服务发现与配置中心 | Consul 自注册，生产 K8s DNS 接管，ServiceDiscovery trait 抽象 | ✅ |

---

## 6. 逃生门汇总

| trait | 今天实现 | 未来替换 |
|-------|---------|---------|
| `OntologyObjectStore` | SurrealDB | Postgres / TiDB |
| `OntologyGraphStore` | SurrealDB | Neo4j / TigerGraph |
| `OntologyReader` | SurrealDB | 只读副本 |
| `EventPublisher/Subscriber` | InProcessBus | Fluvio / NATS / Kafka |
| `MemoryStore` | SurrealDB 内置向量 | LanceDB / Qdrant |
| `PolicyEvaluator` | RBAC 简单实现 | OPA / Cedar |
| `ObjectStore`（文件）| RustFS | MinIO / S3 / 云 OSS |

---

## 7. 子文档索引

| 领域 | 文档 |
|------|------|
| 前端 | [frontend/arch.md](frontend/arch.md) |
| 共享库 | [crates/arch.md](crates/arch.md) |
| 基础设施 | [infrastructure/arch.md](infrastructure/arch.md) |
| 各服务 | [services/](services/) |
| ADR | [adr/](adr/) |
