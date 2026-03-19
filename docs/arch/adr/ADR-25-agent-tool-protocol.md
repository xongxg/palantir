# ADR-25: Agent 工具调用协议

> 状态：✅ 已决策 | 日期：2026-03-19 | 实现阶段：待定（暂不实现）

## 问题

Agent 调用内部 API 和外部集成时，应该统一用 Tool Calling（API 包装成 Tools）还是 MCP（Model Context Protocol）？

## 决策

**分场景混用：内部 API → Tool Calling；外部集成 → MCP Client。**

## 两种方式定位

| 维度 | Tool Calling | MCP |
|------|-------------|-----|
| 适用场景 | 内部受控服务 | 外部 / 第三方集成 |
| 协议 | 自定义（OpenAI schema）| 标准化（JSON-RPC over SSE / stdio）|
| 工具发现 | 代码注册（宏 / 静态）| 动态发现（MCP 握手）|
| 扩展方式 | 改代码加新 Function | 接入新 MCP Server，无需改 agent |
| 内部通信 | gRPC（ADR-20）| HTTP / SSE（MCP 协议层）|
| 生态 | 项目自闭环 | 外部 MCP Server 生态即插即用 |

## 架构分层

```
agent-svc
  ├── 内部工具（Tool Calling）
  │     ├── Ontology 查询           → gRPC → ontology-svc
  │     ├── Workflow 触发           → gRPC → workflow-svc
  │     └── #[ontology_function]    → gRPC → function-svc
  │
  └── 外部工具（MCP Client）
        ├── MCP Server A（Slack）   → 标准 MCP 协议
        ├── MCP Server B（GitHub）  → 标准 MCP 协议
        └── MCP Server C（自定义） → 标准 MCP 协议
```

## 内部用 Tool Calling 的理由

- ADR-02 的 `#[ontology_function]` 宏已自动生成 OpenAI tool schema，无需额外封装
- 内部走 gRPC 延迟更低，MCP 协议层是不必要的开销
- 两端均自己控制，标准化互操作性收益不大

## 外部用 MCP 的理由

- 第三方 MCP Server 即插即用（Slack、GitHub、数据库等已有大量现成实现）
- agent-svc 只需实现一个 MCP Client，即可对接所有外部工具
- 新外部集成无需改 agent 代码，只需注册新 MCP Server

## 实现骨架（设计参考，暂不实现）

```rust
pub struct AgentExecutor {
    // 内部工具注册表（来自 function-svc）
    tool_registry: Arc<dyn FunctionRegistry>,
    // MCP Client（外部工具）
    mcp_client: Arc<McpClient>,
}

impl AgentExecutor {
    async fn execute(&self, plan: ExecutionPlan) -> Vec<ToolResult> {
        match plan.tool_source {
            ToolSource::Internal(id) =>
                self.tool_registry.invoke(id, plan.args).await,
            ToolSource::Mcp(server, tool) =>
                self.mcp_client.call(server, tool, plan.args).await,
        }
    }
}
```

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策，暂不实现 |
