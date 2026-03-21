# Palantir — 详细用户 Story

> 版本：v0.1.0 | 日期：2026-03-19
> 关联：user-roles_v0.2.0.md、cross-domain-interactions_v0.1.0.md
>
> **US 编号规则：** `US-{PERSONA前缀}-{序号}`
> - `PA` = Platform Admin
> - `DE` = Data Engineer
> - `AB` = Application Builder
> - `AN` = Analyst
> - `DS` = Data Scientist
> - `DG` = Data Governance
>
> **优先级：** P0 = MVP 必须 | P1 = 第一版迭代 | P2 = 后续规划

---

## Epic 1：Platform Admin — 平台管理

---

### US-PA-01：用户登录

> P0

**As** 平台用户（任意 Persona）
**I want** 用用户名/密码或 OAuth 凭证登录平台
**So that** 系统识别我的身份并授予对应权限范围内的访问能力

**描述：**
用户在登录页输入凭证，系统验证后颁发 Access Token（15 分钟，存内存）和 Refresh Token（7 天，HttpOnly Cookie）。首次登录时系统从 Ontology 图派生 EnrichedIdentity 并写入缓存。登录成功后根据用户持有的 Persona 跳转到对应工作台首页。

**验收标准：**
- [ ] 凭证正确 → 200，Access Token 在响应 body，Refresh Token 在 Set-Cookie
- [ ] 凭证错误 → 401，前端显示"用户名或密码错误"，不泄露具体原因
- [ ] 账号被停用 → 403，提示"账号已停用，请联系管理员"
- [ ] 登录成功后跳转到用户的默认工作台（依 Persona 判断）
- [ ] 登录页不缓存密码字段（`autocomplete="off"`）

**前端行为：**
- **入口：** `/login` — `LoginPage.tsx`
- **操作流：**
  1. 用户填写邮箱 + 密码，点击"登录"
  2. 按钮变为 Loading 状态，禁止重复提交
  3. 成功 → 跳转（Platform Admin → 管理控制台；Analyst → 分析工作台；其他 → 首页）
  4. 失败 → 表单下方红色提示文案，密码字段清空
- **OAuth 支持：** 额外展示"使用企业 SSO 登录"按钮，跳转 OAuth 授权页
- **错误状态：** 网络超时提示"网络连接异常，请稍后重试"

**触发 API：** `POST /auth/login`
**关联 Flow：** Flow 1（用户登录与身份增强）

---

### US-PA-02：Token 静默续期

> P0

**As** 已登录用户
**I want** Access Token 过期时无感知自动续期，不需要重新登录
**So that** 长时间使用不被意外踢出

**描述：**
前端 HTTP 客户端拦截 401 响应，自动携带 Refresh Token Cookie 请求续期接口，获取新 Access Token 后重放原始请求。Refresh Token 采用单次使用（Rotation），每次续期颁发新 Refresh Token，防止令牌泄露后被长期滥用。

**验收标准：**
- [ ] 用户无感知：请求 401 后自动续期，原请求透明重放
- [ ] Refresh Token 使用后立即失效，返回新 Refresh Token
- [ ] Refresh Token 过期或已吊销 → 跳转登录页，提示"会话已过期"
- [ ] 并发 401 请求只触发一次续期，其余请求排队等待

**前端行为：**
- **位置：** `api/client.ts` — Axios/ky 响应拦截器
- **用户无感知**，无 UI 变化
- 续期失败（Refresh Token 过期）→ 清除内存中的 Access Token → 跳转 `/login?reason=session_expired`，Toast 提示"会话已过期，请重新登录"

**触发 API：** `POST /auth/token/refresh`

---

### US-PA-03：用户登出

> P0

**As** 已登录用户
**I want** 点击登出后清除我的会话，并在服务端吊销 Token
**So that** 他人无法用我已登出的 Token 继续访问

**描述：**
前端清除内存中的 Access Token，后端将 Refresh Token 加入吊销黑名单（Redis，TTL 7 天）。服务端收到已吊销 Token 的请求时返回 401。

**验收标准：**
- [ ] 登出后 Access Token 从内存清除，后端加入 JTI 黑名单
- [ ] 登出后访问任意受保护页面 → 重定向到登录页
- [ ] 登出接口幂等：多次调用均返回 200

**前端行为：**
- **入口：** 顶部导航栏 → 用户头像下拉菜单 → "退出登录"
- 点击后弹出确认 Dialog："确认退出登录？"
- 确认 → 调用接口 → 清除状态 → 跳转 `/login`
- 全程 Loading 状态，防止重复点击

**触发 API：** `POST /auth/logout`

---

### US-PA-04：创建用户账号

> P0

**As** Platform Admin
**I want** 为新员工创建平台账号并分配初始 Persona
**So that** 新员工能够登录并按职责范围访问数据

**描述：**
管理员在用户管理页面填写新用户信息（姓名、邮箱、初始 Persona），系统创建 User ABox 对象并建立 HAS_ROLE 图关系。新用户首次登录时须重置密码。

**验收标准：**
- [ ] 邮箱唯一性校验，重复提示"该邮箱已注册"
- [ ] 创建成功后向新用户邮箱发送激活邮件（含临时密码）
- [ ] 创建的 User 作为 ABox 对象写入 Ontology，触发 OntologyEvent
- [ ] Platform Admin 自身不能删除自己的账号

**前端行为：**
- **入口：** 管理控制台 → 用户管理 → "新建用户"按钮
- **表单字段：** 姓名、邮箱、Persona（多选）、所属 OrgUnit
- 提交后表格列表刷新，新用户行高亮显示
- 错误（邮箱重复）→ 邮箱字段下方红色提示

**触发 API：** `POST /admin/users`

---

### US-PA-05：分配角色与组织

> P0

**As** Platform Admin
**I want** 将用户加入指定的 Persona、OrgUnit、Group，并建立 MANAGES 关系
**So that** 用户获得正确的权限边界

**描述：**
管理员通过用户详情页或批量操作，为用户分配 Persona（角色）、将其加入组织单元（OrgUnit）、设置直属上级（MANAGES 关系）。变更立即触发该用户的 EnrichedIdentity 缓存失效，下次请求重新图遍历。

**验收标准：**
- [ ] 角色变更后，该用户的 EnrichedIdentity 缓存 DEL（通过 NATS 事件驱动）
- [ ] MANAGES 关系建立后，上级用户能看到下级的 OntologyObject（ReBAC 生效）
- [ ] 批量操作支持一次更新多个用户

**前端行为：**
- **入口：** 管理控制台 → 用户管理 → 点击用户行 → 侧边抽屉
- 抽屉内有"角色"、"所属部门"、"直属上级"三个编辑区
- 每项修改有独立保存按钮，避免误操作覆盖
- 变更后显示 "已更新，权限将在下次请求时生效" 的 Toast

**触发 API：** `PUT /admin/users/{id}/roles`, `POST /v1/links`

---

### US-PA-06：查看全量审计日志

> P0

**As** Platform Admin / Data Governance
**I want** 查看平台上所有数据访问和变更操作的审计记录
**So that** 满足合规要求，并能在安全事件后追溯

**描述：**
审计日志按时间倒序展示，支持按用户、操作类型、目标对象、时间范围过滤。日志不可删除（WORM 保护），只能导出。

**验收标准：**
- [ ] 每次 OntologyObject 读写都产生一条 AuditLog
- [ ] 支持按 user_id / entity_type / operation / date_range 过滤
- [ ] 导出为 CSV（最大 10 万条）
- [ ] 审计日志本身的访问也会被记录

**前端行为：**
- **入口：** 管理控制台 → 审计日志
- 表格列：时间、操作人、操作类型（Read/Write/Delete）、目标对象、IP、决策（Allow/Deny）
- 顶部过滤栏：时间范围选择器 + 用户搜索框 + 操作类型下拉
- 点击行展开详情（读取了哪些字段、权限评估路径）
- 右上角"导出 CSV"按钮

**触发 API：** `GET /admin/audit-logs`

---

## Epic 2：Data Engineer — 数据接入

---

### US-DE-01：定义 EntityType Schema

> P0

**As** Data Engineer
**I want** 创建一个新的 EntityType，定义它的字段名称、类型和数据分类
**So that** 外部数据可以按统一的结构契约写入 Ontology

**描述：**
Data Engineer 在 Schema 管理页创建新的 EntityType（如"Employee"），定义每个字段的名称、数据类型（String / Number / Boolean / DateTime / Reference）、是否必填、字段分类（Public / Internal / Confidential / PII）。Schema 保存后版本为 1，发布 `schema_updated` 事件通知下游。

**验收标准：**
- [ ] EntityType 名称在平台内唯一（大小写不敏感）
- [ ] 每个字段必须指定 Classification（不允许默认值，强制决策）
- [ ] PII 字段自动提示"是否启用加密存储"
- [ ] Schema 创建成功后版本号为 1，发布 `schema_updated` 事件
- [ ] function-svc 和 agent-svc 收到事件后刷新 tool_schema 缓存

**前端行为：**
- **入口：** 数据工程工作台 → Schema 管理 → "新建 EntityType"
- **表单：**
  - 顶部：EntityType 名称（英文，驼峰提示）、描述
  - 字段列表：每行 = 字段名 + 类型下拉 + 必填开关 + 分类徽章（颜色区分）
  - 右侧"添加字段"按钮，支持拖拽排序
  - 底部"保存并发布"按钮
- 名称校验：实时校验唯一性（输入停顿 500ms 后）
- PII 字段：选择分类 PII 后弹出提示 "此字段含个人敏感信息，建议启用字段加密"
- 保存成功 → Toast "EntityType 已创建，版本 1"

**触发 API：** `POST /v1/schema/entity-types`
**关联 Flow：** Flow 4（TBox Schema 定义）

---

### US-DE-02：更新 EntityType Schema

> P0

**As** Data Engineer
**I want** 向已有 EntityType 新增字段或修改字段分类
**So that** Schema 能随业务需求演进，同时不破坏已有数据

**描述：**
Schema 更新时版本号递增，系统校验变更的向下兼容性（不允许删除必填字段、不允许将字段类型改为不兼容类型）。更新后发布 `schema_updated` 事件，已有 OntologyObject 的新字段默认为 null。

**验收标准：**
- [ ] 只能新增字段或修改字段的 description / classification（不能改 field_type）
- [ ] 删除字段需要"软删除"确认，已有数据不受影响
- [ ] 版本号自动递增，Schema 历史版本可查看
- [ ] 高危操作（删除字段）需要二次确认 Dialog

**前端行为：**
- **入口：** Schema 管理 → 点击 EntityType 行 → 编辑模式
- 已有字段以浅灰背景显示（保护态），新增字段以蓝色高亮
- 删除字段：行尾出现红色"删除"图标，点击弹出确认 Dialog "删除字段将在新写入数据中不再出现，历史数据保留。确认？"
- 右侧 Diff 预览：显示"变更前 vs 变更后"的字段对比
- 保存后 Toast "Schema 已更新，版本 N"

**触发 API：** `PUT /v1/schema/entity-types/{id}`

---

### US-DE-03：注册外部数据源

> P0

**As** Data Engineer
**I want** 注册一个外部数据库或 API 作为摄入来源
**So that** 外部系统的数据可以被持续同步到 Ontology

**描述：**
支持的数据源类型：MySQL / PostgreSQL / CSV（S3 路径）/ REST API / Kafka Topic。配置信息（连接串、API Key）加密存储，注册后可"测试连接"验证可达性。

**验收标准：**
- [ ] 支持 MySQL / PostgreSQL / CSV / REST API / Kafka 五种类型
- [ ] 测试连接：30 秒超时，成功显示"连接正常，发现 N 张表"，失败显示具体错误
- [ ] 连接凭证（密码/API Key）以密文存储，前端只显示 `****`
- [ ] 注册成功后数据源状态为 `Active`

**前端行为：**
- **入口：** 数据工程工作台 → 数据源管理 → "新建数据源"
- **步骤式表单（3 步）：**
  1. 选择数据源类型（图标卡片布局）
  2. 填写连接配置（不同类型动态表单：MySQL 显示 host/port/database/user/password；REST API 显示 base_url/auth_type/api_key）
  3. 测试连接 → 成功后命名并保存
- "测试连接"按钮点击后显示 Spinner，成功绿色勾，失败红色叉 + 错误信息
- 密码字段有"显示/隐藏"眼睛图标

**触发 API：** `POST /v1/sources`, `POST /v1/sources/{id}/test`

---

### US-DE-04：配置字段映射

> P0

**As** Data Engineer
**I want** 为数据源配置字段映射规则（源字段 → EntityType 字段），并可以加转换表达式
**So that** 外部原始数据能被准确规范化为标准的 OntologyObject

**描述：**
映射配置支持：直接映射（源字段名 → 目标字段名）、CEL 转换表达式（如 `value.toUpperCase()`）、静态默认值、忽略字段。配置保存后可立即预览前 5 条源数据的转换效果。

**验收标准：**
- [ ] 支持拖拽或下拉选择字段对应关系
- [ ] CEL 转换表达式有语法校验，错误实时高亮
- [ ] "预览转换"功能：拉取前 5 条真实源数据，展示转换前后对比
- [ ] 必填字段（EntityType 定义的 required=true）必须被映射，否则不能保存

**前端行为：**
- **入口：** 数据源详情页 → "配置映射" Tab
- **两列布局：**
  - 左列：源字段列表（从数据源动态拉取表结构）
  - 右列：EntityType 字段列表（从 Schema 拉取）
  - 中间：连线或下拉选择对应关系
- 点击映射行 → 展开 CEL 转换编辑器（小型 Monaco，单行）
- 底部"预览转换"按钮 → 弹出 Modal 显示前 5 条数据的源值和转换后值
- 必填字段未映射时保存按钮变灰，Tooltip 提示"以下字段未映射：..."

**触发 API：** `POST /v1/sources/{id}/mappings`, `POST /v1/sources/{id}/preview`

---

### US-DE-05：触发摄入任务

> P0

**As** Data Engineer
**I want** 手动触发一次摄入任务，并实时查看进度
**So that** 能够按需导入或刷新数据

**描述：**
触发后创建 IngestJob，后台异步执行。前端通过 SSE 或轮询获取实时进度（已处理条数 / 总条数 / 当前状态）。任务失败后保留错误信息，游标保留在失败位置，支持重试。

**验收标准：**
- [ ] 任务触发后立即返回 `job_id`，状态 `Pending → Running`
- [ ] 进度实时更新：每秒刷新 written/skipped/failed 条数
- [ ] 任务成功 → 状态变 `Success`，显示最终统计
- [ ] 任务失败 → 状态变 `Failed`，显示错误原因，"重试"按钮可从游标位置续传
- [ ] 运行中的任务有"暂停"按钮

**前端行为：**
- **入口：** 数据源详情页 → "立即摄入"按钮
- 点击后弹出确认 Dialog（显示映射规则摘要）
- 确认后 → 任务卡片出现在页面底部，展示实时进度条
  - 进度条：蓝色（运行中）/ 绿色（成功）/ 红色（失败）
  - 数字：`已写入: 12,304 / 跳过: 45 / 失败: 2`
- 失败时点击"查看错误"→ 展开错误详情（哪条数据、什么错误）
- 点击"重试"→ 从游标位置续传，不重复写入已成功数据

**触发 API：** `POST /v1/jobs`, `GET /v1/jobs/{id}` (SSE or polling)

---

### US-DE-06：配置定时增量摄入

> P1

**As** Data Engineer
**I want** 为数据源配置 Cron 表达式，让系统自动定时同步增量数据
**So that** Ontology 中的数据能够持续保持最新，无需手动干预

**描述：**
在摄入配置中选择"定时摄入"，输入 Cron 表达式（如 `0 */6 * * *` = 每 6 小时）。系统展示下次执行时间预览。触发时系统自动从上次游标位置读取增量数据。

**验收标准：**
- [ ] Cron 表达式语法校验，错误时实时提示
- [ ] 界面展示"下次执行时间：xxxx-xx-xx xx:xx"
- [ ] 定时任务产生的 Job 在历史列表中标注来源为 `Cron`
- [ ] 禁用定时任务后不再创建新 Job，但已运行的 Job 不受影响

**前端行为：**
- **入口：** 数据源详情页 → "摄入策略" Tab
- 模式切换：手动 / 定时（Radio 按钮）
- 选择"定时"后展开 Cron 配置面板：
  - Cron 输入框（如 `0 */6 * * *`）+ 右侧"下次执行时间"实时预览
  - 快捷选项：每小时 / 每天 / 每周（点击自动填入 Cron）
- 保存后顶部显示"定时摄入已启用，每 6 小时执行一次"

**触发 API：** `PUT /v1/sources/{id}/schedule`

---

### US-DE-07：查看摄入历史

> P1

**As** Data Engineer
**I want** 查看某个数据源的所有摄入任务历史
**So that** 能够排查数据延迟或错误问题

**描述：**
历史记录列表按时间倒序，显示每次任务的状态、持续时间、写入/跳过/失败条数。点击任务可展开详情，查看具体的错误日志。

**验收标准：**
- [ ] 列表按时间倒序，默认展示最近 30 次
- [ ] 每条记录显示：开始时间、持续时间、状态、统计数字
- [ ] 失败任务可展开查看具体错误（最多 100 条错误样本）
- [ ] 支持按状态过滤（全部 / 成功 / 失败 / 运行中）

**前端行为：**
- **入口：** 数据源详情页 → "运行历史" Tab
- 表格列：开始时间 | 状态（彩色徽章）| 耗时 | 写入 | 跳过 | 失败 | 操作
- 失败行：点击"查看错误"→ 侧边抽屉展开错误日志列表
- 顶部状态筛选 Tabs：全部 / 成功 / 失败 / 进行中

**触发 API：** `GET /v1/jobs?source_id={id}&limit=30`

---

## Epic 3：Application Builder — 应用构建

---

### US-AB-01：注册 Function

> P0

**As** Application Builder
**I want** 注册一个新 Function（CEL / NL），定义它的输入输出 Schema 和执行逻辑
**So that** Agent 和 Workflow 可以将它作为工具调用

**描述：**
Function 有三种运行时：CEL（表达式求值）、NL（自然语言描述转 LLM 执行）、Rust（外部编译后注册）。前端主要支持 CEL 和 NL 两种。Function 需要写清楚 `description`（LLM 用来判断是否调用）、`input_schema`（JSON Schema）、`output_schema`。

**验收标准：**
- [ ] CEL Function 保存时做语法校验，错误行高亮
- [ ] `description` 字段字数 10–200 字，不足提示
- [ ] 注册成功后发布 `functions.updated` 事件，agent-svc 刷新 tool_schema 缓存
- [ ] 同名 Function 注册时自动版本递增，历史版本可查

**前端行为：**
- **入口：** 开发者工作台 → Function 列表 → "新建 Function"
- **分区布局：**
  - 左侧：基本信息（名称、描述、运行时类型 Radio）
  - 中间：Monaco Editor（CEL 模式：语法高亮 + 自动补全 EntityType 字段；NL 模式：Prompt 文本框）
  - 右侧：Input/Output Schema 编辑器（JSON Schema 格式）
- 顶部工具栏：保存 | 测试运行 | 查看历史版本
- CEL 编辑器底部实时显示语法错误（红色波浪线 + 错误信息面板）

**触发 API：** `POST /v1/functions`
**关联 Flow：** Flow 9（Function 注册与 Agent 工具调用）

---

### US-AB-02：测试运行 Function

> P1

**As** Application Builder
**I want** 在保存前用模拟输入数据测试 Function 的输出是否符合预期
**So that** 在影响生产之前发现逻辑错误

**描述：**
在 Function 编辑页内，填入符合 `input_schema` 的 JSON 测试数据，点击"试运行"，系统在沙箱中执行 Function 并返回输出结果、耗时、日志。测试运行不产生持久化记录，不触发 OntologyEvent。

**验收标准：**
- [ ] 测试运行在 5 秒内返回结果
- [ ] 结果面板显示：输出 JSON、耗时（ms）、执行日志
- [ ] CEL 运行时错误（如类型不匹配）显示具体错误行和原因
- [ ] 测试运行不写入真实 Ontology 数据（沙箱隔离）

**前端行为：**
- **位置：** Function 编辑页右下角 "测试运行" 面板（可折叠）
- 左侧：JSON 输入编辑器（预填 schema 示例值）
- 右侧：输出面板（运行后展示结果 / 错误 / 耗时）
- 点击"运行"→ 按钮 Loading → 右侧结果实时出现
- 错误时右侧显示红色错误卡片，含行号和错误信息

**触发 API：** `POST /v1/functions/{id}/test`

---

### US-AB-03：设计 Workflow

> P0

**As** Application Builder
**I want** 可视化地创建一个 Workflow，定义步骤顺序、每步调用的 Function、失败跳转逻辑
**So that** 复杂的多步业务流程可以被自动化

**描述：**
Workflow 设计器使用 React Flow 画布，节点代表 WorkflowStep，边代表步骤间的流转（成功路径 / 失败路径）。每个节点绑定一个 Function，配置输入映射（从上下文或上一步输出取值）。支持设置 Saga 补偿 Function。

**验收标准：**
- [ ] 画布支持拖拽添加步骤节点、连线
- [ ] 每个节点可配置：关联 Function、输入映射、超时时间、补偿 Function
- [ ] 至少有一个步骤才能保存
- [ ] 保存时校验：所有边都有出口（不允许死节点）
- [ ] 支持 Ctrl+Z 撤销

**前端行为：**
- **入口：** 开发者工作台 → Workflow 设计器
- **布局：**
  - 左侧面板：步骤节点库（拖拽到画布）
  - 中间：React Flow 画布，节点 = 步骤，边 = 流转
  - 右侧：选中节点的属性配置面板（Function 选择下拉、输入映射 JSON、超时输入、补偿 Function 选择）
- 每个节点有两个输出连接点：绿色（成功）和红色（失败）
- 未连接的失败出口显示橙色警告

**触发 API：** `POST /v1/workflows`, `GET /v1/functions`（获取可用 Function 列表）

---

### US-AB-04：配置 Workflow 触发器

> P1

**As** Application Builder
**I want** 为 Workflow 配置 Cron 触发器或 OntologyEvent 触发器
**So that** 业务流程可以在正确的时机自动启动

**描述：**
Cron 触发器：输入 Cron 表达式，下次执行时间预览。
Event 触发器：选择订阅的 EntityType + Action 类型，可选填 CEL 过滤条件（如 `object.attrs.status == 'signed'`）。

**验收标准：**
- [ ] 一个 Workflow 可同时配置多个触发器（Cron + Event 混合）
- [ ] Event 触发器的 CEL 过滤有语法校验
- [ ] 展示"近期触发记录"（最近 10 次触发时间和来源）

**前端行为：**
- **入口：** Workflow 详情页 → "触发器" Tab
- 触发器列表，右上角"添加触发器"
- 触发器类型卡片：Cron（时钟图标）/ 事件（闪电图标）
- Cron：输入框 + 下次执行时间 + 快捷选项
- Event：EntityType 下拉 + Action 下拉（upsert / delete / link）+ CEL 过滤输入框
- 每个触发器有独立启用/停用开关

**触发 API：** `POST /v1/workflows/{id}/triggers`

---

### US-AB-05：查看 Workflow 执行历史

> P1

**As** Application Builder / Operator
**I want** 查看 Workflow 的执行实例列表，并能下钻到单次执行的每步详情
**So that** 能快速定位失败原因或确认流程正常执行

**描述：**
执行实例列表按时间倒序，显示状态（Running / Success / Failed / Compensating）。点击实例进入详情页，展示 DAG 视图，每个节点高亮当前状态。

**验收标准：**
- [ ] 每个步骤显示：Function 名称、开始时间、耗时、输入/输出摘要
- [ ] 失败步骤显示错误信息
- [ ] Compensating 状态时，展示 Saga 补偿进度（哪些步骤已补偿/待补偿/补偿失败）

**前端行为：**
- **入口：** Workflow 详情页 → "执行历史" Tab
- 列表：执行时间 | 触发来源（Cron / Event / Manual）| 状态徽章 | 耗时 | 操作
- 点击行 → 展开执行详情 Modal：
  - 上方：步骤 DAG（React Flow 只读，节点颜色：绿=成功，红=失败，灰=未执行，黄=补偿中）
  - 下方：步骤日志列表（选中节点高亮对应日志）
- Saga 补偿时：失败节点有橙色"补偿中"标记，补偿成功变为"已补偿"

**触发 API：** `GET /v1/workflows/{id}/executions`

---

### US-AB-06：配置外部 API 集成

> P1

**As** Application Builder
**I want** 为 Function 配置外部 HTTP API 的连接信息（URL、认证方式）
**So that** Function 可以调用外部系统（如 ERP、邮件服务）

**描述：**
OutboundConfig 绑定到一个 Function，配置 base_url（需在白名单中）、认证方式（API Key / OAuth2 / None）、超时时间、速率限制。平台管理员预先维护允许访问的域名白名单，App Builder 只能从白名单中选择。

**验收标准：**
- [ ] base_url 必须在 Platform Admin 维护的白名单域名内
- [ ] API Key 加密存储
- [ ] 有"测试请求"功能，发送一次 HEAD 或 GET 请求验证连通性
- [ ] 配置后 Function 编辑器出现 `http.call(config_id, path, body)` 内置函数提示

**前端行为：**
- **入口：** Function 详情页 → "外部集成" Tab → "绑定外部 API"
- 表单：选择白名单 URL（下拉）/ 认证方式（Radio）/ 超时（数字）/ 速率（数字/分钟）
- 保存后显示连接状态徽章，点击"测试"可即时验证

**触发 API：** `POST /v1/outbound-configs`

---

## Epic 4：Analyst — 数据分析

---

### US-AN-01：自然语言提问

> P0

**As** Analyst
**I want** 在对话框中用中文提问，Agent 自动理解意图、查询 Ontology 并给出回答
**So that** 我不需要学习查询语法就能获取数据洞察

**描述：**
用户在 Agent 对话页输入自然语言问题，Agent 经过规划（LLM 选择工具）→ 执行（调用 Function 查询 Ontology）→ 合成（LLM 生成回答）三步，回答携带数据来源引用。查询结果受当前用户权限限制，Analyst 不会看到 Confidential/PII 字段。

**验收标准：**
- [ ] 响应开始时间 ≤ 3 秒（第一个 token 到达）
- [ ] 回答以 Markdown 格式渲染（表格、列表、代码块）
- [ ] 引用来源可点击（如"查看原始对象 Employee:456"）
- [ ] 权限拦截的数据不出现在回答中，也不提示"被隐藏了"（透明过滤）

**前端行为：**
- **入口：** 分析工作台 → Agent 对话（默认首页）
- **布局：** 对话历史区（上）+ 输入区（下，固定底部）
- 输入框：多行文本，Enter 发送，Shift+Enter 换行
- 发送后：用户消息气泡出现，Agent 消息气泡显示"思考中..."动画
- 流式输出：字符逐渐出现（SSE），Markdown 实时渲染
- 右上角"停止生成"按钮：流式输出过程中可见
- 回答下方来源引用区：小型徽章显示"数据来源：Employee(12条)"

**触发 API：** `POST /v1/query`（SSE）
**关联 Flow：** Flow 5（Agent 查询），Flow 9（Function 工具调用）

---

### US-AN-02：多轮对话

> P1

**As** Analyst
**I want** 在同一个会话中继续追问，Agent 能记住上下文
**So that** 我可以逐步深入分析，而不是每次都重新描述背景

**描述：**
会话内的所有消息被发送给 LLM 作为 context。同时，Agent 会检索相关的 AgentMemory（历史相关记忆）注入上下文。用户可以引用之前回答中的对象（如"对上面的 Employee 列表按部门汇总"）。

**验收标准：**
- [ ] 同一会话内 LLM 能引用之前的问答
- [ ] 会话支持命名和收藏，历史会话列表可继续
- [ ] 单会话最多 50 轮消息（超出后提示"建议开启新会话"）

**前端行为：**
- **左侧边栏：** 历史会话列表（时间分组：今天 / 昨天 / 更早）
  - 每条会话显示标题（取第一条问题的前 20 字）
  - Hover 显示"重命名"和"删除"图标
- **当前对话：** 顶部显示会话名称，右侧有"新建会话"按钮
- 超出 50 轮：对话框底部出现黄色提示条

**触发 API：** `POST /v1/sessions`, `GET /v1/sessions/{id}/messages`

---

### US-AN-03：查看 Agent 执行轨迹

> P1

**As** Analyst / App Builder
**I want** 查看 Agent 每次回答时调用了哪些工具、查询了哪些数据、消耗了多少 Token
**So that** 能理解回答的依据，也能在结果不对时排查原因

**描述：**
每条 Agent 回答旁边有可展开的"执行轨迹"面板，显示 LLM 的推理步骤、每次工具调用的输入输出、耗时和 Token 消耗明细。

**验收标准：**
- [ ] 每条回答有"查看轨迹"入口
- [ ] 轨迹显示：工具调用链（Function 名称 + 输入摘要 + 输出摘要 + 耗时）
- [ ] 总 Token 消耗显示（Prompt + Completion）

**前端行为：**
- **位置：** 每条 Assistant 消息底部，"轨迹"折叠面板
- 展开后：Timeline 组件，每步有图标（齿轮=Function调用，数据库=Ontology查询，LLM=模型调用）
- 每步可再次展开查看完整输入/输出 JSON

**触发 API：** `GET /v1/sessions/{session_id}/traces/{message_id}`

---

### US-AN-04：Ontology 图浏览

> P1

**As** Analyst
**I want** 在可视化图界面中浏览 OntologyObject 及其关系
**So that** 能直观理解业务实体之间的连接关系

**描述：**
图画布展示 OntologyObject 节点和 OntologyRelationship 边。点击节点展开属性，双击节点加载相邻节点（N 跳展开）。节点颜色按 EntityType 区分，边标签显示关系类型。

**验收标准：**
- [ ] 按权限过滤：Analyst 看不到 Confidential/PII 字段
- [ ] 支持搜索节点（按名称或 ID 跳转）
- [ ] N 跳展开：1 跳（直接邻居）/ 2 跳，不超过 100 个节点（防止性能问题）
- [ ] 节点可固定（Pin）防止重新布局

**前端行为：**
- **入口：** 分析工作台 → Ontology 图
- 顶部搜索栏：输入 EntityType 名称 + ID 快速定位
- 右键节点菜单：查看属性 / 展开 1 跳 / 展开 2 跳 / 固定节点
- 左侧图例面板：EntityType 颜色对照
- 点击节点 → 右侧属性面板滑入（显示可见字段，Confidential/PII 字段不出现）

**触发 API：** `GET /v1/objects/{id}`, `GET /v1/objects/{id}/neighbors`

---

## Epic 5：Data Scientist — 向量与 AI

---

### US-DS-01：上传文件建立语义索引

> P1

**As** Data Scientist
**I want** 上传 PDF / Word / TXT 文件，系统自动分片向量化，建立可语义搜索的索引
**So that** Agent 可以对文件内容提问

**描述：**
文件上传后创建 Document OntologyObject，后台异步分片（每块 512 token）、调用 embedding-svc 向量化、写入 VectorStore。处理完成后状态变为 `indexed`，文件内容即可被 Agent 语义检索。

**验收标准：**
- [ ] 支持 PDF / DOCX / TXT 文件，单文件最大 50MB
- [ ] 处理状态实时展示：上传中 → 解析中 → 向量化中 → 已完成
- [ ] embedding-svc 不可用时：文件上传成功，向量化任务加入重试队列，提示"将稍后处理"
- [ ] 处理完成后 Agent 能检索到文件内容（可在对话中 @文件名）

**前端行为：**
- **入口：** AI 工作台 → 文件库 → "上传文件"
- 拖拽上传区 + 点击选择（支持多文件）
- 文件卡片显示：文件名 / 大小 / 状态（进度条动画）
- 处理完成：状态徽章变绿"已索引"，右侧显示分块数
- 失败：状态变红"处理失败"，Tooltip 显示原因，右键菜单有"重试"

**触发 API：** `POST /v1/files`（multipart）
**关联 Flow：** Flow 10（文件上传与向量化）

---

## Epic 6：Data Governance — 数据治理

---

### US-DG-01：配置 EntityType 角色权限（RBAC）

> P0

**As** Data Governance
**I want** 为每个 EntityType 指定哪些 Persona 有 Read / Write / Delete / Admin 权限
**So that** 数据访问控制在类型级别有明确边界

**描述：**
在 EntityType 的权限配置页，Data Governance 为每个 Persona 选择允许的操作（Read / Write / Delete / Admin）。保存后发布 `schema_updated` 事件，RBAC 缓存失效，新权限立即生效。

**验收标准：**
- [ ] 权限配置变更后 RBAC 缓存自动失效（通过 NATS 事件）
- [ ] 必须至少保留一个 Owner 权限的 Persona（防止锁死）
- [ ] 变更历史可查（谁在何时修改了权限配置）

**前端行为：**
- **入口：** 治理工作台 → 选择 EntityType → "权限配置" Tab
- 矩阵表格：行 = Persona，列 = Read / Write / Delete / Admin
- 每格是 Checkbox，Owner 行置灰（不可修改）
- 保存按钮 → 确认 Dialog（"本次变更将影响 N 个用户的访问权限，确认？"）

**触发 API：** `PUT /v1/schema/entity-types/{id}/permissions`
**关联 Flow：** Flow 4（TBox Schema 定义）

---

### US-DG-02：定义 ABAC 行级策略

> P1

**As** Data Governance
**I want** 为某个 EntityType 定义 CEL 表达式策略，控制哪些行对特定用户可见
**So that** 实现比角色更细粒度的数据隔离（如"只能看自己部门的数据"）

**描述：**
ABAC 策略由 CEL 表达式描述，表达式中可使用 `subject`（调用方 EnrichedIdentity）和 `object`（目标 OntologyObject 的 attrs）两个变量。表达式返回 `true` 则允许，`false` 则 Deny。

**验收标准：**
- [ ] CEL 表达式有语法校验和变量提示（subject. 和 object. 后自动补全）
- [ ] 支持"测试策略"：输入模拟 subject 和 object，显示评估结果（Allow/Deny）
- [ ] 策略有优先级（数字，越大越先评估），同优先级按创建时间

**前端行为：**
- **入口：** 治理工作台 → ABAC 策略 → "新建策略"
- 表单：策略名称 / 适用 EntityType / CEL 条件（Monaco，单行，自动补全 subject.xxx / object.attrs.xxx）/ 效果（Allow / Deny）/ 优先级
- 底部"测试策略"展开面板：左侧填 subject JSON，右侧填 object JSON，中间"评估"按钮，结果显示 Allow 绿色 / Deny 红色

**触发 API：** `POST /v1/policies/abac`

---

### US-DG-03：查看数据分类与字段可见性

> P1

**As** Data Governance
**I want** 查看当前所有 EntityType 中的字段分类分布，以及每个 Persona 对这些字段的可见性矩阵
**So that** 能一目了然地审查整个平台的字段级数据隔离状态

**描述：**
提供两种视图：① 字段分类统计：按 Classification 分组展示所有字段；② 可见性矩阵：行 = EntityType.Field，列 = Persona，单元格显示 可见 / 不见 / 脱敏。

**验收标准：**
- [ ] 可见性矩阵可按 EntityType 过滤
- [ ] PII 字段高亮展示（红色）
- [ ] 支持导出 CSV

**前端行为：**
- **入口：** 治理工作台 → 字段分类总览
- Tab 切换：分类统计 / 可见性矩阵
- 矩阵视图：冻结左侧字段列，横向滚动查看各 Persona
- 单元格颜色：绿=可见，灰=不可见，橙=脱敏

**触发 API：** `GET /v1/schema/field-visibility-matrix`

---

### US-DG-04：审计日志分析

> P0

**As** Data Governance
**I want** 对审计日志进行过滤分析，识别异常访问模式（如某用户大量访问 PII 字段）
**So that** 主动发现潜在的数据泄露风险

**描述：**
在审计日志页提供聚合统计视图：按用户/EntityType/字段的访问频率热力图，异常检测提示（如单用户 1 小时内访问 PII 字段超过 N 次自动标记）。

**验收标准：**
- [ ] 展示 TOP-10 最活跃用户（按访问次数）
- [ ] 展示 TOP-10 最多被访问的 PII 字段
- [ ] 单用户 1 小时内 PII 访问超阈值（默认 100 次）时显示告警标记
- [ ] 所有导出操作本身也写入审计日志

**前端行为：**
- **入口：** 治理工作台 → 审计分析
- 顶部：时间范围选择器（默认最近 7 天）
- 上方：3 个数字卡片（总访问次数 / Deny 次数 / PII 字段访问次数）
- 中部：按用户访问量柱状图 + 按 EntityType 分布饼图
- 底部：异常告警列表（红色，可展开查看详细记录）

**触发 API：** `GET /admin/audit-logs/analytics`

---

## 附录：US 与 Persona 索引

| US ID | 标题 | Persona | 优先级 |
|-------|------|---------|--------|
| US-PA-01 | 用户登录 | 全员 | P0 |
| US-PA-02 | Token 静默续期 | 全员 | P0 |
| US-PA-03 | 用户登出 | 全员 | P0 |
| US-PA-04 | 创建用户账号 | Platform Admin | P0 |
| US-PA-05 | 分配角色与组织 | Platform Admin | P0 |
| US-PA-06 | 查看全量审计日志 | Platform Admin / DG | P0 |
| US-DE-01 | 定义 EntityType Schema | Data Engineer | P0 |
| US-DE-02 | 更新 EntityType Schema | Data Engineer | P0 |
| US-DE-03 | 注册外部数据源 | Data Engineer | P0 |
| US-DE-04 | 配置字段映射 | Data Engineer | P0 |
| US-DE-05 | 触发摄入任务 | Data Engineer | P0 |
| US-DE-06 | 配置定时增量摄入 | Data Engineer | P1 |
| US-DE-07 | 查看摄入历史 | Data Engineer | P1 |
| US-AB-01 | 注册 Function | App Builder | P0 |
| US-AB-02 | 测试运行 Function | App Builder | P1 |
| US-AB-03 | 设计 Workflow | App Builder | P0 |
| US-AB-04 | 配置 Workflow 触发器 | App Builder | P1 |
| US-AB-05 | 查看 Workflow 执行历史 | App Builder / Operator | P1 |
| US-AB-06 | 配置外部 API 集成 | App Builder | P1 |
| US-AN-01 | 自然语言提问 | Analyst | P0 |
| US-AN-02 | 多轮对话 | Analyst | P1 |
| US-AN-03 | 查看 Agent 执行轨迹 | Analyst / App Builder | P1 |
| US-AN-04 | Ontology 图浏览 | Analyst | P1 |
| US-DS-01 | 上传文件建立语义索引 | Data Scientist | P1 |
| US-DG-01 | 配置 EntityType 角色权限 | Data Governance | P0 |
| US-DG-02 | 定义 ABAC 行级策略 | Data Governance | P1 |
| US-DG-03 | 查看字段可见性矩阵 | Data Governance | P1 |
| US-DG-04 | 审计日志分析 | Data Governance | P0 |

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本：基于 6 个 Palantir Persona 重写，28 个详细 US，含前端行为、验收标准、触发 API |
