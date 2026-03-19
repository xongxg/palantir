# ADR-21: 服务发现与配置中心 — Consul

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

内部服务如何自动发现彼此地址？配置如何集中管理并支持热更新？

## 决策

**P1 阶段引入 Consul**，服务自注册模式，全自动接入。

## 自动化流程

```
服务启动
  ↓
1. 从 Consul KV 拉取配置
2. 向 Consul 注册自己（地址 + 健康检查端点）
3. 正常服务

服务关闭（SIGTERM）
  ↓
4. 自动注销（signal handler）
```

调用方动态发现地址，不写死：

```
agent-svc 调 function-svc
  → 查 Consul："function-svc 在哪？"
  → 得到地址
  → 发起 gRPC 调用
```

## Consul 覆盖三个需求

| 需求 | Consul 能力 |
|------|------------|
| 服务注册 / 发现 | Agent API 自注册 |
| 配置中心 | KV Store |
| 健康检查 | HTTP / gRPC / TTL 三种模式 |

## Rust 实现骨架

```rust
// 启动：拉取配置
let config = consul.kv_get("palantir/function-svc/config").await?;

// 启动：注册自己
consul.register(ServiceRegistration {
    name: "function-svc",
    address: my_ip,
    port: 3002,
    check: HealthCheck::Http {
        url: "http://localhost:3002/health",
        interval: "10s",
    },
}).await?;

// 关闭：自动注销
tokio::signal::ctrl_c().await?;
consul.deregister("function-svc").await?;

// 调用方：服务发现
let instances = consul.discover("function-svc").await?;
let addr = instances.pick_one(); // 随机 or 轮询
let client = FunctionSvcClient::connect(addr).await?;
```

## 配置热更新

```rust
// Watch KV，变更时自动感知，无需重启
consul.watch("palantir/function-svc/config", |new_config| {
    config.update(new_config); // 原子替换
}).await;
```

## 分阶段落地

| 阶段 | 方案 | 自动化程度 |
|------|------|-----------|
| MVP | 环境变量写死地址 | 手动 |
| P1 | Consul 自注册 + KV 配置 | 全自动 |
| 生产（K8s）| K8s DNS + ConfigMap / Secret | 全自动（平台接管）|

## K8s 上的替代

部署到 K8s 后，服务发现由 K8s DNS 原生接管：

```
function-svc.default.svc.cluster.local:3002
```

ConfigMap / Secret 替代 Consul KV，Consul 可退出。
`ServiceDiscovery` trait 抽象，切换不改业务代码。

## 逃生门

```rust
pub trait ServiceDiscovery: Send + Sync {
    async fn discover(&self, name: &str) -> Result<Vec<ServiceInstance>>;
    async fn register(&self, reg: ServiceRegistration) -> Result<()>;
    async fn deregister(&self, name: &str) -> Result<()>;
}
// 实现：ConsulDiscovery / K8sDnsDiscovery / StaticDiscovery（MVP env vars）
```

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
