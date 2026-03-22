# ADR-37: Ontology 建模以领域语义为主，Dataset 列结构为辅

> 日期：2026-03-21
> 状态：已采纳
> 关联：ADR-35（Ontology Upsert Identity）、ADR-36（Raw vs Typed）、ingest-workflow_v0.5.0

---

## 背景

在为新 Dataset 配置 Ontology 映射时，系统会自动推导列结构（列名、类型、样例值）。
当新 Dataset 的列与已有 Entity Type 高度相似时，产生了"是否复用/继承 Schema"的问题。

典型场景：`aircraft` 已有 Schema + 实例，新业务 `airline` 列高度重合但有新增属性。

---

## 决策

**不引入 Schema 继承、Fork 或复用机制。**

Object Type 的定义以**业务领域语义**为准，Dataset 列结构仅作辅助参考，不驱动 ET 的设计。

---

## 理由

### 1. 相似的列不代表相同的业务实体

`aircraft.manufacturer` 和 `airline.manufacturer` 字面相同，语义不同：

| 字段 | 所属 ET | 语义 |
|------|---------|------|
| manufacturer | Aircraft | 这架飞机的制造商 |
| manufacturer | Airline  | 这家公司主要运营的机型制造商 |

Schema 相似是行业属性的巧合，不是"两个实体相同"的证明。

### 2. 复用 Schema 会污染语义

将 `airline` 数据 promote 到 `aircraft` ET（字段别名映射），或 Fork aircraft Schema 给 airline，都会导致：
- 两个业务概念的实例混在同一个 ET 里，查询时需要额外过滤
- 字段的业务含义随使用者不同而漂移，失去 Ontology 作为 Single Source of Truth 的价值

### 3. Palantir 的实践

Palantir Foundry 没有 Object Type 继承机制。
相似实体通过 **Link Type** 建立关联，通过 **Interface**（共享属性契约）处理跨类型多态查询。
两者均不要求 Schema 复用，只要求语义对齐。

---

## 正确建模方式

```
Airline ──[OPERATES]──▶ Aircraft
```

- `Airline` 独立定义：iata_code、hub_airport、route、... + 恰好和 Aircraft 重合的字段
- `Aircraft` 独立定义：tail_number、max_range、... + 恰好和 Airline 重合的字段
- 字段重叠是**可以接受的正常现象**，不需要技术手段消除

---

## 对映射向导的影响

向导以"业务实体命名"为第一步，引导用户先确认语义，再看列结构：

```
① 业务确认  "这个 Dataset 代表什么业务实体？" [Airline        ]
② Schema 发现  推导列结构，用户决定哪些列是该 ET 真正需要的属性
③ 映射配置  列名 → 属性名（支持别名、忽略）
④ 关联推断  _id/_fk 列 → 建议创建 Link Type → 用户确认
```

Dataset 的列只决定"能有什么属性"，**不决定"应该有什么属性"**。

---

## 不采纳的方案

| 方案 | 拒绝理由 |
|------|---------|
| Schema Fork | 误把数据结构相似当业务相似，产生语义漂移 |
| 字段别名映射到已有 ET | 不同业务实体的数据混入同一 ET，查询污染 |
| Interface 抽象（当前） | P1 级别，等出现 3 个以上 ET 共享字段时再引入，避免过度设计 |
| 基类 ET / 继承 | Ontology 不是 ORM，领域语义不应由代码继承关系表达 |

---

## 后续演进

当多个 ET 确实存在语义上共同的属性集（不只是列名相似）时，引入 **Interface**：

```
Interface: AviationEntity
  ├── manufacturer: string  ← 业务含义统一的字段
  └── capacity: integer

Aircraft implements AviationEntity + 飞机特有字段
Airline  implements AviationEntity + 公司特有字段
```

触发条件：**≥3 个 ET 有语义相同的字段集，且跨类型查询是真实需求**。
