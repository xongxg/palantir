# ADR-18: L2 计算引擎 — Apache Arrow + DataFusion

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

L2 本地内存的分析查询用什么计算引擎？

## 决策

**Apache Arrow RecordBatch + DataFusion**，Arrow IPC 序列化到 Redis。

## 理由

- Arrow 列式存储，分析查询比行式快 10-100x
- DataFusion 纯 Rust，与项目技术栈一致
- CEL 表达式可编译为 DataFusion 执行计划
- Arrow IPC 序列化紧凑，Redis 传输效率高

## 数据模型

```
每个 EntityType 对应一个 Arrow RecordBatch
列 = EntityType 的属性字段
行 = 每个 OntologyObject 实例
```

## 与 Redis 配合

```
查询结果：RecordBatch → Arrow IPC → Redis（短 TTL）
启动预热：Redis Arrow IPC → 反序列化 → L2 RecordBatch
事件更新：patch RecordBatch + 失效 Redis 相关 key
```

## 使用场景

- CEL 表达式执行（`employees.filter(e => e.salary > 50000)`）
- 跨 EntityType 分析（JOIN + GROUP BY）
- Function 计算推导

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
