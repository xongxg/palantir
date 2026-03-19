# 基础设施子架构

> 状态：设计阶段 | 日期：2026-03-19 | 关联：ADR-27 v1.1, ADR-28 v1.1

---

## 1. Standard Profile 完整基础设施栈

Standard Profile 是私有化部署的默认选型，所有组件均为开源自托管。

| 层 | Trait | 默认实现 | 语言 | 用途 |
|----|-------|---------|------|------|
| 图存储 | `OntologyGraphStore` | NebulaGraph | C++ | Ontology TBox/ABox/Relationship（图核心）|
| 结构化存储 | `StructuredStore` | TiDB | Go | 身份/权限配置/审计/Memory 元数据（MySQL 兼容）|
| 文档存储 | `DocumentStore` | —（可选）| — | 遗留 MongoDB 适配，灵活 Schema 场景 |
| 追加写存储 | `AppendStore` | TiDB（复用）| Go | 审计日志、事件元数据高吞吐写入 |
| 向量搜索 MVP | `VectorStore` | TiDB Vector | Go | Agent Memory 向量索引（内置，无需额外服务）|
| 向量搜索中期 | `VectorStore` | LanceDB（嵌入式）| Rust | > 50 万向量或 P99 > 200ms |
| 向量搜索生产 | `VectorStore` | Qdrant | Rust | 多节点部署 |
| 文件存储 | `ObjectStore` | RustFS（S3-compatible）| Rust | 用户上传原始文件 |
| 列式计算 | — | Apache Arrow + DataFusion | Rust | L2 分析查询引擎 |
| 缓存 | `CacheStore` | Redis | C | L1 热数据、授权缓存、语义缓存 |
| 事件总线 | `EventPublisher/Subscriber` | NATS JetStream | Go | 异步事件 |
| 本地 Embedding | — | fastembed-rs + BGE-small-zh | Rust | 向量化，独立 embedding-svc |
| 密钥管理 | `KeyManager` | HashiCorp Vault | Go | P2，字段加密密钥 |

---

## 2. 可替换实现（按客户 DeploymentProfile）

不同客户因政策、合规、遗留系统原因对存储有差异化要求。通过 `deployment.toml` 切换，业务代码零修改。

### 结构化存储（StructuredStore）替代选项

| 产品 | 协议 | 适用场景 |
|------|------|---------|
| MySQL 8.0 | MySQL | 遗留系统，运维经验最丰富 |
| PolarDB-MySQL | MySQL | 阿里云，serverless，云托管运维最低 |
| PolarDB-PG | PostgreSQL | 阿里云，pgvector + AGE |
| GaussDB | PostgreSQL | 华为云，政企场景 |
| OceanBase | MySQL / Oracle | 超大规模，金融（支付宝、工商银行级别）|

### 文档存储（DocumentStore，可选）

| 产品 | 适用场景 |
|------|---------|
| MongoDB | 客户已有 MongoDB 投入（遗留系统迁移）|
| 阿里云 MongoDB | 云托管，国内合规 |
| 腾讯云 MongoDB | 云托管，国内合规 |

> DocumentStore 在 InfrastructureContainer 中为 `Option`，无 MongoDB 遗留的客户不启用。

### 追加写存储（AppendStore）替代选项

| 产品 | 语言 | 适用场景 |
|------|------|---------|
| Cassandra | Java | 客户已有 Cassandra 集群（滴滴、字节级别写入量）|
| HBase | Java | 运营商、金融，超大规模时序 |
| Lindorm | Java | 阿里云 HBase 兼容托管版 |
| ScyllaDB | C++ | Cassandra 兼容，性能更高 |

> 无高吞吐审计需求时，AppendStore 默认复用 TiDB（StructuredStore 的 append-only 写法），避免引入额外组件。

### 事件总线（EventBus）替代选项

| 产品 | 适用场景 |
|------|---------|
| RocketMQ（阿里云版）| 阿里云部署，金融级可靠性 |
| Kafka / MSK | AWS 部署，或客户已有 Kafka 投入 |
| DMS Kafka | 华为云部署 |

---

## 3. 本地开发启动（无 Docker）

**不使用 docker-compose**，所有依赖均为单二进制，直接启动进程。

```bash
cargo xtask dev   # 并发启动全部依赖 + services
cargo xtask stop  # 全部停止
```

### xtask 启动顺序

```
1. nebula-metad                      → NebulaGraph Meta 服务
2. nebula-storaged                   → NebulaGraph 存储层
3. nebula-graphd                     → NebulaGraph 查询层
4. tidb-server                       → TiDB（结构化存储 + 向量）
5. nats-server -js                   → NATS JetStream
6. redis-server                      → Redis
7. rustfs server                     → RustFS
8. consul agent -dev                 → Consul（服务发现 + 配置中心）
9. 等待健康检查通过（/health 轮询）
10. cargo run -p ontology-svc
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

## 4. 四层计算模型

详见 [../adr/ADR-14-compute-layers.md](../adr/ADR-14-compute-layers.md)

```
L1  Redis           → 热数据缓存（AccessDecision 2min、EnrichedIdentity 5min、Arrow 快照）
L2  Arrow/DataFusion → 本地内存列式分析查询
L3  TiDB / Nebula   → 持久化查询
L4  专项服务         → embedding-svc、向量检索
```

---

## 5. 事件 Topic 规范

```
Stream：ontology-events（唯一 NATS Stream）

Subject：
  ontology.events.{entity_type}.upsert
  ontology.events.{entity_type}.delete
  ontology.events.{entity_type}.link
  ingest.jobs.created
  workflow.triggers
  agent.feedback
  authz.policy.changed          ← 权限变更，触发缓存失效
  authz.identity.changed        ← 身份变更，触发 EnrichedIdentity 失效
```

---

## 6. NebulaGraph 配置

```bash
# 启动顺序（本地开发，单机模式）
nebula-metad --meta_server_addrs=127.0.0.1:9559 --local_ip=127.0.0.1 --data_path=/tmp/nebula/meta
nebula-storaged --meta_server_addrs=127.0.0.1:9559 --local_ip=127.0.0.1 --data_path=/tmp/nebula/storage
nebula-graphd --meta_server_addrs=127.0.0.1:9559 --local_ip=127.0.0.1 --port=9669

# 连接
nebula-console -addr 127.0.0.1 -port 9669 -u root -p nebula
```

NebulaGraph 已在美团、京东、快手、bilibili 等大规模生产环境验证，图遍历性能接近 Neo4j，水平扩容原生支持，开源免费。

---

## 7. TiDB 配置

```bash
# 本地开发（单节点）
tidb-server --store=unistore --path="" --log-level=warn

# 连接（MySQL 兼容）
mysql -h 127.0.0.1 -P 4000 -u root
```

TiDB Vector 向量搜索（MVP 阶段）：

```sql
ALTER TABLE agent_memory ADD COLUMN embedding VECTOR(512);
CREATE VECTOR INDEX ON agent_memory ((VEC_COSINE_DISTANCE(embedding)));
SELECT * FROM agent_memory
ORDER BY VEC_COSINE_DISTANCE(embedding, ?) LIMIT 10;
```

---

## 8. Redis 使用规范

| Key 前缀 | 用途 | TTL |
|---------|------|-----|
| `sc:{hash}` | Semantic Cache（语义相似度）| 动态（按命中率调整）|
| `authz:{sub}:{res}:{act}` | AccessDecision 缓存 | 2min |
| `identity:{user_id}` | EnrichedIdentity 缓存 | 5min |
| `rbac:{role}` | RBAC 角色权限缓存 | 30min |
| `wf:cooldown:{object_id}` | Workflow 冷却窗口 | 自定义 |
| `arrow:{entity_type}:snapshot` | Arrow IPC 快照 | 中 |
| `mem:{user_id}:{hash}` | Agent Memory 热数据 | 72h |
| `lock:{resource}` | 分布式锁 | 短 |

---

## 9. 向量存储演进路径

| 阶段 | 方案 | 触发条件 |
|------|------|---------|
| MVP | TiDB Vector（内置）| 默认，无需额外服务 |
| 中期 | LanceDB（嵌入式，无独立进程）| 向量 > 50 万 或 P99 > 200ms |
| 生产 | Qdrant 自托管 | 多节点部署需求 |

---

## 10. 待细化

- [ ] `cargo xtask dev` 实现（NebulaGraph 三进程管理、健康检查轮询）
- [ ] NebulaGraph Schema 初始化脚本（CREATE TAG / CREATE EDGE）
- [ ] TiDB Schema 初始化脚本（sqlx migrate）
- [ ] 生产 Kubernetes Helm Chart 设计
- [ ] 监控 / 告警方案（Prometheus + Grafana）
- [ ] DeploymentProfile 配置验证（startup 时 schema check）

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本，架构设计阶段 |
| v0.1.1 | 2026-03-19 | ADR-27 v1.1：SurrealDB → NebulaGraph（图核心）+ TiDB（结构化） |
| v0.1.2 | 2026-03-19 | ADR-28 v1.1：加入 DocumentStore（MongoDB 适配）、AppendStore（Cassandra/HBase）；完整可替换实现表；移除 SurrealDB 配置节；Redis 规范补充权限缓存 Key；事件 Topic 补充权限失效事件 |
