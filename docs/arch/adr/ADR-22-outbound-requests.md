# ADR-22: 对外出站请求架构

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

系统主动向外部发起请求（Webhook 回调、第三方 API、LLM、数据源拉取、通知）时，由哪个部件承接？如何统一管理 API Key、Retry、审计？

## 决策

**不引入独立 Egress Gateway，利用现有服务架构分场景归属，共享 `palantir-http-client` crate 统一出站能力。**

## 出站场景归属

| 场景 | 归属服务 | 理由 |
|------|---------|------|
| LLM API（OpenAI / Anthropic）| agent-svc 直接调 | 流式响应，延迟敏感，紧耦合合理 |
| Webhook 回调外部系统 | function-svc（HTTP Function）| 天然有调用记录，workflow-svc 触发 |
| 第三方 REST API 集成 | function-svc（Integration Function）| 统一出口，可审计 |
| 外部数据源拉取 | ingest-svc（HttpAdapter）| SourceAdapter 天然支持 HTTP 轮询 |
| 邮件 / 短信 / 推送通知 | function-svc（Notification Function）| 统一出口 |

## function-svc 作为集成出口

外部调用封装为 Ontology Function，统一在 function-svc 执行：

```
workflow-svc 触发
  ↓
function-svc.invoke("send_webhook")
  ↓
HTTP POST → 外部系统
```

好处：
- 调用记录天然存在（FunctionTrace）
- 前端 / Agent 可复用同一个 Function
- Retry、超时策略集中配置

## 共享 palantir-http-client crate（新增）

所有服务共用，不重复实现出站能力：

```rust
let client = HttpClient::new()
    .with_retry(3)
    .with_timeout(Duration::from_secs(30))
    .with_circuit_breaker(threshold: 5);

client.post("https://external-api.com/webhook")
    .api_key_from_vault("external/webhook-key")
    .json(&payload)
    .send()
    .await?;
```

| 能力 | 说明 |
|------|------|
| 自动 Retry | 指数退避，可配置次数 |
| Circuit Breaker | 断路器，防止雪崩 |
| 超时控制 | 每个请求独立超时 |
| 出站请求日志 | 统一记录（who + url + status + latency）|
| API Key 注入 | 从 Vault / Consul KV 动态拉取 |

## 整体出站流向

```
agent-svc     → LLM API（直接，流式）         ┐
function-svc  → Webhook / 第三方 API / 通知   ├─ 均使用 palantir-http-client
ingest-svc    → 外部数据源（HttpAdapter）      ┘

API Key 统一从 Vault / Consul KV 拉取（ADR-21）
```

## 何时引入独立 Egress Gateway

以下条件满足时再评估：
- 出站请求量极大，需要独立扩容限流
- 合规要求所有出站流量经过同一审计节点（固定出口 IP）
- 多租户场景，不同租户出站走不同 IP 池

**当前阶段不需要。**

## 逃生门

`palantir-http-client` 内部抽象 `OutboundClient` trait，未来引入 Egress Gateway 时只替换实现，调用方代码不变。
