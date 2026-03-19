# ADR-17: Agent 流式协议

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

Agent 查询结果如何流式推送给客户端？

## 决策

**现阶段 SSE，中期升级 WebSocket，WebRTC 仅语音功能时引入**。

## 演进路径

| 阶段 | 协议 | 触发条件 |
|------|------|---------|
| 现阶段 | SSE（单向，Server → Client）| 默认，文本流式输出 |
| 中期 | WebSocket（双向全双工）| Proactive Agent 主动推送 / 用户打断生成 |
| 未来 | WebRTC（P2P 音视频）| 语音输入输出功能上线时 |

## "停止生成"不依赖 WebSocket

独立 HTTP 接口 + 后端 `CancellationToken`：

```
POST /v1/query              → 发起查询，返回 SSE stream
POST /v1/query/{id}/cancel  → 取消，后端 token.cancel()
```

## SSE 和 WebSocket 并存

agent-svc 同时暴露两个端点，客户端按能力选择，互不干扰。

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
