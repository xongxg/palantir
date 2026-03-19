# ADR-20: 内部服务通信 — gRPC

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

内部服务间同步调用用什么协议？HTTP + JSON 还是二进制 RPC？

## 决策

**内部同步调用 → gRPC（tonic + protobuf）**

## 协议分层

```
外部（用户 / 前端）
  → api-gateway → HTTP + JSON + SSE（对外友好，保持不变）

内部（服务间同步调用）
  → gRPC（tonic + protobuf，强类型、二进制、低延迟）

异步事件
  → NATS JetStream（保持不变）
```

## 理由

| 对比项 | HTTP + JSON | gRPC + protobuf |
|--------|------------|-----------------|
| 序列化体积 | 大 | 小 3-10x |
| 序列化速度 | 慢 | 快 5-10x |
| 类型安全 | 运行时 | 编译期（proto 生成代码）|
| 流式支持 | 靠 SSE | 原生双向流 |
| 调试难度 | 容易（JSON 可读）| 稍难（需工具）|

## 技术选型

- `tonic`：Rust gRPC 框架，tokio 原生异步
- `prost`：protobuf 序列化

## 实现顺序

**MVP**：内部地址写死到环境变量，HTTP 先跑通
**P1**：引入 gRPC，定义 proto 文件，服务间切换为 tonic 调用
**P2**：结合 ADR-21 服务发现，gRPC 地址动态获取
