# ADR-30: 可插拔可观测性架构（监控、追踪、日志、故障定位）

> 状态：✅ 已决策 | 日期：2026-03-19 | 依赖：[ADR-28](ADR-28-pluggable-storage.md)、[ADR-29](ADR-29-service-discovery-config-center.md)

---

## 问题

不同部署环境对监控平台要求不同，且审计日志（ADR-09）与运维日志职责不同：

```
Standard / AirGapped → Prometheus + Grafana + Jaeger（自托管）
阿里云               → ARMS（Metrics + Tracing）+ SLS（日志）
华为云               → AOM（Metrics + Tracing）+ LTS（日志）
AWS                  → CloudWatch（三合一）
腾讯云               → 云监控 + APM + CLS
政企离线             → 本地文件落盘 + 本地 Jaeger，无外网
```

同时，分布式故障定位需要 Traces / Metrics / Logs 三信号通过 `trace_id` 强关联，否则各自孤立、定位靠猜。

---

## 决策

**以 OpenTelemetry（OTLP）为统一中间层，定义 `TelemetryProvider` + `LogSink` 两个 Trait，通过 `DeploymentProfile` 驱动后端选择。审计日志不在此 ADR，复用 ADR-09。**

---

## 1. 三信号分工

| 信号 | 用途 | 工具 |
|------|------|------|
| **Traces** | 跨服务调用链，故障定位到具体 span | Jaeger / Tempo / ARMS / AOM |
| **Metrics** | 趋势监控、告警规则、SLO | Prometheus / Grafana / CloudWatch |
| **Logs** | 运维上下文，含 trace_id 关联 | Loki / SLS / LTS / 本地文件 |

> **审计日志（who/when/what，WORM 不可篡改）→ ADR-09，独立 `AuditLog` trait，不在此 ADR。**

---

## 2. Trait 定义

### TelemetryProvider（Traces + Metrics）

```rust
pub trait TelemetryProvider: Send + Sync {
    /// 获取 Tracer，业务代码用 tracing crate 创建 span
    fn tracer(&self, name: &'static str) -> BoxedTracer;

    /// 获取 Meter，业务代码记录 Counter / Histogram / Gauge
    fn meter(&self, name: &'static str) -> BoxedMeter;

    /// 上报错误事件（Sentry-like 聚合，相同 fingerprint 去重告警）
    fn report_error(&self, err: &dyn std::error::Error, ctx: &ErrorContext);

    /// Graceful shutdown，确保缓冲 span/metric 全部 flush
    async fn shutdown(&self) -> Result<()>;
}

pub struct ErrorContext {
    pub trace_id:    String,
    pub service:     &'static str,
    /// 错误聚合键：相同 fingerprint 的错误合并为一条，避免告警风暴
    pub fingerprint: String,
    pub extra:       HashMap<String, String>,
}
```

### LogSink（运维日志，AirGapped 专用）

```rust
/// 标准部署直接用 tracing-subscriber + OTLP log exporter 即可。
/// LogSink 仅用于无法推 OTLP 的 AirGapped / 离线政企场景。
pub trait LogSink: Send + Sync {
    async fn write(&self, record: &LogRecord) -> Result<()>;
    async fn flush(&self) -> Result<()>;
}

pub struct LogRecord {
    pub timestamp:  DateTime<Utc>,
    pub level:      tracing::Level,
    pub trace_id:   Option<String>,   // 必须传，用于与 Trace 关联
    pub span_id:    Option<String>,
    pub service:    &'static str,
    pub message:    String,
    pub fields:     HashMap<String, serde_json::Value>,  // 结构化，JSON 格式
}
```

---

## 3. 后端映射

### TelemetryProvider 实现

| Profile | Traces | Metrics | Error 聚合 |
|---------|--------|---------|-----------|
| `Standard` | Jaeger（OTLP gRPC） | Prometheus scrape | Sentry self-hosted |
| `Aliyun` | ARMS（OTLP HTTP） | ARMS | ARMS 异常分析 |
| `Huawei` | AOM（OTLP HTTP） | AOM | AOM |
| `AWS` | X-Ray（OTLP via ADOT） | CloudWatch EMF | CloudWatch |
| `AirGapped` | 本地 Jaeger | 本地 Prometheus | 本地 Sentry |

> 国内云（ARMS / AOM / CLS）均已支持 OTLP 协议，**不需要为每家云写专属适配器**，统一用 `OtlpExporter { endpoint }` 变体 + 配置文件切换。

### LogSink 实现（仅 AirGapped）

| Profile | 实现 |
|---------|------|
| `AirGapped` | `LocalFileSink`：JSON 行，按日期 rotate，定期 gzip 归档 |
| 其他 | 不用 LogSink，直接 `tracing-subscriber` + OTLP log exporter |

---

## 4. DeploymentProfile 集成

```toml
# deployment.toml（示例：阿里云）
[telemetry]
backend = "otlp"
endpoint = "https://arms-dc-vpc.cn-hangzhou.aliyuncs.com"
service_name = "ontology-svc"
sample_rate = 0.1          # 生产环境 10% 采样，AirGapped 可设 1.0

[logging]
sink = "otlp"              # AirGapped 改为 "local_file"
local_file_path = "/var/log/palantir"
rotation = "daily"
```

```rust
pub enum TelemetryBackend {
    Otlp {
        endpoint:     String,
        sample_rate:  f64,
    },
    PrometheusLocal {
        scrape_port:  u16,        // Prometheus pull 模式
    },
    // AirGapped 组合：本地 Jaeger + 本地 Prometheus
    AirGapped {
        jaeger_endpoint:      String,
        prometheus_port:      u16,
        sentry_dsn:           Option<String>,
    },
}
```

`InfrastructureContainer::from_profile()` 根据 `deployment.toml` 装配 `TelemetryProvider`，业务代码只用 `tracing::info!` / `tracing::instrument` 宏，完全不感知后端。

---

## 5. 故障定位规范

### 5.1 TraceID 全链路传播

所有 HTTP / gRPC 请求必须传播 **W3C TraceContext**（`traceparent` header）。axum 中间件示例：

```rust
// 自动从 incoming request 提取 trace context，新 span 挂到同一棵树
async fn trace_middleware(req: Request, next: Next) -> Response {
    let parent_ctx = global::get_text_map_propagator(|p| {
        p.extract(&HeaderExtractor(req.headers()))
    });
    let span = tracer.start_with_context("http.request", &parent_ctx);
    // span 生命周期覆盖整个请求处理
    next.run(req).with_context(Context::current_with_span(span)).await
}
```

### 5.2 NATS 消息跨服务追踪

异步消息没有 HTTP header，需在消息体显式携带 trace context：

```rust
#[derive(Serialize, Deserialize)]
struct EventEnvelope<T> {
    trace_id:  String,    // 从当前 span 提取
    span_id:   String,
    payload:   T,
}

// 消费端：从 envelope 恢复 context，新 span 挂到原始 trace
let parent_ctx = restore_context(&envelope.trace_id, &envelope.span_id);
let _span = tracer.start_with_context("nats.consume", &parent_ctx);
```

### 5.3 Workflow 心跳监控

长任务（workflow-svc）每个执行节点发送心跳 metric，防止卡死无感知：

```rust
// 每 30s emit 一次
metrics::gauge!(
    "palantir_workflow_last_heartbeat_seconds",
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as f64,
    "workflow_id" => workflow_id,
    "step" => step_name,
);
```

告警规则（Prometheus）：
```yaml
- alert: WorkflowStuck
  expr: time() - palantir_workflow_last_heartbeat_seconds > 300
  labels:
    severity: warning
  annotations:
    summary: "Workflow {{ $labels.workflow_id }} 步骤 {{ $labels.step }} 超过 5 分钟无心跳"
```

### 5.4 Agent 工具调用追踪

```rust
// 每次工具调用强制 instrument
#[tracing::instrument(
    name = "agent.tool_call",
    fields(
        tool_name = %tool.name(),
        input_hash = %hash_input(input),   // 不记录原始数据（隐私）
        retry_count = tracing::field::Empty,
    )
)]
async fn call_tool(&self, tool: &dyn Tool, input: &Value) -> Result<Value> {
    // ...
    tracing::Span::current().record("retry_count", retry);
}
```

### 5.5 数据库慢查询

| 数据库 | 慢查询定位方式 |
|--------|--------------|
| NebulaGraph | `PROFILE <nGQL>` 分析执行计划；`nebula_graph_slow_queries_total` metric |
| TiDB | 内置 `slow_query` 系统表，兼容 MySQL，Grafana TiDB dashboard 现成可用 |
| Qdrant | `/metrics` 端点暴露 `qdrant_collection_search_duration_seconds` |
| Redis | `SLOWLOG GET` 命令 |

---

## 6. 故障定位 SOP

```
1. 收到告警（Prometheus Alertmanager / 云监控）
   → 告警消息里必须带 trace_id（通过 exemplar 或告警 label）

2. 打开 Jaeger / Tempo / ARMS
   → 搜 trace_id，找最红的 span（耗时最长 / 有 error tag）
   → 确认是哪个服务、哪个操作出问题

3. 跳到日志系统（Loki / SLS）
   → filter: trace_id = "<xxx>"
   → 看具体错误 message 和 fields

4. 根据 span 类型深入：
   - HTTP span 报错  → 看该服务自身 log
   - NATS span 报错  → NATS dashboard 看 consumer lag / unacked 数量
   - DB span 超时    → NebulaGraph PROFILE / TiDB slow_query
   - Workflow 卡住   → 检查 palantir_workflow_last_heartbeat_seconds 告警

5. 如果错误反复出现
   → 查 Sentry / ARMS 异常聚合，同一 fingerprint 的历史记录
   → 判断是偶发还是系统性劣化
```

---

## 7. Rust crate 选型

| 功能 | crate |
|------|-------|
| 业务埋点 | `tracing` + `tracing-subscriber` |
| OTLP 导出 | `opentelemetry-otlp` + `opentelemetry` |
| tracing → OTel 桥接 | `tracing-opentelemetry` |
| Prometheus 本地 scrape | `metrics` + `metrics-exporter-prometheus` |
| 错误上报 | `sentry`（可选，Standard/AirGapped 用） |

---

## 8. 与其他 ADR 的关系

| ADR | 关系 |
|-----|------|
| ADR-09 合规架构 | 审计日志（AuditLog trait）独立于此，不混用 |
| ADR-28 可插拔存储 | 复用 DeploymentProfile 机制，Telemetry 作为新维度加入 |
| ADR-29 服务发现配置中心 | 共享 InfrastructureContainer 装配流程 |
| ADR-31（待讨论） | 可插拔日志收集（AirGapped 归档策略细化） |

---

## 结论

- **OpenTelemetry OTLP** 作为统一中间层，覆盖 Traces + Metrics + Logs，一套代码适配所有云
- **`TelemetryProvider` trait** 屏蔽后端差异，`LogSink` trait 专门处理 AirGapped 离线场景
- **`trace_id` 贯穿** HTTP / gRPC / NATS / 日志，故障定位无需猜测
- **Workflow 心跳 + Agent span** 覆盖系统特有的长任务和非确定性故障场景
- **审计日志不在此 ADR**，ADR-09 独立负责
