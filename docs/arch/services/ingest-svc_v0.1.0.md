# ingest-svc 子架构

> 状态：设计阶段 | 日期：2026-03-19

## 职责

Source / Mapping 管理、摄入调度、游标续传（Cursor）。

---

## API 端点

```
# Source 管理
POST   /v1/sources                    → 注册数据源
GET    /v1/sources                    → 列出数据源
DELETE /v1/sources/{id}               → 删除

# Mapping 管理
POST   /v1/mappings                   → 创建映射（TOML 配置）
GET    /v1/mappings/{source_id}       → 查看映射
PUT    /v1/mappings/{id}              → 更新映射

# 摄入控制
POST   /v1/ingest/{source_id}/run     → 手动触发摄入
GET    /v1/ingest/{source_id}/status  → 查看状态 + Cursor
POST   /v1/ingest/{source_id}/pause   → 暂停
POST   /v1/ingest/{source_id}/resume  → 恢复

# 文件上传
POST   /v1/uploads                    → 上传文件（RustFS）
GET    /v1/uploads/{id}               → 查询上传状态
```

---

## 摄入流程

```
SourceAdapter.stream(cursor)
  ↓
TomlMapping.apply(record) → Vec<OntologyEvent>
  ↓
ontology-svc HTTP POST /v1/objects
  ↓
cursor 更新（断点续传）
```

---

## 游标续传

```rust
pub struct IngestCursor {
    pub source_id: String,
    pub position: CursorPosition,  // 文件行数 / 时间戳 / offset
    pub last_updated: DateTime,
}
```

摄入中断后从 cursor 位置恢复，不重复处理已摄入数据。

---

## 复用 crate

- `palantir-ontology-manager`：CsvAdapter、TomlMapping、OntologyEvent

---

## 支持的 Source 类型

| 类型 | Adapter | 状态 |
|------|---------|------|
| CSV 文件 | CsvAdapter | ✅ 已实现 |
| 数据库（JDBC）| DbAdapter | 待实现 |
| REST API | HttpAdapter | 待实现 |
| 消息队列 | MqAdapter | 待实现 |

---

## 待细化

- [ ] 摄入任务调度策略（定时 / 事件触发）
- [ ] 错误重试 + 死信队列
- [ ] 摄入进度 SSE 推送

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本，架构设计阶段 |
