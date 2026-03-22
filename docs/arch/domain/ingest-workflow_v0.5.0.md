# 数据接入工作流设计 v0.5.0

> 版本：v0.5.0
> 日期：2026-03-21
> 状态：P0 已实现，P1 规划中
> 前置文档：[ingest-workflow v0.4.0](ingest-workflow_v0.4.0.md) · [ADR-35 Ontology Upsert Identity](../adr/ADR-35-ontology-upsert-identity.md) · [ADR-36 Ontology Raw vs Typed](../adr/ADR-36-ontology-raw-vs-typed.md)

---

## 变更记录

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.4.0 | 2026-03-20 | Palantir 四层对位；七次迭代路线图；palantir-storage crate 设计 |
| **v0.5.0** | **2026-03-21** | **Sources ↔ Ontology 管道打通；Schema-first 导入流程；关联关系自动推断；`is_required` Bug 修复；Fold 范围关联推断原则；Project/Fold 业务层级语义** |

---

## 一、Sources ↔ Ontology 管道

### 背景问题

v0.4.0 之前，Sources 和 Ontology 是两个孤立流程：
- Sources：连接器 → sync → S3（raw files）→ 完成
- Ontology：手动 Import tab → 手动 Promote

用户每次 sync 后必须手动去 Ontology 页重新 promote，操作不连贯。

### 解决方案：`object_type_mappings` 作为桥接

一次配置映射，后续每次 sync 自动触发 promote：

```
DataSource ─sync─▶ S3(manifest + CSV parts)
                          │
                          ▼
              object_type_mappings
              (dataset_id → entity_type_id + pk_col + field_mapping)
                          │
                          ▼
              auto_promote_if_mapped()    ← sync 完成后自动调用
                          │
                          ▼
              OntologyObject (upsert，主键去重)
```

### 数据库表

```sql
CREATE TABLE object_type_mappings (
    id              TEXT PRIMARY KEY,
    dataset_id      TEXT NOT NULL UNIQUE,
    entity_type_id  TEXT NOT NULL,
    primary_key_col TEXT NOT NULL DEFAULT '',
    field_mapping   TEXT NOT NULL DEFAULT '{}',  -- JSON: {src_col: target_attr}
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
)
```

### API

| 方法 | 路径 | 用途 |
|------|------|------|
| `POST` | `/api/datasets/:id/mapping` | 保存/更新映射配置 |
| `GET`  | `/api/datasets/:id/mapping` | 读取当前映射，供 UI 回填 |
| `POST` | `/api/datasets/:id/promote` | 手动触发 promote（含保存 mapping） |

### 请求体（POST mapping）

```json
{
  "entity_type_id":  "uuid",
  "primary_key_col": "id",
  "field_mapping":   { "src_col": "target_attr" }
}
```

### `auto_promote_if_mapped` 逻辑

```
1. db().get_object_type_mapping(dataset_id) → 无映射则 return
2. 读 S3 manifest → 获取 part 文件列表
3. 逐 part 读 CSV → 解析 records
4. 按 field_mapping 转换字段名
5. 按 primary_key_col 提取 external_id
6. db().upsert_ontology_object(entity_type_id, external_id, properties) × N
```

幂等：相同 `(entity_type_id, external_id)` ON CONFLICT DO UPDATE，重复 sync 不重复写入。

---

## 二、映射向导设计原则：自动优先，确认为辅

### 核心原则

对于标准格式数据（CSV / JSON / SQL），**80% 以上的映射工作可以自动完成**。映射向导的目标是让系统做推断，用户只在关键语义节点上确认，而不是逐列手动配置。

| 自动完成 | 原理 |
|---------|------|
| 列名 → 属性名 | 直接 1:1 映射 |
| 类型推断 | string/number/boolean/date 规则推导 |
| 主键识别 | `id`/`_id`/`{entity}_id` 命名约定 |
| FK 关联推断 | `_id`/`_fk` 后缀匹配已有 ET |
| 同 Fold 内关联 | 置信度最高，默认勾选 |

**用户只需决策：**
1. 业务实体名称（"这是什么业务概念？"）
2. 哪些列需要忽略（从"全部保留"中排除）
3. 跨 Fold 关联是否确认

### UX 模式：审查排除，而非逐个添加

```
❌ 旧模式：空白起点，用户逐列配置
✅ 新模式：系统默认全部自动映射，用户审查并排除不需要的
```

映射向导打开时，所有列默认选中，推断的主键/FK/类型已预填，用户以"确认 + 微调"的姿态操作。

---

## 三、Ontology Import — Schema-first 流程

### 旧流程（问题）

```
上传文件 → 直接 Promote → OntologyObject（同时写 schema + 实例）
```

问题：无法只保存 schema，每次"保存 Entity Type"都会创建实例。

### 新流程（Schema-first）

```
上传文件
  │
  ▼
Schema 发现（列类型推断：string/number/boolean/date）
  │
  ▼
"保存为 Entity Type"  ← 仅保存 EntityType + fields，不写任何实例
  │
  ▼
"Promote → Ontology"  ← 独立步骤，写数据实例（幂等 upsert）
```

两步完全分离，用户可以：
- 只定义 schema，暂不 promote
- 修改 schema 后再 promote
- 多次 promote 不产生重复数据（主键去重）

### is_required 字段对齐

前端 JS 发送 `is_required`，后端 `AddFieldReq` 接收 `is_required`（已修复历史 Bug：之前发送 `required` 被 serde 忽略，导致所有字段 `is_required=false`）。

---

## 三、关联关系自动推断

详见 [§ 四：关联发现方案](#四关联发现方案)。

---

## 四、关联发现方案

### 问题背景

当用户导入一个 Dataset 时，它的某些列可能是对另一个 Entity Type 的外键引用。如何自动或半自动地发现这些关联，是 Schema-first 流程的重要组成部分。

### 方案一：列名约定匹配（已实现，P0）

**原理**：列名以 `_id` 或 `_fk` 结尾的列，很可能是对另一个实体的引用。

```
airport_id  → 去掉 _id  → "airport"  → 匹配 EntityType "Airport" (case-insensitive)
airline_fk  → 去掉 _fk  → "airline"  → 匹配 EntityType "Airline"
```

**实现**：`autoInferRelationships()` 函数，扫描当前列列表，匹配已有 EntityType 名称。

**优点**：零配置，即时可用；FK 命名约定是行业惯例，误判率低。
**缺点**：依赖命名规范；无法发现非标准命名的关联（如 `carrier` → Airline）。
**适用**：标准规范的数据库导出，工程团队数据。

---

### 方案二：值域重叠匹配（P1）

**原理**：列 A 的值集合与另一个 Dataset 的主键列值集合有显著重叠，说明可能存在关联。

```
aircraft.csv: airline_code = [AA, UA, DL, ...]
airlines.csv: code         = [AA, UA, DL, ...]   → 重叠率 > 阈值 → 推断关联
```

**实现思路**：
1. promote 时记录每个 EntityType 的主键值集合（Bloom Filter 或 HashSet，存 DB）
2. 新 Dataset schema 发现时，对所有列做值域抽样（取 top 1000 值）
3. 与已有 EntityType 主键集合做 Jaccard 相似度
4. 相似度 > 0.3（可配置）则推断关联

**优点**：不依赖命名，可发现任意列名的关联。
**缺点**：计算成本较高；需要对已有 ET 数据做预处理；可能有误判（如 status 列 = [0,1] 与 bool 型主键重叠）。
**适用**：非标准命名的历史系统数据。

---

### 方案三：语义名称匹配（P1）

**原理**：列名与 EntityType 名称虽不完全匹配，但语义相近。

```
"carrier"   ≈ "Airline"   (语义相似)
"vendor_id" ≈ "Supplier"  (语义相似)
```

**实现思路**：
- 轻量：预置同义词词典（`carrier → airline`, `vendor → supplier`）
- 中等：字符串相似度（编辑距离、n-gram）
- 重量：LLM embedding 相似度（本地 embedding 模型）

**优点**：覆盖语义等价的不同命名。
**缺点**：同义词词典需维护；embedding 方案引入模型依赖。
**适用**：多部门协作、历史遗留系统合并场景。

---

### 方案四：用户历史映射学习（P2）

**原理**：用户手动确认过的关联记录下来，相同列名 + 相同 EntityType 组合出现时自动预填。

```
用户上次：aircraft.csv[airline_id] → Airline  （手动确认）
下次上传：flight_records.csv[airline_id]  → 自动建议 → Airline
```

**实现**：在 `object_type_mappings` 或独立的 `user_relation_hints` 表中记录已确认的映射模式。

**优点**：越用越准，个性化。
**缺点**：冷启动无数据；需要持续学习基础设施。
**适用**：用户积累一段时间后。

---

### 方案汇总与实施优先级

| 方案 | 原理 | 精度 | 成本 | 优先级 |
|------|------|------|------|--------|
| 列名约定匹配 | `_id`/`_fk` 后缀 | 中高 | 极低 | **P0 ✅ 已实现** |
| 值域重叠匹配 | 主键值集合 Jaccard | 高 | 中 | P1 |
| 语义名称匹配 | 同义词词典 / embedding | 中 | 中高 | P1（词典）/ P2（embedding） |
| 历史映射学习 | 用户行为积累 | 高 | 低（依赖积累） | P2 |

**设计原则**：多方案并行推断，结果做置信度排序，呈现给用户确认，不强制自动应用。用户确认 = 反馈信号，持续改进推断准确率。

---

## 五、数据层级与关联发现范围

### 业务层级模型

```
Project（大业务线）
  └── Fold（子业务线 / 子域）
        └── DataSource（具体数据源，如 CSV 文件）
              └── Dataset（每次 sync 的一个版本快照）
```

对应数据库：`projects → folds → data_sources → datasets`

### CSV 批量上传的业务语义

业务部门上传 CSV 时，通常是**一批业务相关数据一起推送**：同一个 Fold 下的多个 CSV 是由同一业务域统一设计的，它们之间的关联关系是**预先设计好的**，不需要猜测。

```
Fold: 航空运营
  ├── aircraft.csv      → Aircraft ET
  ├── airline.csv       → Airline ET
  ├── route.csv         → Route ET
  └── flight_log.csv    → FlightLog ET

aircraft.airline_id → Airline（同 Fold 内，高置信度关联）
```

### Fold 范围关联推断原则

| 关联范围 | 置信度 | 推断策略 | 用户操作 |
|---------|--------|---------|--------|
| 同 Fold 内 `_id`/`_fk` 匹配 | 极高 | 直接推荐，默认勾选 | 一键确认即可 |
| 同 Fold 内值域重叠匹配 | 高 | 推荐，需用户确认 | 查看样例后确认 |
| 跨 Fold 列名/值域匹配 | 中 | 提示，默认不勾选 | 主动添加 |
| 跨 Project | 低 | 不自动推断 | 手动配置 Link Type |

**核心原则**：Fold 是关联推断的天然边界。同 Fold 内的关联发现置信度最高，应优先展示；跨 Fold 的关联需要更明确的用户意图。

### 对映射向导的影响

Step 3（关联推断）展示关联候选时：
1. 优先展示**同 Fold** 内的 ET 作为关联目标
2. 跨 Fold 的候选单独分组（"其他 Fold 中的 Entity Types"）
3. 关联置信度标注来源（列名约定 / 值域重叠 / 跨 Fold）

---

## 六、已修复的关键 Bug

| Bug | 现象 | 根因 | 修复 |
|-----|------|------|------|
| `is_required` 字段丢失 | ET 保存后所有字段 `is_required=false` | 前端发 `required`，后端 `AddFieldReq` 字段名为 `is_required`，serde 忽略未知字段 | 改为 `is_required` 对齐后端 |
| 重复 promote 产生重复实例 | 同一 ET 出现 14 条相同对象 | `ontology_objects` 无 UNIQUE 约束，每次 promote = INSERT | 加 `UNIQUE(entity_type_id, external_id)` + ON CONFLICT DO UPDATE |
| `SaveMappingReq` 命名冲突 | 编译失败 | main.rs 中两处 struct 同名 | 新 struct 重命名为 `SaveDatasetMappingReq` |
| Schema tab 不刷新 | 切换到 Schema tab，ET 列表为空 | `setTab('schema')` 未调用 `loadEntityTypes()` | 在 setTab 中加 `if (t === 'schema') loadEntityTypes()` |
| S3 多文件同步只生成 1 个 Dataset | hr_ds 同步 3 个文件，Ontology Schema 只有 1 条可配置 | S3 source 走 single-file 模式，所有选中文件写入同一 dataset_id | 新增 S3 per-file 模式：`selected_files.len() > 1` 时每个文件独立创建 Dataset + SyncRun，对称 CSV folder 模式 |

---

## 六、下一步（Iter 对应）

| 任务 | 优先级 | 对应 Iter |
|------|--------|---------|
| `list_dataset_records` 加 `sync_run_id != 'promote'` 过滤（方案 A） | P0 | 当前 |
| 值域重叠关联推断 | P1 | Iter-3 |
| 增量 promote（diff by external_id） | P1 | Iter-4 |
| LinkType 数据驱动（边 Dataset：from_id, to_id 两列） | P1 | Iter-5 |
| Transform Pipeline 雏形 | P2 | Iter-5 |

---

## 七、技术储备：REST API 与 Streaming Event 的映射推导

### REST API（P1）

REST 源返回 JSON，结构通常稳定，推导策略：

```
① Preview 采样（已有 /api/sources/preview）
   → 调用一次 API 取前 N 条，展开 JSON 提取所有 key 路径（含嵌套）
② Schema 发现
   → 嵌套 JSON 支持 Flatten（user.address.city → address_city）
   → 用户决定保留/忽略哪些 key
③ 主键推断
   → 优先 id / uuid / {entity}_id 字段
   → 没有则建议用 URL 路径参数（/users/:id 的 :id）
④ 关联推断
   → _id 后缀（同 CSV）
   → href/url 字段指向其他资源（P1 语义推断）
```

**难度**：低。接口版本固定，Schema 稳定，和 CSV 基本对称。

---

### Streaming Event（Kafka / WebSocket / SSE）（P2）

Event 流的两个本质挑战：
1. **Schema 随时间漂移**（早期和最新 event 字段可能不同）
2. **同一 Stream 混入多种 Event 类型**（OrderCreated / OrderUpdated / OrderCancelled）

推导策略：

```
① 采样窗口
   → 拉最近 K 条事件（建议 1000 条）做 schema 发现
   → 统计每个 key 出现频率，频率 > 阈值（如 80%）才算稳定字段

② Event 类型识别
   → 检查 type / event_type / kind / action 字段是否存在
   → 若存在，按 event_type 分组，每组单独推导 Schema
   → 决策点：每种 event_type → 独立 ET（精细）
              OR 合并为一个 ET，event_type 作为状态字段（简化）

③ 时序主键
   → Event 无天然主键，策略：
      - event_id（全局唯一）→ 直接用
      - (stream_id + offset/sequence_number) → 复合主键
      - 业务聚合：同 order_id 的所有事件 → 同一 Order ET 实例（CQRS 模式）

④ 关联推断
   → _id 后缀（同 CSV），值域重叠
   → event payload 中的 entity_id 字段通常是关联 FK
```

**难度**：高。Schema 不稳定，需要采样窗口 + 频率统计。Event 聚合键的设计是领域问题，系统只能建议，用户必须确认。

---

### 三种源类型对比

| 维度 | CSV/JSON 文件 | REST API | Streaming Event |
|------|-------------|---------|----------------|
| Schema 稳定性 | 高 | 高 | 低 |
| 主键来源 | id 列 | URL 参数 / id 字段 | event_id / 业务聚合键 |
| 关联发现 | _id 后缀 | _id 后缀 + href | entity_id 字段 |
| 自动化程度 | 80%+ | 70%+ | 40%~60% |
| 当前实现 | P0 ✅ | Preview 已有，schema 推导 P1 | P2 |


---

## 八、Dataset 数据管理策略：Sync Mode

### 问题

同一个数据源（如 employees.csv）第二次推送过来时，和第一批数据是什么关系？不能直接覆盖，也不能无限累积——需要明确的数据管理语义。

### Palantir Foundry 的方案

Foundry 在 Dataset 写入时必须声明 **Transaction Type**：

| 类型 | 语义 | 适用场景 |
|------|------|---------|
| `SNAPSHOT` | 全量替换，当前版本 = 本次全部数据 | 每次推送完整数据集 |
| `APPEND` | 追加，历史行保留，新行累加 | 日志、事件流、增量导出 |
| `UPDATE` | 按主键 upsert，修改已有行 | 状态同步、缓慢变化维度 |

每次写入 = 一个 Transaction，保留完整 Transaction Log，可按 timestamp 回溯。

### 当前实现状态

系统已有版本控制基础：
- 每次 sync → 新 `dataset_version`（v1, v2, v3...）
- 各版本独立 `manifest.json`（物理不覆盖）✅
- `is_current` 标记最新已提交版本 ✅
- `schema_change` 检测（none/compatible/breaking）✅
- `rollback` 回退到历史版本 ✅
- `gc` 清理旧版本（保留 N 个）✅

**缺失**：没有显式的 sync_mode 声明，当前默认行为 = SNAPSHOT。

### 设计方案

在 `data_sources.config` 的 JSON 中加入 `sync_mode` 字段：

```json
{ "sync_mode": "snapshot" }   // 或 "append" / "upsert"
```

| sync_mode | Dataset 层 | Ontology 层 | 优先级 |
|-----------|-----------|------------|--------|
| `snapshot`（默认）| 新 version 替换 current | 全量 re-promote（幂等 upsert）| ✅ 已实现 |
| `append` | 新 version = 历史 + 新增行累积 | 仅 promote 新增行 | P1 |
| `upsert` | 按主键合并到当前 version，无新 version | 按主键 upsert ontology objects | P1 |

### Schema 演进处理

| schema_change | 行为 |
|--------------|------|
| `none` | 正常提交，静默 |
| `compatible` | 新增列，提交 + 提示用户可配置新字段映射 |
| `breaking` | 列删除/类型变更，阻止自动 promote，需用户确认 |


---

## 九、Sync Mode 实现（已完成，2026-03-21）

### 实现内容

**DB 层**
- `data_sources` 表新增 `sync_mode TEXT NOT NULL DEFAULT 'snapshot'`（idempotent migration）
- `DataSourceRow` 新增 `sync_mode: String` 字段
- `create_data_source` / `update_data_source` 携带 `sync_mode`

**API 层**
- `CreateSourceReq` 新增 `sync_mode`（default: `"snapshot"`）
- `UpdateSourceReq` 新增 `sync_mode: Option<String>`

**Sync 层：`merge_records_for_mode(sync_mode, dataset_id, new_records)`**

```
snapshot → 直接返回 new_records（全量替换，默认）
append   → 读取 current version manifest + new_records 追加
upsert   → 读取 current version manifest，按 pk_col 合并（新记录覆盖同主键旧记录）
           pk_col 来自 object_type_mappings，未配置时降级为 append
```

三条 sync 路径均已接入：CSV folder mode、S3 per-file mode、single-file mode。

### 使用场景

| 场景 | sync_mode |
|------|-----------|
| 每次推送完整数据集（最常见） | `snapshot` |
| 增量导出（每次只推新增行） | `append` |
| 推送更新+新增混合批次 | `upsert` |
| 日志、事件流 | `append` |
| 状态同步（每次推当前全量状态） | `snapshot` 或 `upsert` |

