# 基础设施子架构

> 状态：设计阶段 | 日期：2026-03-19 | 关联：ADR-27 v1.1

## 完整基础设施栈

| 层 | 选型 | 语言 | 用途 |
|----|------|------|------|
| 图存储 | NebulaGraph | C++ | Ontology TBox/ABox/Relationship（图核心）|
| 结构化存储 | TiDB | Go | 身份/权限/审计/Memory元数据（MySQL兼容）|
| 向量搜索 MVP | TiDB Vector | Go | Agent Memory 向量索引（内置，无需额外服务）|
| 向量搜索中期 | LanceDB（嵌入式）| Rust | > 50万向量或 P99 > 200ms |
| 向量搜索生产 | Qdrant | Rust | 多节点部署 |
| 文件存储 | RustFS（S3-compatible）| Rust | 用户上传原始文件 |
| 列式计算 | Apache Arrow + DataFusion | Rust | L2 分析查询引擎 |
| 缓存 | Redis | C | L1 热数据、授权缓存、语义缓存 |
| 事件总线 | InProcessBus → NATS JetStream | Rust / Go | 异步事件 |
| 本地 Embedding | fastembed-rs + BGE-small-zh | Rust | 向量化，独立 embedding-svc |
| 密钥管理 | HashiCorp Vault / KMS | — | P2，字段加密密钥 |

---

## 本地开发启动（无 Docker）

**不使用 docker-compose**，所有依赖均为单二进制，直接启动进程。

```bash
cargo xtask dev   # 并发启动全部依赖 + services
cargo xtask stop  # 全部停止
```

### xtask 启动顺序

```
1. nebula-storaged / nebula-graphd    → NebulaGraph（图存储）
2. tidb-server                        → TiDB（结构化存储）
3. nats-server -js                    → NATS JetStream
4. redis-server                       → Redis
5. rustfs server                      → RustFS
6. 等待健康检查通过（/health 轮询）
7. cargo run -p ontology-svc
   cargo run -p ingest-svc
   cargo run -p function-svc
   cargo run -p agent-svc
   cargo run -p embedding-svc
   cargo run -p workflow-svc
   cargo run -p auth-svc
   cargo run -p api-gateway
```

### Docker 使用场景（降级为可选）

- CI/CD 环境隔离（GitHub Actions）
- 生产 Kubernetes 需要镜像
- Windows 上运行 Redis（无官方原生版）

---

## 四层计算模型详见

[../adr/ADR-14-compute-layers.md](../adr/ADR-14-compute-layers.md)

---

## 事件 Topic 规范

```
Stream：ontology-events（唯一 NATS Stream）

Subject：
  ontology.events.{entity_type}.upsert
  ontology.events.{entity_type}.delete
  ontology.events.{entity_type}.link
  ingest.jobs.created
  workflow.triggers
  agent.feedback
```

---

## SurrealDB 配置

```bash
# 开发（内存模式）
surreal start --user root --pass root memory

# 生产（RocksDB）
surreal start --user root --pass root file:./data/surreal.db

# 大规模（TiKV，未来）
surreal start tikv://tikv-cluster:2379
```

---

## Redis 使用规范

| Key 前缀 | 用途 | TTL |
|---------|------|-----|
| `sc:{hash}` | Semantic Cache | 短（按命中率调整）|
| `authz:{sub}:{res}:{act}` | 授权结果缓存 | 短 |
| `wf:cooldown:{object_id}` | Workflow 冷却窗口 | 自定义 |
| `arrow:{entity_type}:snapshot` | Arrow IPC 快照 | 中 |
| `mem:{user_id}:{hash}` | Agent Memory 热数据 | 72h |
| `lock:{resource}` | 分布式锁 | 短 |

---

## 向量存储演进路径

| 阶段 | 方案 | 触发条件 |
|------|------|---------|
| MVP | SurrealDB 内置向量 | 默认 |
| 中期 | LanceDB（嵌入式，无独立进程）| 向量 > 50万 或 P99 > 200ms |
| 生产 | Qdrant 自托管 | 多节点部署 |

---

## 待细化

- [ ] `cargo xtask dev` 实现（进程管理、健康检查轮询）
- [ ] SurrealDB Schema 初始化脚本
- [ ] 生产 Kubernetes Helm Chart 设计
- [ ] 监控 / 告警方案（Prometheus + Grafana）

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本，架构设计阶段 |
| v0.1.1 | 2026-03-19 | ADR-27 v1.1：SurrealDB → NebulaGraph（图核心）+ TiDB（结构化），TiDB Vector 替代 SurrealDB 内置向量 |
