# Design Journal 精炼 — 2026-03-21

> 本文是 design-journal_2026-03-21.md 的提炼版，保留所有关键决策和思考过程，去除实施细节。

---

## 一、Sources ↔ Ontology 管道打通

**问题：** Sources 和 Ontology 是两个孤立流程，每次 sync 后需要手动去 Ontology 页重新 promote。

**决策：** 引入 `object_type_mappings` 表作为桥接。一次配置映射，后续每次 sync 自动调用 `auto_promote_if_mapped()`。

**关键设计：**
- Dataset → ET 的映射持久化到 DB，不依赖运行时状态
- promote 幂等：`UNIQUE(entity_type_id, external_id)` + ON CONFLICT DO UPDATE
- Schema-first：保存 ET = 仅写 schema；Promote = 独立步骤写实例

**参考：** ADR-35（upsert identity）、ADR-36（raw vs typed）

---

## 二、Project / Fold / DataSource 业务层级语义

**澄清：**
```
Project   = 大业务线
Fold      = 子业务线（子域）
DataSource= 具体数据源（CSV文件/S3路径/DB表/REST接口）
```

**核心洞察：** 业务部门推送 CSV 时，同一 Fold 内的多个文件是同一团队一批设计的，关联关系已知，推断置信度最高。

**Fold 范围关联推断原则：**

| 关联范围 | 置信度 | 策略 |
|---------|--------|------|
| 同 Fold，`_id`/`_fk` 匹配 | 极高 | 默认勾选 |
| 同 Fold，值域重叠 | 高 | 推荐确认 |
| 跨 Fold，同 Project | 中 | 展示不勾选 |
| 跨 Project | 低 | 不自动推断 |

---

## 三、ADR-38：Fold 是数据组织层，Ontology 是 Project 级语义层（关键）

**这是今日最重要的架构决策。**

**决策：** Fold 只约束 DataSource，Entity Type 和 Link Type 无 fold_id，天然是 Project 级全局。

**为什么重要：**
1. 跨 Fold 的业务关联直接用 Link Type，零额外配置
2. Fold 重组（业务架构调整）不破坏已有 Ontology 定义
3. 语义稳定性不依赖组织结构稳定性——企业级数据平台的核心要求

**Palantir 对应：** Foundry 的 Folder 管数据存储位置，Object Type 和 Link Type 属于全局 Ontology。

**边界：** Project 是语义边界，Fold 不是。跨 Project 关联 = 未来的 Federation 概念（P2）。

---

## 四、ADR-37：Ontology 领域语义优先，数据列结构为辅

**背景：** aircraft 和 airline 列高度重合，是否复用 Schema？

**决策：** 否。相似的列不代表相同的业务实体。

- aircraft.manufacturer = 这架飞机的制造商
- airline.manufacturer = 这家公司主营机型的制造商

语义不同，Schema 相似是行业属性的巧合。

**正确建模：** `Airline ──[OPERATES]──▶ Aircraft`，两个独立 ET + Link Type。

**映射向导影响：** 业务实体名称确认作为 Step 1，先问"这是什么业务概念"，再看列结构。

**Palantir 对应：** Foundry 没有 Object Type 继承机制，相似实体用 Link Type 关联，用 Interface 处理多态（≥3 ET 共享语义字段时触发）。

---

## 五、映射向导设计原则：自动优先，确认为辅

**核心：** 对于 CSV/JSON/SQL，80% 以上的映射可以自动完成。

| 自动完成 | 用户决策 |
|---------|---------|
| 列名 → 属性名（1:1） | 业务实体名称（"这是什么？"） |
| 类型推断 | 忽略哪些列 |
| 主键识别（id/_id 约定） | 跨 Fold 关联确认 |
| FK 推断（_id/_fk 后缀） | — |
| 同 Fold 关联（默认勾选）| — |

**UX 模式：** 默认全部选中，用户从"审查 + 排除"角度操作，不是"逐个添加"。

---

## 六、Dataset Sync Mode：同名文件多批次数据管理

**问题：** 同一数据源（如 employees.csv）第二批数据来了，和第一批是什么关系？

**Palantir 方案：** Transaction Type — SNAPSHOT / APPEND / UPDATE，写入时显式声明。

**我们的设计：** `sync_mode` 字段存入 `data_sources` 表：

| sync_mode | 语义 | 适用场景 |
|-----------|------|---------|
| `snapshot`（默认）| 全量替换 current version | 每次推完整数据集 |
| `append` | 历史行保留 + 新行追加 | 增量导出、日志、事件 |
| `upsert` | 按主键合并，新记录覆盖旧 | 状态同步、有更新的批次 |

**已实现：** DB migration、API 字段、`merge_records_for_mode()` 函数接入所有三条 sync 路径。

**Schema 演进：** compatible 变更静默接受；breaking 变更（列删除/类型变更）阻止自动 promote，需用户确认。

---

## 七、REST API / Streaming Event 的 Schema 推导（技术储备）

**REST API（P1）：**
- Preview 采样已有，Schema 发现（JSON flatten）待补
- 主键推断：id/uuid/{entity}_id，或 URL 路径参数
- 难度低，和 CSV 基本对称

**Streaming Event（P2）：**
- 核心挑战：Schema 随时间漂移 + 同一 Stream 混多种 Event 类型
- 采样窗口（最近 1000 条）+ 字段频率统计（>80% = 稳定字段）
- Event 类型识别：按 event_type 分组，每种 → 独立 ET 或合并为一个 ET + 状态字段
- 主键：event_id、(stream_id + offset)、或业务聚合键（CQRS 模式）
- 自动化程度 40%~60%，低于 CSV（80%+）

---

## 八、Bug Fixes（今日）

| Bug | 根因 | 修复 |
|-----|------|------|
| S3 多文件只生成 1 个 Dataset | S3 走 single-file 模式，所有文件写入同一 dataset_id | 新增 S3 per-file 模式，对称 CSV folder 模式 |
| Graph 只显示 employee | Graph API 用 ontology_objects（实例级）作节点 | 改为 entity_types（schema 级）作节点 |

---

## 九、整体进展（截止今日）

| 层 | 完成度 | 关键待做 |
|----|--------|---------|
| 数据模型 | 85% | `list_dataset_records` 加 `sync_run_id != 'promote'` 过滤 |
| API | 65% | Link Type 独立 CRUD 缺失 |
| UI | 40% | project_workspace.html 已写待 rebuild；Fold 分组/跨 Fold 推断未做 |

---

## 十、关联 ADR

| ADR | 主题 |
|-----|------|
| ADR-35 | Ontology Upsert Identity（external_id 去重键） |
| ADR-36 | Raw vs Typed（ontology_objects 混表问题） |
| ADR-37 | 领域语义优先（Schema 相似 ≠ 同一业务实体） |
| ADR-38 | Fold 数据层 vs Ontology 语义层解耦（关键） |

---

## 十一、ADR-39：系统是工具，不是决策者（最重要的决策）

> "我们提供的是工具"——用户在讨论 sync_mode 时的原话，提炼为核心产品定位。

**决策：** 系统推断 + 呈现 + 执行；决策权在业务人员。

**不可替代的业务判断：**
- 数据集代表什么业务实体（同样的 user_id，A 系统是客户，B 系统是员工）
- 新批次是替换还是追加（上游导出逻辑，系统看不出来）
- 相似 ET 是否是同一概念（aircraft vs airline）
- FK 推断是否真实成立（值域重叠可能是巧合）

**产品设计影响：**
- UI 用"建议"语气，不用"事实"语气
- 用户确认动作是价值，不是摩擦
- 已确认的决策自动执行，不重复询问

**Palantir 验证：** Ontology 是业务人员和数据人员协商共建的，不是 ETL 自动生成的。

**ADR 文件：** `docs/arch/adr/ADR-39-system-as-tool-not-decision-maker.md`
