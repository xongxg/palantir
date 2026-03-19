# ADR-29: 可插拔服务发现与配置中心

> 状态：✅ 已决策 | 日期：2026-03-19 | 取代/扩展：[ADR-21](ADR-21-service-discovery.md)

---

## 问题

ADR-21 选定 Consul 作为服务发现和配置中心。但国内企业存在大量遗留基础设施投资：

```
Nacos（阿里开源）    → 服务发现 + 配置中心二合一，Spring Cloud / Dubbo 生态标配
Zookeeper           → Dubbo 传统注册中心，金融/运营商大量存量
Apollo（携程）       → 纯配置中心，国内使用最广泛的配置管理平台
etcd                → K8s 原生，云原生企业已有投入
Dubbo Registry      → 服务注册兼容要求（客户现有 Dubbo 服务需要发现我们的服务）
```

强制引入 Consul 将：
1. 增加客户运维成本（多维护一套中间件）
2. 浪费已有基础设施投资
3. 造成两套注册中心并存的混乱

---

## 决策

**将服务发现和配置中心分为两个独立 Trait，通过 DeploymentProfile 独立配置，充分复用客户已有中间件。**

---

## 1. Trait 定义

### ServiceDiscovery（服务发现）

```rust
pub trait ServiceDiscovery: Send + Sync {
    /// 注册当前服务实例
    async fn register(&self, instance: &ServiceInstance) -> Result<()>;
    /// 注销（graceful shutdown）
    async fn deregister(&self, service_id: &str) -> Result<()>;
    /// 查找某服务的所有健康实例
    async fn discover(&self, service_name: &str) -> Result<Vec<ServiceInstance>>;
    /// 订阅服务实例变化（流式，用于客户端负载均衡更新）
    async fn watch(&self, service_name: &str) -> Result<BoxStream<'static, Vec<ServiceInstance>>>;
}

pub struct ServiceInstance {
    pub id:       String,          // 唯一 ID（服务名 + IP + 端口）
    pub name:     String,          // 服务名（如 "ontology-svc"）
    pub host:     String,
    pub port:     u16,
    pub metadata: HashMap<String, String>,  // 协议版本、权重等
    pub healthy:  bool,
}

// 实现：
// ConsulDiscovery      → ADR-21 原有实现
// NacosDiscovery       → 阿里 Nacos，gRPC 或 HTTP API
// EtcdDiscovery        → K8s 生态，lease-based TTL
// ZookeeperDiscovery   → 传统 Dubbo 注册中心
// K8sDnsDiscovery      → 生产 K8s 环境，DNS SRV 记录，零额外组件
// StaticDiscovery      → AirGapped / 极简部署，deployment.toml 写死地址
```

### ConfigCenter（配置中心）

```rust
pub trait ConfigCenter: Send + Sync {
    /// 读取配置项
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<String>>;
    /// 读取整个 namespace 的所有配置
    async fn get_all(&self, namespace: &str) -> Result<HashMap<String, String>>;
    /// 写入配置（运维操作，非业务热路径）
    async fn set(&self, namespace: &str, key: &str, value: &str) -> Result<()>;
    /// 订阅配置变化（热更新，无需重启）
    async fn watch(&self, namespace: &str, key: &str) -> Result<BoxStream<'static, Option<String>>>;
}

// 实现：
// ConsulKvCenter       → Consul KV，ADR-21 原有实现
// NacosConfigCenter    → Nacos Config，namespace + dataId 映射
// ApolloConfigCenter   → 携程 Apollo，AppId + Cluster + Namespace
// EtcdConfigCenter     → etcd prefix watch
// K8sConfigMapCenter   → K8s ConfigMap + Watch API
// LocalFileCenter      → deployment.toml 本地文件（AirGapped / 开发环境）
```

---

## 2. Nacos 的特殊地位

Nacos 是国内使用最广泛的服务发现 + 配置中心二合一产品，同一个 Nacos 实例可以**同时实现两个 Trait**：

```rust
pub struct NacosClient {
    client: Arc<NacosServiceClient>,   // 复用同一连接
}

impl ServiceDiscovery for NacosClient { /* 注册/发现 */ }
impl ConfigCenter for NacosClient    { /* 配置读取/订阅 */ }
```

```toml
# deployment.toml — Nacos 同时承担两个角色
[service_discovery]
provider = "nacos"
server_addr = "nacos:8848"
namespace = "palantir-prod"
group = "DEFAULT_GROUP"

[config_center]
provider = "nacos"          # 与 service_discovery 共享同一连接，无额外成本
# server_addr / namespace 继承自上面，不需要重复配置
```

**优势**：客户已有 Nacos 的情况下，一套组件同时解决服务发现和配置管理，无需额外引入 Consul。

---

## 3. Dubbo 生态兼容

### 场景

客户现有 Java 微服务体系基于 Dubbo，已有 Nacos 或 Zookeeper 作为注册中心。客户希望我们的 Rust 服务能被他们的 Dubbo 服务发现，统一进入同一个注册中心。

### 方案

我们的 gRPC 服务注册到客户的 Nacos / Zookeeper 时，在 metadata 中标注协议信息：

```rust
ServiceInstance {
    name: "ontology-svc",
    host: "10.0.0.1",
    port: 50051,
    metadata: hashmap! {
        "protocol"  => "grpc",          // 区分 Dubbo RPC 和我们的 gRPC
        "version"   => "1.0.0",
        "weight"    => "100",
    },
}
```

**效果**：
- 我们的服务出现在客户的 Nacos 控制台，运维可以统一监控
- Dubbo 客户端可以看到我们的服务，但因协议不同（gRPC vs Dubbo 二进制）不会直接调用
- 客户的服务网格（如 Istio / 阿里 MSE）可以感知到我们的服务

> Dubbo 3.x 已原生支持 Triple 协议（HTTP/2 + Protobuf，与 gRPC 互通），若客户升级到 Dubbo 3.x，可以直接互调。

---

## 4. 各实现对比

| 实现 | 服务发现 | 配置中心 | 国内案例 | Rust SDK | 适用场景 |
|------|---------|---------|---------|---------|---------|
| Consul | ✅ | ✅ KV | 国际通用 | ✅ `consul` crate | Standard Profile 默认 |
| **Nacos** | ✅ | ✅ 原生 | 阿里、字节、腾讯 | ✅ `nacos-sdk-rust`（官方）| 国内企业首选，二合一 |
| **Apollo** | ❌ | ✅ 原生 | 携程、滴滴、美团 | ⚠️ HTTP API 封装 | 纯配置中心，已有 Apollo 投入 |
| **Zookeeper** | ✅ | ⚠️ 有限 | 传统 Dubbo 体系 | ✅ `zookeeper` crate | Dubbo 遗留体系 |
| etcd | ✅ | ✅ prefix | K8s 生态 | ✅ `etcd-client` crate | 云原生，K8s 内部 |
| K8s DNS | ✅ 自动 | ✅ ConfigMap | — | 无需 SDK | K8s 生产环境（零额外组件）|
| StaticFile | ✅ 写死 | ✅ 本地 toml | — | 无需 SDK | AirGapped / 极简单机 |

---

## 5. deployment.toml 配置格式

```toml
# Standard Profile（默认）
[service_discovery]
provider = "consul"
url = "http://consul:8500"
service_ttl = "10s"           # 健康检查 TTL

[config_center]
provider = "consul"           # 与 service_discovery 共享 Consul，无额外成本


# Nacos Profile（国内企业遗留系统）
[service_discovery]
provider = "nacos"
server_addr = "nacos:8848"
namespace = "palantir-prod"
group = "DEFAULT_GROUP"
username = "nacos"
password = "nacos"

[config_center]
provider = "nacos"            # 复用 service_discovery 的 Nacos 连接


# Apollo + Nacos 混合（常见：Nacos 做服务发现，Apollo 做配置）
[service_discovery]
provider = "nacos"
server_addr = "nacos:8848"
namespace = "palantir-prod"

[config_center]
provider = "apollo"
meta_server = "http://apollo-meta:8080"
app_id = "palantir"
cluster = "default"
namespace = "application"
token = "YOUR_TOKEN"


# Zookeeper Profile（Dubbo 遗留体系）
[service_discovery]
provider = "zookeeper"
connect_string = "zk1:2181,zk2:2181,zk3:2181"
root_path = "/dubbo"

[config_center]
provider = "local"            # Zookeeper 配置能力弱，降级到本地文件


# K8s Profile（生产云原生）
[service_discovery]
provider = "k8s"              # 无需额外配置，读取 in-cluster K8s DNS

[config_center]
provider = "k8s"              # 读取 ConfigMap（namespace: palantir-config）


# AirGapped Profile（完全离线）
[service_discovery]
provider = "static"
services = [
    { name = "ontology-svc",  addr = "10.0.0.1:50051" },
    { name = "function-svc",  addr = "10.0.0.2:50051" },
    { name = "agent-svc",     addr = "10.0.0.3:50051" },
]

[config_center]
provider = "local"            # 读取本地 deployment.toml，无外部依赖
```

---

## 6. InfrastructureContainer 更新

```rust
pub struct InfrastructureContainer {
    // ... 其他字段同 ADR-28 ...
    pub discovery:    Arc<dyn ServiceDiscovery>,   // 服务发现
    pub config:       Arc<dyn ConfigCenter>,        // 配置中心
}

impl InfrastructureContainer {
    pub async fn from_profile(profile: &DeploymentConfig) -> Result<Self> {
        // Nacos 二合一：同一个客户端实现两个 trait
        let (discovery, config) = match (
            profile.service_discovery.provider.as_str(),
            profile.config_center.provider.as_str(),
        ) {
            ("nacos", "nacos") => {
                let nacos = Arc::new(NacosClient::new(&profile.service_discovery).await?);
                (nacos.clone() as Arc<dyn ServiceDiscovery>,
                 nacos        as Arc<dyn ConfigCenter>)
            }
            ("consul", "consul") => {
                let consul = Arc::new(ConsulClient::new(&profile.service_discovery).await?);
                (consul.clone() as Arc<dyn ServiceDiscovery>,
                 consul        as Arc<dyn ConfigCenter>)
            }
            (sd, cc) => {
                // 独立配置（如 Nacos 服务发现 + Apollo 配置）
                let discovery = build_discovery(sd, &profile.service_discovery).await?;
                let config    = build_config(cc, &profile.config_center).await?;
                (discovery, config)
            }
        };
        Ok(Self { discovery, config, /* ... */ })
    }
}
```

---

## 7. 配置热更新

通过 `ConfigCenter::watch` 实现无重启配置更新：

```rust
// 服务启动时订阅关键配置
let mut watcher = infra.config.watch("palantir", "feature_flags").await?;
tokio::spawn(async move {
    while let Some(new_value) = watcher.next().await {
        if let Some(flags) = new_value {
            feature_flags.update(flags);
            tracing::info!("feature_flags updated: {}", flags);
        }
    }
});
```

适合热更新的配置项：
- Feature flags（灰度发布）
- 限流阈值
- LLM 模型切换
- 向量搜索阈值（从 TiDB Vector 切换到 Qdrant 的触发条件）

---

## 8. 对 ADR-21 的影响

ADR-21 的 Consul 方案**不废除**，变为 Standard Profile 的默认实现。

| 变更点 | ADR-21（原）| ADR-29（扩展）|
|--------|-----------|-------------|
| ServiceDiscovery | Consul 唯一 | Consul / Nacos / etcd / Zookeeper / K8s / Static |
| ConfigCenter | Consul KV | Consul / Nacos / Apollo / etcd / K8s / LocalFile |
| 配置来源 | 硬编码 Consul | deployment.toml provider 字段驱动 |
| 二合一优化 | — | Nacos / Consul / etcd 可同时实现两个 Trait，共享连接 |

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策：ServiceDiscovery + ConfigCenter 双 Trait，支持 Nacos / Apollo / Zookeeper / etcd / K8s / Static；Dubbo 生态兼容说明 |
