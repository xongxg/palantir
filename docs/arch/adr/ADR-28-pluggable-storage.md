# ADR-28: 可插拔基础设施架构（Pluggable Infrastructure）

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

不同客户因政策、合规、云厂商绑定等原因，对存储基础设施有差异化要求：

```
客户 A（金融/政企）  → 数据不能离开本地机房，私有化部署
客户 B（国内企业）   → 阿里云，优先使用 PolarDB + OSS
客户 C（国际客户）   → AWS，S3 + Aurora MySQL
客户 D（医疗/军工）  → 完全离线，专有网络隔离，无外网
客户 E（合规要求）   → PIPL / GDPR / HIPAA，数据主权限制
```

系统架构需要在**不修改业务代码**的前提下，适配不同客户的存储要求。

## 决策

**所有基础设施依赖通过 Trait 抽象，以 DeploymentProfile 驱动实现选择，运行时动态装配。**

---

## 1. 可插拔层次

```
业务逻辑（services / crates）
  │  只依赖 Trait，不依赖具体实现
  ▼
┌──────────────────────────────────────────────┐
│            Infrastructure Trait Layer         │
│  OntologyGraphStore  │  StructuredStore       │
│  ObjectStore         │  VectorStore           │
│  EventBus            │  CacheStore            │
│  KeyManager          │  SearchStore           │
└──────────────────────────────────────────────┘
  │  DeploymentProfile 决定注入哪个实现
  ▼
┌──────────────────────────────────────────────┐
│            Pluggable Implementations          │
│  Standard   │  Aliyun  │  AWS  │  AirGapped  │
└──────────────────────────────────────────────┘
```

---

## 2. 完整 Trait 体系

### OntologyGraphStore（图存储）

```rust
pub trait OntologyGraphStore: Send + Sync {
    async fn upsert_vertex(&self, obj: &OntologyObject) -> Result<()>;
    async fn upsert_edge(&self, rel: &OntologyRelationship) -> Result<()>;
    async fn traverse(&self, from: &OntologyId, depth: u8) -> Result<Vec<OntologyObject>>;
    async fn query(&self, nql: &str) -> Result<Vec<OntologyObject>>;
}
// 实现：NebulaGraphStore / Neo4jStore / ArangoStore
```

### StructuredStore（关系型存储）

适用于 **有 Schema、支持事务、需要 JOIN** 的场景（身份、权限配置、审计日志元数据）。

```rust
pub trait StructuredStore: Send + Sync {
    async fn insert<T: Serialize>(&self, table: &str, record: &T) -> Result<Id>;
    async fn find<T: DeserializeOwned>(&self, table: &str, filter: &SqlFilter) -> Result<Vec<T>>;
    async fn update<T: Serialize>(&self, table: &str, id: &Id, record: &T) -> Result<()>;
    async fn delete(&self, table: &str, id: &Id) -> Result<()>;
    async fn execute_raw(&self, sql: &str, params: Vec<Value>) -> Result<Vec<Row>>;
}
// 实现：TiDbStore / MySqlStore / PostgresStore / PolarDbStore / GaussDbStore / OceanBaseStore
```

> **SqlFilter** 使用标准 WHERE 子句语义，所有关系型数据库均可映射。

---

### DocumentStore（文档存储）

适用于 **灵活 Schema、嵌套文档、无需 JOIN** 的场景。

国内客户常见场景：
- 已有 MongoDB 投入（遗留系统）
- OntologyObject 的 `properties` 字段结构差异大、无法预定义 Schema
- 阿里云 MongoDB（托管版）、腾讯云 MongoDB

```rust
pub trait DocumentStore: Send + Sync {
    async fn insert(&self, collection: &str, doc: Value) -> Result<Id>;
    async fn find_one(&self, collection: &str, filter: &DocFilter) -> Result<Option<Value>>;
    async fn find_many(&self, collection: &str, filter: &DocFilter, opts: &QueryOpts) -> Result<Vec<Value>>;
    async fn update_one(&self, collection: &str, filter: &DocFilter, update: &DocUpdate) -> Result<()>;
    async fn delete_one(&self, collection: &str, filter: &DocFilter) -> Result<()>;
    async fn aggregate(&self, collection: &str, pipeline: Vec<Value>) -> Result<Vec<Value>>;
}
// 实现：MongoDbStore / DynamoDbStore（海外）/ CosmosDbStore（Azure）
```

> **DocFilter** 使用 JSON 表达式（类 MongoDB query syntax），由各实现负责翻译为目标 API。

---

### AppendStore（追加写存储）

适用于 **高吞吐追加写、按时间范围查询、不需要更新删除** 的场景。

国内客户常见场景：
- 审计日志（AuditLog，append-only）
- 事件流元数据（Event 明细，海量写入）
- 已有 Cassandra / HBase 投入（电商、金融、运营商）
- 阿里云 Lindorm / 华为云 CloudTable（HBase 兼容）

```rust
pub trait AppendStore: Send + Sync {
    async fn append(&self, table: &str, record: Value) -> Result<()>;
    async fn append_batch(&self, table: &str, records: Vec<Value>) -> Result<()>;
    async fn scan(
        &self,
        table: &str,
        partition_key: &str,       // Cassandra partition key / HBase rowkey prefix
        time_range: Option<(DateTime, DateTime)>,
        limit: usize,
    ) -> Result<Vec<Value>>;
}
// 实现：CassandraStore / HBaseStore / LindormStore / ScyllaDbStore / BigTableStore
```

> Cassandra 的核心约束是**必须指定 partition key**，AppendStore 的接口设计反映了这一约束，避免全表扫描。

### ObjectStore（文件存储）

```rust
// 复用 object_store crate（已有，ADR-08）
// 实现：LocalFileSystem / RustFs / S3Store / OssStore / ObsStore
```

### VectorStore（向量搜索）

```rust
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, id: &str, vector: Vec<f32>, metadata: Value) -> Result<()>;
    async fn search(&self, query: Vec<f32>, top_k: usize) -> Result<Vec<VectorMatch>>;
    async fn delete(&self, id: &str) -> Result<()>;
}
// 实现：TiDbVectorStore / QdrantStore / LanceDbStore / MilvusStore
```

### EventBus（事件总线）

```rust
// 已有 ADR-10 的 EventPublisher / EventSubscriber trait
// 实现：InProcessBus / NatsBus / RocketMqBus / KafkaBus / SqsBus
```

### CacheStore（缓存）

```rust
pub trait CacheStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Bytes>>;
    async fn set(&self, key: &str, value: Bytes, ttl: Option<Duration>) -> Result<()>;
    async fn del(&self, key: &str) -> Result<()>;
    async fn del_pattern(&self, pattern: &str) -> Result<u64>;
}
// 实现：RedisStore / MemoryStore（单机开发）/ TencentRedisStore
```

### KeyManager（密钥管理）

```rust
pub trait KeyManager: Send + Sync {
    async fn get_secret(&self, path: &str) -> Result<SecretValue>;
    async fn encrypt(&self, plaintext: &[u8], key_id: &str) -> Result<Vec<u8>>;
    async fn decrypt(&self, ciphertext: &[u8], key_id: &str) -> Result<Vec<u8>>;
}
// 实现：VaultKeyManager / AwsKmsManager / AliKmsManager / LocalKeyManager（开发）
```

---

## 3. DeploymentProfile

配置文件驱动，启动时装配对应实现：

```toml
# deployment.toml（每个客户独立一份）

[profile]
name = "aliyun"          # standard | aliyun | aws | huawei | airgapped | custom

[graph]
provider = "nebula"
hosts = ["nebula-graph:9669"]
namespace = "palantir"

[structured]
provider = "polardb"     # tidb | mysql | postgres | polardb | gaussdb
url = "mysql://..."

[object]
provider = "oss"         # rustfs | s3 | oss | obs | local
bucket = "palantir-files"
region = "cn-hangzhou"

[vector]
provider = "tidb"        # tidb | qdrant | lancedb | milvus

# [document]              # 可选。有 MongoDB 遗留资产的客户才配置
# provider = "mongodb"
# url = "mongodb://mongo:27017"
# database = "palantir"

[append]
provider = "tidb"        # tidb | cassandra | hbase | lindorm（阿里云）| scylladb
# 无大规模审计写入时复用 structured；有 Cassandra 投入时切换
# provider = "cassandra"
# hosts = ["cassandra:9042"]
# keyspace = "palantir_audit"

[event_bus]
provider = "nats"        # inprocess | nats | rocketmq | kafka | sqs
url = "nats://nats:4222"

[cache]
provider = "redis"
url = "redis://redis:6379"

[key_manager]
provider = "vault"       # vault | awskms | alikms | local
url = "http://vault:8200"
```

---

## 4. 预定义 DeploymentProfile

### Standard（标准私有化）

```
图存储：   NebulaGraph（自托管）
结构化：   TiDB（自托管）
文件：     RustFS（自托管）
向量：     TiDB Vector
事件总线： NATS JetStream
缓存：     Redis
密钥：     HashiCorp Vault
```

### Aliyun（阿里云）

```
图存储：   NebulaGraph（ECS 自托管）
结构化：   PolarDB-MySQL 或 RDS MySQL
文件：     OSS
向量：     TiDB Vector 或 阿里云向量检索
事件总线： RocketMQ（阿里云版）
缓存：     Redis（阿里云版）
密钥：     阿里云 KMS
```

### AWS（国际云）

```
图存储：   NebulaGraph（EC2 自托管）或 Neptune
结构化：   Aurora MySQL
文件：     S3
向量：     TiDB Vector 或 OpenSearch Vector
事件总线： SNS / SQS 或 MSK（Kafka）
缓存：     ElastiCache Redis
密钥：     AWS KMS
```

### HuaweiCloud（华为云）

```
图存储：   NebulaGraph（华为云 ECS）
结构化：   GaussDB（PostgreSQL 兼容）
文件：     OBS
向量：     GaussDB 向量
事件总线： DMS Kafka
缓存：     DCS Redis
密钥：     DEW（华为云密钥管理）
```

### AirGapped（完全离线）

```
图存储：   NebulaGraph（本地）
结构化：   TiDB（本地）或 MySQL（本地）
文件：     LocalFileSystem 或 MinIO
向量：     TiDB Vector 或 Qdrant（本地）
事件总线： NATS（本地单节点）
缓存：     Redis（本地）
密钥：     Vault（本地）或 LocalKeyManager
网络：     完全断网，无外部依赖
```

---

## 5. Rust 实现方式

### 运行时动态装配（推荐）

```rust
// 启动时根据 deployment.toml 装配
pub struct InfrastructureContainer {
    pub graph:        Arc<dyn OntologyGraphStore>,    // NebulaGraph / Neo4j / Arango
    pub structured:   Arc<dyn StructuredStore>,       // TiDB / MySQL / PolarDB / GaussDB
    pub document:     Option<Arc<dyn DocumentStore>>, // MongoDB（可选，遗留系统适配）
    pub append:       Arc<dyn AppendStore>,            // Cassandra / TiDB / HBase（审计日志）
    pub object:       Arc<dyn ObjectStore>,            // RustFS / S3 / OSS / OBS
    pub vector:       Arc<dyn VectorStore>,            // TiDB Vector / Qdrant / LanceDB
    pub event_bus:    Arc<dyn EventPublisher>,         // NATS / RocketMQ / Kafka
    pub cache:        Arc<dyn CacheStore>,             // Redis / 内存
    pub key_manager:  Arc<dyn KeyManager>,             // Vault / AliKMS / AWS KMS
    pub discovery:    Arc<dyn ServiceDiscovery>,       // Consul / Nacos / etcd / Zookeeper / K8s
    pub config:       Arc<dyn ConfigCenter>,           // Consul / Nacos / Apollo / etcd / K8s / 本地文件
    // 注：Nacos / Consul / etcd 同时实现两个 trait，共享同一连接（见 ADR-29）
}

impl InfrastructureContainer {
    pub async fn from_profile(profile: &DeploymentConfig) -> Result<Self> {
        Ok(Self {
            graph: match profile.graph.provider.as_str() {
                "nebula"  => Arc::new(NebulaGraphStore::new(&profile.graph).await?),
                "neo4j"   => Arc::new(Neo4jStore::new(&profile.graph).await?),
                _         => bail!("unknown graph provider"),
            },
            structured: match profile.structured.provider.as_str() {
                "tidb" | "mysql" | "polardb" =>
                    Arc::new(SqlStore::new(&profile.structured).await?),
                "gaussdb" | "postgres" =>
                    Arc::new(PostgresStore::new(&profile.structured).await?),
                _ => bail!("unknown structured provider"),
            },
            // ... 其他组件类似
        })
    }
}
```

### Cargo Feature 编译裁剪（减小二进制体积）

```toml
# Cargo.toml
[features]
default    = ["nebula", "tidb", "rustfs", "nats", "redis", "vault"]
aliyun     = ["nebula", "polardb", "oss", "rocketmq", "redis", "alikms"]
aws        = ["nebula", "aurora", "s3", "sqs", "elasticache", "awskms"]
huawei     = ["nebula", "gaussdb", "obs", "dms", "dcs", "dew"]
airgapped  = ["nebula", "tidb", "localfs", "nats", "redis", "vault"]
```

---

## 6. 配置下发方式

| 场景 | 配置来源 |
|------|---------|
| 开发环境 | `deployment.toml`（本地文件）|
| 私有化部署 | 安装包内置 + 安装向导生成 |
| 云部署 | Consul KV（ADR-21）或 ConfigMap（K8s）|
| 离线环境 | 安装包内置，不依赖外部配置中心 |

---

## 7. 数据主权与合规

不同 Profile 天然隔离数据主权：

```
AirGapped Profile：
  - 所有数据在本地，无外网请求
  - Vault 本地实例管理密钥
  - 审计日志写本地 TiDB，不上报

Aliyun Profile：
  - 数据在阿里云中国区
  - 满足 PIPL（个人信息保护法）数据本地化要求
  - KMS 使用阿里云 KMS，密钥不出境

AWS Profile：
  - 数据在 AWS 指定 Region
  - 满足 GDPR（EU Region）或 HIPAA（US）
```

---

## 8. 存储选型决策矩阵

不同存储技术适合的场景和国内客户现状：

| 技术 | Trait | 适合场景 | 国内大规模案例 | 备注 |
|------|-------|---------|--------------|------|
| NebulaGraph | OntologyGraphStore | 图遍历、多跳关系 | 美团、京东、快手、bilibili | ✅ 经过大规模生产验证；中国产开源图数据库，社区活跃 |
| TiDB | StructuredStore / AppendStore | 结构化、水平扩容、HTAP | 美团、京东、平安银行 | MySQL 兼容，sqlx 接入 |
| MySQL 8.0 | StructuredStore | 结构化、遗留系统 | 极广 | 最低迁移成本，DBA 最多 |
| PolarDB-MySQL | StructuredStore | 阿里云，serverless | 阿里云客户通用 | 云托管，运维成本最低 |
| GaussDB | StructuredStore | 华为云，政企 | 华为云客户 | PostgreSQL 兼容 |
| OceanBase | StructuredStore | 超大规模，金融 | 支付宝、工商银行 | MySQL / Oracle 兼容 |
| MongoDB | DocumentStore | 灵活 schema，遗留 | 遗留系统常见 | 适配已有 MongoDB 投资 |
| Cassandra | AppendStore | 高吞吐追加写，审计 | 滴滴、字节 | 写入吞吐远超关系型 |
| HBase / Lindorm | AppendStore | 超大规模时序，运营商 | 运营商、金融 | 阿里云 Lindorm 托管 |
| Qdrant | VectorStore | 大规模向量检索 | — | Rust 实现，性能最好 |
| RocketMQ | EventBus | 事件总线，金融级 | 阿里、腾讯、字节 | 阿里云托管版可用 |

### NebulaGraph 的生产验证说明

NebulaGraph 不是小众数据库，而是经过国内头部互联网公司大规模验证的图数据库：

```
美团：外卖骑手调度图、商品知识图谱
京东：供应链关系图、商品推荐
快手：社交关系图（亿级节点）
bilibili：UP 主 - 视频 - 用户关系图
微众银行：风控关系图
```

与 Neo4j 相比：
- 性能接近，水平扩容更好（分布式原生设计）
- 开源免费（vs Neo4j 企业版扩容极贵）
- 中文社区和文档更完善
- 有官方 Rust SDK（`nebula-rust` crate）

---

## 10. 对现有 ADR 的影响

| ADR | 影响 |
|-----|------|
| ADR-07 | 已取代，NebulaGraph 作为默认图实现，其他可替换 |
| ADR-08 | RustFS 是默认文件实现，object_store crate 已抽象 ✅ |
| ADR-10 | EventPublisher trait 已抽象，多实现 ✅ |
| ADR-21 | Consul 是默认配置中心，AirGapped 时降级为本地文件 |
| ADR-27 | TiDB + NebulaGraph 是 Standard Profile 的选型 |

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策：可插拔基础设施架构，InfrastructureContainer + DeploymentProfile |
| v1.1 | 2026-03-19 | 拆分 StructuredStore → StructuredStore / DocumentStore / AppendStore；新增 MongoDB、Cassandra、HBase、OceanBase 适配；补充 NebulaGraph 大规模生产验证说明 |
