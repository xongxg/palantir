# function-svc 子架构

> 状态：设计阶段 | 日期：2026-03-19

## 职责

Function / Logic 注册与执行，CEL 表达式引擎，自然语言生成 CEL。

---

## API 端点

```
# Function 注册
GET    /v1/functions                  → 列出所有 Function
GET    /v1/functions/{id}             → 查看详情（含 schema）
POST   /v1/functions/{id}/invoke      → 调用执行

# CEL Logic（业务分析师）
POST   /v1/logics                     → 创建 CEL Logic
PUT    /v1/logics/{id}                → 更新
DELETE /v1/logics/{id}                → 删除
POST   /v1/logics/{id}/invoke         → 执行

# 自然语言辅助
POST   /v1/logics/generate            → 业务描述 → 生成 CEL（需人工确认）
```

---

## 执行模型（ADR-02）

```
Layer 1：Rust 编译时注册
  #[ontology_function]
  fn calculate_total(order: &Order) -> f64 { ... }
  → 自动注册 + 生成 OpenAI tool schema

Layer 2：CEL 表达式
  "employees.filter(e => e.department == dept).map(e => e.salary).sum()"
  → cel-interpreter crate 执行
  → Monaco Web IDE + Schema 感知补全

Layer 3：WASM 沙箱（接口占坑，暂不实现）
```

---

## 自然语言路径

```
业务描述（自然语言）
  ↓
agent-svc（注入 Schema）→ LLM 生成 CEL
  ↓
用户在 Monaco IDE 确认 / 修改
  ↓
POST /v1/logics 保存
```

---

## FunctionRegistry

```rust
pub trait FunctionRegistry {
    fn register(&mut self, meta: FunctionMeta, handler: BoxedHandler);
    fn get(&self, id: &str) -> Option<&FunctionMeta>;
    fn list(&self) -> Vec<&FunctionMeta>;
    async fn invoke(&self, id: &str, ctx: InvokeContext) -> InvokeResult;
}
```

---

## 复用 crate

- `palantir-function-core`：Function / Logic trait + FunctionRegistry

---

## 待细化

- [ ] CEL 执行沙箱（超时、内存限制）
- [ ] Function 版本管理
- [ ] 调用链追踪（与 AgentTrace 集成）
- [ ] `build.rs` codegen 流程设计

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本，架构设计阶段 |
