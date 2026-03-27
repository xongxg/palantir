# Action Type 使用指南

> **文档版本**：v0.1
> **日期**：2026-03-26
> **受众**：业务分析师、运营人员、客户培训

---

## 一、什么是 Action Type？

**Action Type 是你的业务操作菜单。**

你告诉系统"Order 可以被取消"，并定义取消的规则和参数。之后不管是人工操作、API 调用、还是 AI 执行，都走这个菜单——不会绕过规则，不会忘记记录。

### 为什么需要 Action Type？

| 直接改字段 | 通过 Action Type |
|-----------|----------------|
| 任何人随时改任何值 | 只能执行预定义的业务操作 |
| 没有参数校验 | 前置条件校验（如：pending 状态才能取消） |
| 无审计记录 | 完整审计：谁、何时、改了什么、为什么 |
| AI 不知道边界 | AI Agent 操作面 = Action 列表，有界可控 |
| 业务逻辑散落各处 | 规则集中定义，一处维护 |

---

## 二、核心概念

### Action Type 只能定义在 AR（聚合根）上

AR（Aggregate Root）是业务操作的唯一入口。所有对业务实体的写操作，必须通过 AR 上定义的 Action 发起，不允许直接修改聚合内部的子实体。

```
◆ Order（AR）        ← Action 定义在这里
  ├── OrderItem     ← 不能直接操作，通过 Order Action 间接修改
  └── Address       ← 同上
```

### 三种实现方式

```
Config Mode（no-code）  — 业务人员，可视化规则配置
AI Mode                 — 描述需求，AI 生成实现，人工审核后激活
Code Mode               — 开发者，直接编写代码，完全控制
```

---

## 三、完整案例：电商订单取消

> 场景：某电商公司使用本系统管理订单数据，运营团队需要处理客户取消申请。

### Step 1：业务分析师定义 Action（Model 页）

打开 **Model → Order AR 详情 → Actions 面板 → + 新建**

填写配置：

```
显示名：  取消订单
描述：    将订单状态改为已取消，并可选通知客户

参数：
  ├── 取消原因    （文本，必填）
  └── 通知客户    （开关，默认：开）

实现方式：Config Mode

规则：
  1. 前置校验：status 必须是 'pending' 或 'confirmed'
               否则报错"只有待处理/已确认的订单可以取消"
  2. set_field: status       → 'cancelled'
  3. set_field: cancel_reason → params.取消原因
  4. if 通知客户 == 开 → emit_event: order.cancelled
```

保存后点「激活」，Action 进入可用状态。

---

### Step 2：客服人员执行操作（浏览页）

打开 **浏览 → Order → 选中 O003（王芳，status: pending）**

点击右上角 **「▶ Actions」→ 取消订单**，弹出执行面板：

```
┌─────────────────────────────────────────────┐
│  ◈ 取消订单                                  │
│  Order / O003 · 王芳                         │
│  ─────────────────────────────────────────  │
│                                             │
│  取消原因 *                                  │
│  ┌─────────────────────────────────────┐    │
│  │ 客户联系要求取消，已确认              │    │
│  └─────────────────────────────────────┘    │
│                                             │
│  通知客户   ●── 开                           │
│                                             │
│  ⚠ 执行后 status 将变更为 cancelled         │
│              [取消]        [▶ 执行]          │
└─────────────────────────────────────────────┘
```

点「▶ 执行」后：

- O003 的 `status` → `cancelled`
- O003 的 `cancel_reason` → `"客户联系要求取消，已确认"`
- 触发 `order.cancelled` 事件（可接 webhook / 邮件通知）
- 审计日志自动记录

---

### Step 3：管理层查看审计记录

Order AR 详情页底部「执行记录」面板：

```
时间               操作人   Action    对象    结果
2026-03-26 09:31   小李     取消订单  O003    ✓ 成功
                   参数: reason="客户联系要求取消，已确认", notify=true
```

---

## 四、完整使用路径

```
业务分析师定义规则（Model 页）
    ↓
客服 / 运营执行操作（浏览页）
    ↓
系统自动校验 + 执行 + 记录
    ↓
管理层 / 技术层审计（执行日志）
    ↓
AI Agent 也走同一套入口（操作面可自动枚举）
```

---

## 五、与 Palantir Foundry 的差异

| | Palantir Foundry | 本系统 |
|--|-----------------|--------|
| Action 绑定对象 | 任意 Object Type（扁平） | **只允许绑定 AR**，结构约束 |
| 实现方式 | 只有 TypeScript/Python 代码 | **Config / AI / Code 三层**，覆盖所有角色 |
| 业务边界 | 靠组织约定 | **AR = 一致性边界**，结构强制 |
| AI 操作面 | 需手动维护 | **= 所有 AR 的 Action 集合**，自动可枚举 |
| 业务人员参与 | 需工程师中转 | **业务分析师直接定义规则**，no-code |

---

## 六、Function 三层实现模型

Action Type 声明了"能做什么"，Function 决定"怎么做"。三种实现方式面向不同角色：

```
┌─────────────────────────────────────────────────┐
│  Layer 3: Code Mode（开发者）                    │
│  直接写 TypeScript / Python，完全控制             │
│  适用：复杂业务逻辑、外部 API 调用、高性能要求    │
├─────────────────────────────────────────────────┤
│  Layer 2: AI Mode（业务 + 技术）                 │
│  输入：自然语言描述需求                          │
│  输出：AI 生成规则代码，人工审核后激活            │
│  适用：中等复杂度，描述得出但不想手写规则         │
├─────────────────────────────────────────────────┤
│  Layer 1: Config Mode（业务人员，no-code）       │
│  可视化配置：前置条件 + 字段操作 + 事件触发       │
│  适用：标准业务操作，大多数场景的首选             │
└─────────────────────────────────────────────────┘
```

**主体以 Config + AI 结合为主，Code 作为专业兜底。**

---

## 七、系统集成分期（打通客户业务系统）

Action Type 是打通 Ontology 与客户业务系统的关键桥梁：

```
Phase 2（当前）：操作声明层
  定义 Action Type，业务操作有了清单和规则

Phase 3：执行层（系统内闭环）
  Config / AI / Code 执行引擎运行规则
  → Ontology 内部数据变更 + 完整审计日志

Phase 4：集成层（打通外部）
  emit_event → Webhook / 消息队列
  → 客户 ERP / OMS / CRM 同步更新
  → 真正实现 Ontology 驱动业务系统

Phase 5：操作系统（终态）
  Ontology 成为业务操作唯一入口
  外部系统成为数据源和执行节点
```

---

## 八、ActionType 层级模型

> 核心原则：**用 Fold / Project 边界划分操作复杂度，Domain Service 覆盖域内所有协作**

### 四个层级

| 层级 | 范围 | 编排方式 | 定义者 |
|------|------|---------|--------|
| **对象级** | 单个 AR 内部 | AR 自治 | 业务分析师 |
| **BC 级** | 同一 BC 内跨 AR | Domain Service | 业务分析师 + 技术 |
| **Fold/Project 级** | 同一 Project 内跨 BC | Domain Service | 业务分析师 + 技术 |
| **应用级** | 跨 Fold / 跨 Project | Saga / Process Manager | 业务人员定义，技术人员实施 |

### Domain Service 的覆盖范围

**关键点**：Domain Service 不只限于单个 BC 内，只要在**同一 Project 内**，不管跨几个 BC，Domain Service 都可以编排。

```
同一 Project 内：
  ┌─────────────────────────────────────────┐
  │  Fold: 电商域                            │
  │  ┌──────────┐    ┌──────────┐           │
  │  │ Order BC │    │ Payment BC│           │
  │  │          │◄───│          │           │
  │  │ Order AR │    │Invoice AR │           │
  │  └──────────┘    └──────────┘           │
  │         ↑ Domain Service 可以跨 BC 编排  │
  └─────────────────────────────────────────┘
```

Domain Service 知道同一 Project 下所有 BC 的 AR，可以跨 BC 调用多个 AR 的 ActionType 完成一个完整的业务操作——这仍然是**域内行为**，不需要 Saga。

### AR 自治的本质

AR 内部的 Action 不需要外部协调，因为 AR 自己掌握：

- 自己的**状态**（`Order.status`）
- 自己的**聚合成员**（OrderItem、Address）
- 自己的**不变量**（总金额必须 > 0）

**`CancelOrder` 不需要外部告诉它怎么找 OrderItem，它自己管着。**

### 应用级 = Saga 编排

**真正需要 Saga 的边界是跨 Fold 或跨 Project**——这才是真正的"域外"协作，需要异步、补偿、幂等保障。

```
PlaceOrder（跨 Fold 的应用级 Use Case）
    ↓ Saga 编排
  ├── Step 1: 电商域 / Customer BC  → 校验客户信用额度
  ├── Step 2: 仓储域 / Inventory BC → 锁定库存          ← 跨 Fold！
  ├── Step 3: 电商域 / Order BC     → 创建 Order AR
  └── Step 4: 财务域 / Payment BC   → 生成 Invoice      ← 跨 Fold！

补偿链（任一步失败）:
  ← Step 3 失败 → 释放库存（补偿 Step 2）
  ← Step 2 失败 → 结束（Step 1 无副作用）
```

**Saga 的核心价值**：幂等性保障——每一步可重试，失败可补偿，不产生部分修改的脏数据。

### 声明层当前范围

**P2 实现对象级 + BC 级 + Fold/Project 级**（均由 Domain Service 编排）。
跨 Fold/Project 的 Saga 编排留到执行引擎（Phase 3）——没有执行引擎支撑，声明了也没意义。

---

## 九、ActionType = 状态机的 Transition

**Action 的本质是 AR 状态机上的边（transition）**，不是孤立的操作。

### 电商订单状态机示例

```
         PlaceOrder
  draft ──────────→ pending
                       │
          ConfirmOrder  │  CancelOrder
                       ↓         ↓
                  confirmed   cancelled
                       │
            ShipOrder  │
                       ↓
                   shipped
                       │
         CompleteOrder │
                       ↓
                  completed
```

每个 ActionType 声明对应状态机上的一条边：

```yaml
ActionType: CancelOrder
target_ar:  Order
from_states: [pending, confirmed]   # precondition = 合法的 from-state
to_state:    cancelled              # effect = 执行后的 to-state
params:
  - name: cancel_reason
    type: string
    required: true
trigger: manual                     # 手动 / event / cron
allowed_personas: [客服, 运营]
```

**好处**：所有 ActionType 合在一起就是完整的状态图，业务人员一眼能理解，可以直接可视化。

### 关键原则：客户定义状态，我们只关注转换

客户的业务系统定义了自己的状态值（如 `pending / confirmed / cancelled`），**本系统不干预状态语义**，只需要：

1. 知道当前状态是否允许某个 transition（precondition）
2. 知道执行后状态变成什么（effect）
3. 知道 transition 时触发什么 Action

这让系统对任何行业的状态机都适用，无需硬编码业务语义。

---

## 十、触发方式

ActionType 支持三种触发来源：

| 触发方式 | 描述 | 典型场景 |
|---------|------|---------|
| **手动触发** | 用户在 Browse 页对 AR 对象点击执行 | 客服取消订单 |
| **事件触发** | 另一个 AR 状态变化时自动触发 | Order confirmed → 自动生成 Invoice |
| **定时触发** | Cron 表达式周期执行 | 每天凌晨对超时订单执行 AutoCancel |

**事件触发是 BC 级协作的核心粘合剂**——不需要 Saga，但能实现跨 AR 的自动联动。

---

## 十一、Persona 权限绑定

不同角色只能执行对应的 Action，在声明层就锁定：

```yaml
CancelOrder:
  allowed_personas: [客服, 运营]   # 仓库管理员不能取消订单

ShipOrder:
  allowed_personas: [仓库管理员]

RefundOrder:
  allowed_personas: [财务, 运营主管]
```

权限在声明时定义，执行时自动校验，无需额外的权限代码。

---

## 十二、行业参考模板

> 不同行业的状态机不同，但 ActionType 的声明结构完全一致。
> 客户导入自己的数据后，可以参考对应行业模板快速配置。

### 电商 — 订单状态机

| ActionType | from | to | 触发方式 | 执行角色 |
|-----------|------|----|---------|---------|
| PlaceOrder | draft | pending | 手动 | 买家 |
| ConfirmOrder | pending | confirmed | 手动/事件 | 运营 |
| ShipOrder | confirmed | shipped | 手动 | 仓库 |
| CompleteOrder | shipped | completed | 事件（签收确认） | 系统 |
| CancelOrder | pending/confirmed | cancelled | 手动 | 客服/运营 |
| ReturnOrder | completed | returning | 手动 | 客服 |

### 制造 — 工单状态机

| ActionType | from | to | 触发方式 | 执行角色 |
|-----------|------|----|---------|---------|
| CreateWorkOrder | - | draft | 手动 | 计划员 |
| ReleaseWorkOrder | draft | released | 手动 | 生产主管 |
| StartProduction | released | in_progress | 手动 | 班组长 |
| PauseProduction | in_progress | paused | 手动/事件 | 班组长 |
| CompleteProduction | in_progress | completed | 手动 | 质检员 |
| ScrapWorkOrder | any | scrapped | 手动 | 生产主管 |

### 金融 — 贷款审批状态机

| ActionType | from | to | 触发方式 | 执行角色 |
|-----------|------|----|---------|---------|
| SubmitApplication | draft | submitted | 手动 | 客户经理 |
| InitialReview | submitted | under_review | 事件 | 系统 |
| RequestDocuments | under_review | pending_docs | 手动 | 审核员 |
| ApproveApplication | under_review | approved | 手动 | 审批主管 |
| RejectApplication | under_review | rejected | 手动 | 审批主管 |
| Disburse | approved | disbursed | 手动 | 放款员 |

---

## 十三、常见问题

**Q：为什么 Entity 上没有 Actions？**
A：Entity 没有独立的一致性边界。对 Entity 的修改必须通过它所属的 AR 来协调，这样才能保证数据一致性和审计完整性。

**Q：Config Mode 能做多复杂的规则？**
A：覆盖大多数业务场景：字段赋值、条件判断、参数校验、事件触发。超出范围时升级到 AI Mode 或 Code Mode。

**Q：AI Mode 生成的代码安全吗？**
A：AI 生成的实现需经人工审核确认后才能激活（status: draft → active），不会直接上线执行。

**Q：Action 执行失败怎么办？**
A：执行失败会回滚，不会产生部分修改。失败记录写入审计日志，包含错误原因。
