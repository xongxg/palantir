# ADR-14: 四层计算模型

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

数据查询如何分层缓存与计算，兼顾延迟和一致性？

## 决策

**四层计算模型**：L1 Redis / L2 本地内存 / L3 SurrealDB / L4 专项服务。

## 分层定义

```
L1：集中内存（Redis）          延迟 < 1ms    跨服务共享，有 TTL
L2：本地内存（进程内）          延迟 μs 级    服务私有，三种结构
L3：磁盘持久化（SurrealDB）     延迟 ms 级    全量数据，L2 重建来源
L4：专项计算服务（独立进程）     延迟 ms~s 级  embedding / LLM
```

## L2 三种结构

```
DashMap<OntologyId, OntologyObject>   → 点查询，O(1)
Arrow RecordBatch per EntityType      → 分析查询，DataFusion SQL / CEL 编译执行
petgraph                              → 图遍历，BFS / DFS / 最短路径
```

OntologyEvent 到来时三者同步更新：
```rust
fn handle_upsert(object: OntologyObject) {
    self.map.insert(object.id.clone(), object.clone());           // DashMap
    self.batches.entry(&object.entity_type).upsert_row(&object); // Arrow
    self.graph.update_node(&object);                              // petgraph
}
```

## Arrow ↔ Redis 配合

```
服务启动（L2 空）：
  Redis 取 Arrow IPC 快照 → 反序列化 → 暖 L2（毫秒级）
  Redis miss → SurrealDB 重建 → 写回 Redis

DataFusion 查询结果：
  RecordBatch → Arrow IPC → Redis（key = hash(entity_type+query+version), TTL 短）
  下次相同查询 → Redis 命中，跳过 DataFusion

OntologyEvent 到来：
  L2 RecordBatch patch + Redis 失效相关 key（同步）
```

## 查询路由

```
请求进来
  ↓
L1 Redis 命中？→ 返回（< 1ms）
  ↓ miss
L2 本地内存
  ├── 点查询   → DashMap（μs）
  ├── 分析查询 → Arrow + DataFusion（μs~ms）
  └── 图遍历   → petgraph（μs）
  ↓ miss / 冷数据
L3 SurrealDB（ms），结果暖 L1 + L2
  ↓ 需要向量化 / LLM
L4 embedding-svc / agent-svc（ms~s）
```

## 一致性机制

写路径只走 L3，OntologyEvent 驱动 L2 patch + L1 失效，L4 无状态。

## L1 存放内容

| 内容 | Key 规范 | TTL |
|------|---------|-----|
| Semantic Cache | `sc:{hash(query)}` | 短 |
| ReBAC 授权结果 | `authz:{sub}:{res}:{act}` | 短 |
| Workflow 冷却窗口 | `wf:cooldown:{object_id}` | 自定义 |
| Arrow IPC 快照 | `arrow:{entity_type}:snapshot` | 中 |
| Agent Memory 热数据 | `mem:{user_id}:{intent_hash}` | 72h |
| 分布式锁 | `lock:{resource}` | 短 |
