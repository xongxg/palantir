# Palantir — 平台管理与部署 User Stories

> 版本：v0.1.0 | 日期：2026-03-19
> 关联：user-roles_v0.2.0.md、ADR-33（模块化部署）
> 补充：user-stories-detailed_v0.1.0.md（业务功能 28 个 US）
>
> **本文覆盖三类场景，原业务 US 不重复：**
> 1. 平台运营管理（模块 / Edition / License）
> 2. SaaS 多租户管理
> 3. 私有化 & 混合部署配置
> 4. Enterprise+ 私有定制

---

## Epic P：Platform Admin — 平台运营管理

---

### US-PA-07：查看平台模块状态

> P0 | 适用拓扑：全部

**As** Platform Admin
**I want** 在控制台看到所有服务模块的当前运行状态（已启用 / 已禁用 / 降级）
**So that** 能随时了解平台能力边界，快速定位"某功能为何不可用"

**描述：**
模块状态页汇总当前 `ModuleProfile` 的配置结果，包括每个模块的启用状态、健康状态（实例数 / 响应时间）、对前端能力的影响。当某模块异常时，高亮展示受影响的功能和受影响的 Persona。

**验收标准：**
- [ ] 展示全部 8 个服务（Core 3 个 + Optional 5 个）的状态
- [ ] 每个服务卡片显示：启用/禁用、健康实例数、P99 响应时间
- [ ] 软依赖降级（如 embedding-svc 不可用）标注"Agent 语义缓存已禁用"
- [ ] 点击服务卡片可查看最近 10 条健康检查日志

**前端行为：**
- **入口：** 管理控制台 → 系统状态 → 模块总览
- **布局：** 卡片网格（3 列）
  - 绿色卡片：运行正常；黄色：降级；红色：不可用；灰色：已禁用
  - 卡片内：服务名 + 实例数 + P99 + 上次健康检查时间
- 页面顶部：当前 Edition 徽章（如 `Enterprise`）
- 降级状态下页面顶部出现黄色 Banner："embedding-svc 不可用，Agent 语义缓存已关闭"
- 点击卡片 → 侧边抽屉展开：实例列表、健康日志、依赖关系图

**触发 API：** `GET /admin/modules/status`

---

### US-PA-08：启用或禁用功能模块

> P1 | 适用拓扑：私有化、混合

**As** Platform Admin（私有化部署）
**I want** 在控制台界面启用或禁用可选服务模块，不需要手动编辑 `deployment.toml`
**So that** 可以按业务需求灵活调整平台能力，升级 Edition 时可视化操作

**描述：**
开关操作修改后端 `ModuleProfile`，触发 api-gateway 动态路由更新（无需重启 gateway）。对于有硬依赖的模块（如禁用 function-svc 时 agent-svc 仍在运行），系统阻止操作并展示依赖原因。启用新模块时，系统自动拉取对应服务镜像并启动（K8s 模式）。

**验收标准：**
- [ ] 禁用操作：检测硬依赖违反 → 阻止 + 展示受影响模块
- [ ] 启用操作：校验 License 中 Edition 是否包含该模块
- [ ] 操作后 api-gateway 路由在 30 秒内更新
- [ ] 操作记录写入审计日志（谁在何时启用/禁用了哪个模块）
- [ ] 禁用前确认 Dialog：列出"以下功能将不可用：..."

**前端行为：**
- **入口：** 管理控制台 → 系统状态 → 模块总览 → 卡片右上角开关
- 点击开关 → 弹出确认 Dialog
  - 禁用时：列出受影响功能 + 受影响 Persona
  - 启用时：显示该模块的资源需求（CPU/内存）
- 确认后：卡片显示过渡态（Spinner），完成后状态更新
- 依赖冲突：开关变红 + Tooltip "需要先禁用：agent-svc"

**触发 API：** `PUT /admin/modules/{name}/enabled`

---

### US-PA-09：管理产品版本（Edition）

> P1 | 适用拓扑：全部

**As** Platform Admin
**I want** 查看当前激活的 Edition，并在获得授权后升级到更高 Edition
**So that** 按需解锁新模块，同时清楚了解当前版本的能力边界

**描述：**
Edition 与 License 文件绑定（私有化：本地 License 文件；SaaS：在线激活码）。升级 Edition 时系统展示"新增功能差异"，确认后更新 License 并自动启用新模块。降级需要联系 Palantir 支持。

**验收标准：**
- [ ] 当前 Edition 名称、激活时间、过期时间清晰展示
- [ ] "新增功能"差异对比（当前 Edition vs 目标 Edition 的功能矩阵对比）
- [ ] 私有化：License 文件本地验证，无需联网
- [ ] SaaS：在线激活码验证
- [ ] 升级成功后自动启用新模块（按 Edition 预设的 ModuleProfile）

**前端行为：**
- **入口：** 管理控制台 → 许可证管理
- 顶部：当前版本卡片（Edition 名称 + 到期日 + 已用功能摘要）
- 下方：Edition 升级路径（横向卡片列，当前版本高亮）
- 点击目标版本 → 侧边抽屉展开功能对比列表
- "输入激活码"输入框 → 验证 → 确认升级
- 私有化：支持"上传 License 文件"替代激活码

**触发 API：** `GET /admin/license`, `POST /admin/license/activate`

---

## Epic S：SaaS 多租户管理

> 适用拓扑：SaaS（ADR-33 拓扑 A）
> 操作者：Palantir 平台超级运营人员（Super Operator，独立于普通 Platform Admin）

---

### US-SA-01：新租户自助注册（SaaS）

> P1 | 适用拓扑：SaaS

**As** 新客户（企业管理员）
**I want** 在 Palantir SaaS 注册页填写企业信息，自动开通一个新租户并进入平台
**So that** 无需等待人工审批即可立即开始试用

**描述：**
注册流程：① 填写企业名称、管理员邮箱；② 选择初始 Edition（默认 Lite，可选 Standard 试用 14 天）；③ 邮件确认；④ 自动创建租户空间，分配 `tenant_id`；⑤ 进入平台引导流程（Onboarding Wizard）。

**验收标准：**
- [ ] 注册完成到能登录平台 ≤ 2 分钟
- [ ] 初始 Edition 为 Lite，可申请 14 天 Standard 试用
- [ ] 自动创建 1 个 Platform Admin 账号（注册邮箱）
- [ ] 发送欢迎邮件（含登录链接、快速入门文档链接）
- [ ] 同一邮箱域名不重复注册（提示"您的企业已注册，请联系管理员"）

**前端行为：**
- **入口：** `palantir.io/signup` — 独立注册页
- **步骤向导（3 步）：**
  1. 企业信息：企业名 + 管理员邮箱 + 密码
  2. 选择 Edition：Lite（免费）/ Standard 试用（14天）+ 功能对比卡片
  3. 邮件确认：展示"我们已发送确认邮件，请查收"
- 完成后自动跳转平台首页，触发 Onboarding Wizard（首次引导）

**触发 API：** `POST /saas/tenants/register`

---

### US-SA-02：Onboarding 引导向导

> P1 | 适用拓扑：SaaS

**As** 新注册的平台管理员（SaaS）
**I want** 首次登录时有一个交互式引导流程，帮我完成最小可用配置
**So that** 不需要阅读文档就能快速感受核心价值

**描述：**
Onboarding Wizard 是一个覆盖在界面上的步骤向导，引导用户：① 创建第一个 EntityType；② 导入示例数据（或上传 CSV）；③ 提一个 Agent 问题。完成后显示"平台已就绪"。可随时跳过，从任务中心再次进入。

**验收标准：**
- [ ] 引导流程最多 5 步，每步有进度提示
- [ ] 每步完成后有动画反馈（✅ 完成）
- [ ] 支持随时"跳过引导"
- [ ] 引导完成后在首页展示"您的平台已就绪"概览卡

**前端行为：**
- **触发：** 首次登录后自动弹出，覆盖式向导（遮罩 + 聚焦高亮）
- 步骤 1：欢迎页（展示 Edition 能力概览）→ 下一步
- 步骤 2：快速创建 EntityType（预填示例"Employee"，可直接下一步）
- 步骤 3：上传 CSV 或使用示例数据（5 条示例员工数据）
- 步骤 4：向 Agent 提一个问题（预填"显示所有员工"）
- 步骤 5：完成 🎉，展示平台就绪状态卡片
- 右上角随时有"跳过向导"按钮

**触发 API：** 综合调用各业务接口

---

### US-SA-03：租户用量监控（SaaS 运营）

> P1 | 适用拓扑：SaaS

**As** Palantir SaaS 超级运营人员
**I want** 查看每个租户的用量指标（API 调用 / 存储 / LLM Token / 活跃用户）
**So that** 能识别超出配额的租户并触发升级提醒或限流

**描述：**
运营后台的租户用量面板，按租户列表展示实时 + 历史用量，支持按 Edition 过滤。超出配额阈值的租户自动标红。运营人员可手动调整配额上限或发起升级邀约。

**验收标准：**
- [ ] 展示每租户的月度 API 调用量、存储量（GB）、LLM Token 消耗、活跃用户数
- [ ] 超出配额 80% 自动标黄，超出 100% 标红
- [ ] 支持按 Edition 筛选租户列表
- [ ] 点击租户可下钻到用量详情（按服务拆分）

**前端行为：**
- **入口：** SaaS 运营控制台（独立域名，与租户平台隔离）→ 租户管理
- 列表视图：租户名 | Edition | 活跃用户 | API 调用（本月）| 存储 | Token | 状态
- 超配额行标红，"发送升级邀约"快捷操作按钮
- 顶部过滤栏：Edition 下拉 + 状态下拉（正常/警告/超配）
- 点击租户行 → 用量详情页（折线图：近 30 天每日用量趋势）

**触发 API：** `GET /ops/tenants`, `GET /ops/tenants/{id}/usage`

---

### US-SA-04：租户 Edition 升级（SaaS 运营触发）

> P1 | 适用拓扑：SaaS

**As** Palantir SaaS 超级运营人员
**I want** 手动为某租户升级 Edition（如从 Standard → Professional）
**So that** 满足该租户已购买的新套餐，立即为其解锁对应模块

**描述：**
运营人员在租户详情页操作 Edition 升级，选择目标 Edition 后系统更新租户的 License，对应模块在 30 秒内对该租户生效。该租户的 Platform Admin 会收到通知邮件。

**验收标准：**
- [ ] Edition 只能向上升级，不支持降级（降级需走退款流程）
- [ ] 升级后该租户立即可以使用新 Edition 包含的模块
- [ ] 操作记录写入运营审计日志
- [ ] 触发邮件通知："{租户名} 已升级至 Professional Edition"

**前端行为：**
- **入口：** 运营控制台 → 租户详情 → "升级 Edition" 按钮
- 下拉选择目标 Edition + 备注（如"2026-03 合同升级"）
- 确认 Dialog：显示新增功能列表 + "确认升级"
- 操作成功后 Toast + 租户列表中 Edition 徽章实时更新

**触发 API：** `PUT /ops/tenants/{id}/edition`

---

## Epic D：私有化部署管理

> 适用拓扑：私有化（ADR-33 拓扑 B）

---

### US-PD-01：可视化部署配置管理

> P1 | 适用拓扑：私有化

**As** Platform Admin（私有化）
**I want** 在界面上查看和修改部署配置（ModuleProfile + InfrastructureProfile），不需要 SSH 到服务器改 TOML
**So that** 运维操作更安全、有记录、不容易出错

**描述：**
部署配置页展示当前 `deployment.toml` 的可视化表单版本，分为：① 基础设施选择（各 Trait 的实现选择下拉）；② 模块启停（与 US-PA-08 联动）；③ 服务参数（各服务的副本数、资源限制）。修改后生成新的 `deployment.toml` 内容，支持"预览变更"和"应用变更"两步操作。

**验收标准：**
- [ ] 展示当前所有基础设施选项的当前值（如 `StructuredStore = TiDB`）
- [ ] 修改后有 diff 预览（高亮变更项）
- [ ] 应用变更前要求二次确认（输入 "CONFIRM" 字符串）
- [ ] 变更历史可查（谁在何时改了什么）
- [ ] 不允许将数据库从 TiDB 切换为 MySQL（有数据，需要迁移评估，展示警告）

**前端行为：**
- **入口：** 管理控制台 → 部署配置
- **分 Tab 布局：**
  - Tab 1：基础设施（每个 Trait 一行：图标 + 当前值 + 下拉选择）
  - Tab 2：模块开关（同 US-PA-08，卡片开关）
  - Tab 3：服务参数（副本数、CPU/内存 Slider）
- 底部固定：Cancel | 预览变更 | 应用变更
- "预览变更"→ Modal 显示 TOML diff（红删 / 绿增）
- "应用变更"→ 输入确认框 → 提交 → 进度条展示各服务滚动重启状态

**触发 API：** `GET /admin/deployment/config`, `PUT /admin/deployment/config`

---

### US-PD-02：License 文件管理（私有化）

> P0 | 适用拓扑：私有化

**As** Platform Admin（私有化）
**I want** 上传新的 License 文件以激活或续期平台使用权
**So that** 平台能在合同期内正常运行，过期前有预警

**描述：**
License 文件为加密的二进制文件，包含：Edition、有效期、最大用户数、允许模块列表。系统在启动时和每日定时校验 License。距离过期 30 天 / 7 天 / 1 天时，系统分别发出不同级别的告警。

**验收标准：**
- [ ] License 文件验证：签名校验 + 有效期校验 + 机器指纹绑定（可选）
- [ ] 过期前 30 天：系统顶部出现黄色 Banner 倒计时
- [ ] 过期前 7 天：Banner 变红，发送邮件通知所有 Platform Admin
- [ ] License 过期：平台进入只读模式（不允许写操作，仅允许查询和导出）
- [ ] 新 License 上传后立即生效，无需重启

**前端行为：**
- **入口：** 管理控制台 → 许可证管理
- 当前 License 信息卡：Edition + 有效期（倒计时）+ 最大用户数（已用/上限）+ 允许模块
- "上传新 License"区域：拖拽上传 `.lic` 文件 → 自动验证 → 显示新 License 信息预览 → "确认激活"
- 验证失败：红色提示（License 已损坏 / 已过期 / 机器指纹不匹配）
- 有效期 ≤ 30 天：顶部 Banner "License 将于 N 天后过期，请联系 Palantir 续期"

**触发 API：** `GET /admin/license`, `POST /admin/license/upload`

---

## Epic H：混合部署管理

> 适用拓扑：混合（ADR-33 拓扑 C）

---

### US-HY-01：配置云端 AI 服务连接

> P1 | 适用拓扑：混合

**As** Platform Admin（混合部署）
**I want** 配置本地 ingest/agent 服务连接到 Palantir Cloud 的 embedding-svc 的地址和认证信息
**So that** AI 能力走云端，业务数据留在本地，数据边界清晰可见

**描述：**
混合模式下，`embedding-svc` 和 LLM API 运行在云端（或 Palantir 托管端）。Platform Admin 配置远端连接（地址 + mTLS 证书），系统定期检测连通性和延迟。配置页清楚展示"哪些数据会发到云端"（只有文本片段和向量，不含业务数据）。

**验收标准：**
- [ ] 支持配置 embedding-svc 远端地址 + mTLS 客户端证书
- [ ] 支持配置 LLM API（Base URL + API Key）
- [ ] "连通性检测"：发送测试请求，显示延迟和状态
- [ ] 页面有数据流向说明："发往云端的数据仅包含：文本片段（无 ID/关联信息）"
- [ ] 连接失败时 agent-svc 自动降级（跳过语义缓存），页面显示降级状态

**前端行为：**
- **入口：** 管理控制台 → 混合连接配置
- **分区布局：**
  - 区块 1：Embedding 服务（地址输入 + 上传 mTLS 证书 + 测试按钮 + 状态徽章）
  - 区块 2：LLM API（Base URL + API Key + 模型名称 + 测试按钮）
  - 区块 3：数据流向说明（只读，列出"会离开本地的数据类型"）
- "测试连接"按钮 → Spinner → 显示 延迟(ms) + 绿勾/红叉
- 连接不可用时：区块顶部红色 Banner + "AI 功能当前降级运行"

**触发 API：** `PUT /admin/hybrid/connections`, `POST /admin/hybrid/connections/test`

---

### US-HY-02：查看数据边界监控

> P1 | 适用拓扑：混合

**As** Platform Admin（混合部署）
**I want** 实时查看过去 24 小时内有多少请求发往云端、传输了什么类型的数据
**So that** 向合规审计人员证明业务数据从未离开本地机房

**描述：**
数据边界监控面板展示：出站请求次数（按服务分类）、出站数据类型（文本片段 / 向量）、字节量，以及一条关键保证："OntologyObject 数据：0 字节外发"。

**验收标准：**
- [ ] 展示过去 24 小时 / 7 天 / 30 天出站统计
- [ ] 分类展示：embedding-svc 出站（文本 + 向量）/ LLM API 出站（脱敏摘要）
- [ ] "OntologyObject 原始数据外发量：0 字节"这条指标单独高亮展示
- [ ] 支持导出为合规报告（PDF）

**前端行为：**
- **入口：** 管理控制台 → 数据边界监控
- 顶部：绿色大型数字卡 "原始业务数据外发：0 字节 ✅"
- 下方：出站流量统计图（折线图 + 饼图）
- 出站类型说明表：类型 | 用途 | 是否含业务数据 | 近 24h 量
- 右上角"生成合规报告"→ PDF 下载

**触发 API：** `GET /admin/hybrid/data-boundary-stats`

---

## Epic E：Enterprise+ 私有定制

---

### US-EP-01：注册自定义扩展模块

> P2 | 适用拓扑：私有化 / 混合 | Edition：Enterprise+

**As** Application Builder（Enterprise+）
**I want** 将我们自研的微服务注册为 Palantir 平台的扩展模块，使其能通过 api-gateway 路由并使用平台的 JWT 认证
**So that** 自研系统与 Palantir 平台无缝集成，统一入口、统一权限

**描述：**
扩展模块注册后，api-gateway 自动识别并反向代理到对应服务（通过服务注册中心发现）。平台 JWT 透传，自研服务可用 SDK 验证。扩展模块可选择订阅 OntologyEvent，实现与核心模块的事件联动。

**验收标准：**
- [ ] 填写服务名称（须在服务注册中心已注册）、路由前缀、鉴权模式
- [ ] 路由前缀不能与内置路由冲突（`/v1/objects`、`/v1/query` 等）
- [ ] 注册成功后 api-gateway 路由在 60 秒内生效
- [ ] 可选：配置该扩展模块订阅的 OntologyEvent 主题列表

**前端行为：**
- **入口：** 管理控制台 → 扩展模块 → "注册扩展模块"
- **表单：**
  - 模块名称 / 描述
  - 服务注册名（下拉，从服务注册中心拉取已注册服务列表）
  - 路由前缀（输入框 + 实时冲突检测）
  - 鉴权模式（JWT 透传 / 不鉴权 / 自定义）
  - 事件订阅（多选 EntityType + Action 类型）
- 保存后扩展模块出现在模块总览页（灰色"扩展"标记区分）

**触发 API：** `POST /admin/extensions`

---

### US-EP-02：配置私有 LLM 模型

> P1 | 适用拓扑：私有化 / 混合 | Edition：Professional+

**As** Platform Admin（私有化 / 混合）
**I want** 将 Agent 使用的 LLM 替换为我们内部部署的大模型（如 ChatGLM / Qwen / DeepSeek）
**So that** 所有 AI 推理在本地完成，不需要外发任何数据到公有云

**描述：**
LLM 配置支持任何兼容 OpenAI API 格式的模型服务（大多数开源模型都有兼容层）。配置项包括：base_url、api_key、model_name、上下文长度限制、Temperature 默认值。配置后发送一条测试消息验证连通性和响应格式。

**验收标准：**
- [ ] base_url 支持 HTTP（内网）和 HTTPS
- [ ] "测试连接"发送一条"Hello"消息，验证响应格式兼容 OpenAI
- [ ] model_name 自由输入（不做格式校验，以实际模型服务为准）
- [ ] 保存后 agent-svc 立即使用新配置（无需重启）
- [ ] 配置变更写入审计日志

**前端行为：**
- **入口：** 管理控制台 → AI 配置 → LLM 提供商
- **当前配置卡：** 显示当前 provider（OpenAI / 自定义）+ 模型名 + 上次测试状态
- 点击"修改"→ 表单抽屉：
  - Base URL（输入框 + 内网地址提示）
  - API Key（密码框，可"显示"）
  - 模型名称（输入框 + 常见模型快捷选项：ChatGLM-6B / Qwen-14B / DeepSeek-Chat）
  - 上下文长度（数字 Slider：4k / 8k / 16k / 32k）
  - Temperature（0~2 Slider，默认 0.7）
- 底部"测试连接"按钮 → 发送测试 → 显示响应内容预览 + 延迟
- 保存后 Toast "LLM 配置已更新，Agent 将使用新模型"

**触发 API：** `PUT /admin/ai/llm-config`, `POST /admin/ai/llm-config/test`

---

### US-EP-03：Function 私有插件运行时

> P2 | Edition：Enterprise+

**As** Application Builder（Enterprise+）
**I want** 为某个 Function 配置"私有 gRPC Sidecar"运行时，让 Function 直接调用企业内部的 gRPC 服务（不走 HTTP）
**So that** 能以最低延迟和最高安全性集成内部系统

**描述：**
私有 gRPC Sidecar 是一个由客户自行部署的小型服务，实现 Palantir 定义的 `FunctionPluginRuntime` gRPC 接口。`function-svc` 通过本地 gRPC 调用 sidecar，sidecar 再调用客户内部系统。Function 配置界面新增"Sidecar"运行时选项。

**验收标准：**
- [ ] 运行时选项新增"Sidecar（私有）"
- [ ] 配置项：Sidecar gRPC 地址 + TLS 设置 + 超时
- [ ] "测试 Sidecar 连通性"验证 gRPC 连接
- [ ] Sidecar 不可用时 Function 执行失败（不降级），错误信息清晰

**前端行为：**
- **位置：** Function 编辑页 → 运行时类型 Radio → 新增"Sidecar（私有）"选项
- 选择后展开配置区：Endpoint 地址 + TLS 开关（开启后上传证书）+ 超时数字输入
- 测试按钮：发送 ping 请求到 sidecar，验证接口版本兼容性
- 编辑器区域变为"参数映射配置"（sidecar 的输入/输出由 sidecar 接口定义，只配置映射）

**触发 API：** `PUT /v1/functions/{id}/runtime/sidecar`

---

## 附录：完整 US 主索引

> 合并 user-stories-detailed_v0.1.0.md（业务 US）+ 本文（平台/部署 US）

### 业务功能 US（详见 user-stories-detailed_v0.1.0.md）

| 范围 | US 数量 | 说明 |
|------|---------|------|
| Platform Admin（基础）| PA-01 ~ PA-06 | 登录/续期/登出/用户管理/审计 |
| Data Engineer | DE-01 ~ DE-07 | Schema / 数据源 / 摄入 |
| Application Builder | AB-01 ~ AB-06 | Function / Workflow / 集成 |
| Analyst | AN-01 ~ AN-04 | Agent 对话 / 图浏览 |
| Data Scientist | DS-01 | 文件上传向量化 |
| Data Governance | DG-01 ~ DG-04 | 权限策略 / 审计分析 |

### 平台/部署 US（本文）

| ID | 标题 | Persona | 拓扑 | Edition | 优先级 |
|----|------|---------|------|---------|--------|
| US-PA-07 | 查看平台模块状态 | Platform Admin | 全部 | 全部 | P0 |
| US-PA-08 | 启用或禁用功能模块 | Platform Admin | 私有/混合 | 全部 | P1 |
| US-PA-09 | 管理产品版本 Edition | Platform Admin | 全部 | 全部 | P1 |
| US-SA-01 | 新租户自助注册 | 新客户 | SaaS | 全部 | P1 |
| US-SA-02 | Onboarding 引导向导 | Platform Admin | SaaS | 全部 | P1 |
| US-SA-03 | 租户用量监控 | SaaS 运营 | SaaS | 全部 | P1 |
| US-SA-04 | 租户 Edition 升级 | SaaS 运营 | SaaS | 全部 | P1 |
| US-PD-01 | 可视化部署配置管理 | Platform Admin | 私有化 | 全部 | P1 |
| US-PD-02 | License 文件管理 | Platform Admin | 私有化 | 全部 | P0 |
| US-HY-01 | 配置云端 AI 服务连接 | Platform Admin | 混合 | Pro+ | P1 |
| US-HY-02 | 数据边界监控 | Platform Admin | 混合 | Pro+ | P1 |
| US-EP-01 | 注册自定义扩展模块 | App Builder | 私有/混合 | E+ | P2 |
| US-EP-02 | 配置私有 LLM 模型 | Platform Admin | 私有/混合 | Pro+ | P1 |
| US-EP-03 | Function 私有插件运行时 | App Builder | 私有/混合 | E+ | P2 |

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本：14 个平台/部署 US，覆盖 SaaS / 私有化 / 混合 / Enterprise+ 四类场景 |
