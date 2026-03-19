# ADR-05: 离线同步

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

客户端离线时如何同步 Ontology 数据？冲突如何处理？

## 决策

**CRDT Three-Way Merge 内嵌 ontology-svc**，新增 `/v1/sync` 端点。

## 理由

Three-Way Merge 必须原子，跨服务无法保证事务边界，因此内嵌在 ontology-svc。

## 服务端

```
ontology-svc 新增：
  GET  /v1/sync/snapshot        → 全量快照
  POST /v1/sync/delta           → 增量合并（LCA 查找 → Three-Way Merge → 冲突标记）
```

## 客户端

`palantir-sync-client` 独立库：
- 本地 WAL（Write-Ahead Log）
- offline queue（断网期间操作队列）
- delta 发送（恢复网络后推送增量）

## Raft vs CRDT

不竞争，各司其职：
- **Raft**：服务端集群高可用（未来引入）
- **CRDT**：客户端离线合并
