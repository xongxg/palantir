# ADR-27: SurrealDB 依赖风险与替代方案

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

当前架构对 SurrealDB 依赖过重（业务数据 + 身份 + 权限 + 审计 + Agent Memory 全压在一个较新的数据库上），成本和风险如何评估？有哪些替代方案？

## SurrealDB 依赖成本

| 成本维度 | 程度 | 说明 |
|---------|------|------|
| 学习成本 | 中 | SurrealQL 自成一套，非标准 SQL |
| 招聘成本 | 高 | 市场上懂 SurrealDB 的工程师极少 |
| 运维成本 | 高 | 监控/备份/故障排查工具不成熟 |
| 生产稳定性 | 未知 | 2023 年才发布 1.x，缺乏大规模生产验证 |
| 社区资源 | 低 | 遇到问题 Stack Overflow 基本找不到答案 |
| 迁移成本 | 高 | SurrealQL 查询无法直接移植到其他数据库 |

## 我们真正需要的四项能力

```
① 文档模型（OntologyObject 灵活 schema）
② 原生图遍历（MANAGES / BELONGS_TO 多跳）
③ 向量搜索（Agent Memory）
④ Rust SDK
```

## 替代方案全景

### 方案 A：Postgres + Apache AGE + pgvector（推荐）

```
Postgres JSONB + GIN  → ① 文档模型
Apache AGE（扩展）     → ② 图遍历（openCypher 语法）
pgvector（扩展）       → ③ 向量搜索
sqlx crate            → ④ Rust SDK（最成熟）
```

**优势**：单库运维，Postgres 最成熟，sqlx 是 Rust 生态最好的 DB crate，监控/备份/招聘全成熟
**劣势**：Apache AGE 图能力弱于原生图，深度遍历（> 4 跳）性能下降

---

### 方案 B：MySQL 8.0

```
MySQL JSON 类型        → ① 文档模型（弱于 Postgres JSONB）
递归 CTE（MySQL 8.0）  → ② 图遍历（应用层，性能差）
无原生向量搜索         → ③ 需要额外引入 Milvus / Qdrant
sqlx crate            → ④ Rust SDK（支持 MySQL）
```

**优势**：国内使用率最高，运维经验丰富，DBA 多
**劣势**：
- JSON 能力弱于 Postgres JSONB（无 GIN 索引）
- 无图扩展（无 AGE 等价物），图遍历必须在应用层
- 无向量搜索，需要额外引入组件
- 超过 3 跳的图查询写起来极其繁琐

**适合场景**：纯结构化数据 + 不需要图遍历的子系统（如审计日志、权限配置）

---

### 方案 C：TiDB（PingCAP）

```
TiDB（MySQL 兼容）     → ① 文档模型（JSON 支持）
递归 CTE               → ② 图遍历（同 MySQL，应用层）
TiFlash（列式副本）    → 分析查询加速
sqlx / MySQL driver    → ④ Rust SDK
```

**优势**：MySQL 兼容（迁移成本低）、水平扩容原生支持、国内生态完善、PingCAP 支持好
**劣势**：同 MySQL，无原生图能力
**适合场景**：数据量大、需要水平扩容的子系统；国内私有化部署

---

### 方案 D：国内云数据库

| 产品 | 厂商 | 兼容协议 | 适合场景 |
|------|------|---------|---------|
| PolarDB（PG 版）| 阿里云 | PostgreSQL | 阿里云部署，serverless，pgvector 支持 |
| PolarDB-X | 阿里云 | MySQL | 分布式，大数据量 |
| TDSQL | 腾讯云 | MySQL | 金融级强一致，腾讯云部署 |
| OceanBase | 蚂蚁 / 阿里 | MySQL / Oracle | 超大规模 HTAP，金融场景 |
| GaussDB | 华为云 | PostgreSQL | 华为云部署，政企场景 |
| NebulaGraph | Vesoft | 原生图 | 中国产图数据库，开源，生产案例多 |

**NebulaGraph 单独说明**：
- 中国产原生图数据库，开源，有 Rust SDK
- 生产案例：美团、京东、快手等
- 图遍历性能接近 Neo4j
- 适合替换 SurrealDB 的图遍历部分

---

### 方案 E：Neo4j + Postgres

```
Neo4j                  → ② 图遍历（业界最强，Cypher）
Postgres               → ① 文档模型 + 其他结构化数据
pgvector               → ③ 向量搜索
```

**优势**：图能力最强，生产案例多
**劣势**：两库运维，Rust SDK 非官方，企业版扩容贵
**适合场景**：图遍历是核心且深度 > 4 跳的场景

---

## 综合对比

| 方案 | 图能力 | 成熟度 | 运维复杂度 | Rust SDK | 国内生态 | 招聘难度 |
|------|--------|--------|-----------|---------|---------|---------|
| SurrealDB（现状）| ✅ 原生 | ⚠️ 新 | 中 | ✅ 官方 | ❌ 几乎无 | 🔴 极难 |
| Postgres + AGE | ⚠️ 扩展 | ✅ 最成熟 | 低 | ✅ sqlx | ✅ 好 | 🟢 容易 |
| MySQL 8.0 | ❌ 应用层 | ✅ 最成熟 | 最低 | ✅ sqlx | ✅ 最好 | 🟢 最容易 |
| TiDB | ❌ 应用层 | ✅ 较成熟 | 中 | ✅ MySQL | ✅ 很好 | 🟢 容易 |
| PolarDB（PG）| ⚠️ 扩展 | ✅ 成熟 | 低（云托管）| ✅ sqlx | ✅ 阿里云 | 🟢 容易 |
| NebulaGraph + PG | ✅ 原生 | ✅ 较成熟 | 中 | ✅ 有 | ✅ 好 | 🟡 中等 |
| Neo4j + Postgres | ✅ 最强 | ✅ 成熟 | 高 | ⚠️ 社区 | ⚠️ 一般 | 🟡 中等 |

---

## 决策

### 短期（MVP）

**维持 SurrealDB，但收窄范围**（配合 ADR-26 讨论）：

```
SurrealDB：仅承载 Ontology 图核心（TBox + ABox + Relationship）
Postgres / MySQL：身份、权限配置、审计日志、Agent Memory 元数据
```

爆炸半径缩小，SurrealDB 问题只影响 Ontology 图，不影响身份和权限。

### 中期迁移路径（若 SurrealDB 生产稳定性不达标）

**国内私有化部署 → NebulaGraph + MySQL / TiDB**

```
NebulaGraph     → 图遍历（替换 SurrealDB 图能力）
MySQL / TiDB    → 文档存储 + 结构化数据
Qdrant          → 向量搜索
```

**云部署（阿里云）→ PolarDB（PG 版）+ pgvector + Apache AGE**

```
PolarDB（PG）   → 文档 + 结构化 + 向量（pgvector）+ 图（AGE）
云托管，运维成本最低
```

### 逃生门

`OntologyRepository` trait 已抽象，换实现不改业务代码：

```rust
pub trait OntologyGraphStore: Send + Sync {
    async fn relate(&self, from: &OntologyId, rel: &str, to: &OntologyId) -> Result<()>;
    async fn traverse(&self, from: &OntologyId, depth: u8) -> Result<Vec<OntologyObject>>;
}

// 实现：
// SurrealDbStore（当前）
// NebulaGraphStore（迁移备选）
// PostgresAgeStore（保守备选）
```

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策：SurrealDB 依赖风险分析 + MySQL/TiDB/国内云数据库/NebulaGraph 替代方案 |
