# 前端域分离设计：Admin 与 App 完全分域

> 版本：v0.1.0 | 日期：2026-03-19
> 关联：ADR-16（前端选型）、ADR-33（模块化部署）、ADR-04（多租户）

---

## 一、决策

**Admin 控制台与业务应用完全分域，部署为两个独立的 React 应用，通过独立域名访问。**

---

## 二、域名规划

### SaaS 部署

```
ops.palantir.io              ← Palantir 内部超级运营台（SA-03/04）
                                仅 Palantir 员工访问，不对外

{tenant-slug}.admin.palantir.io  ← 租户管理控制台（TenantAdmin / Platform Admin）
                                   示例：acme.admin.palantir.io

{tenant-slug}.palantir.io    ← 租户业务应用（所有业务 Persona）
                                示例：acme.palantir.io
```

### 私有化部署

```
admin.{客户域名}             ← 管理控制台
                               示例：admin.palantir.acme.com

app.{客户域名}               ← 业务应用
                               示例：app.palantir.acme.com
```

### 混合部署

与私有化相同，两个域名均在客户内网，无公网暴露。

---

## 三、两个应用的职责边界

```
┌──────────────────────────────────┐   ┌──────────────────────────────────┐
│     Admin App                    │   │     Business App                  │
│  admin.{tenant}.palantir.io      │   │  {tenant}.palantir.io             │
│                                  │   │                                   │
│  使用者：Platform Admin           │   │  使用者：全部业务 Persona           │
│          TenantAdmin             │   │  (Data Engineer / Analyst /       │
│          SaaS 超级运营            │   │   App Builder / Data Scientist /  │
│                                  │   │   Data Governance)                │
│  职责：                          │   │                                   │
│  - 用户 & 成员管理               │   │  职责：                            │
│  - 组织架构（部门树）             │   │  - 数据工程工作台                   │
│  - 模块启停 & 配置               │   │  - Agent 对话                      │
│  - License 管理                  │   │  - Function & Workflow 设计        │
│  - 审计日志查看                  │   │  - Ontology 图浏览                 │
│  - 部署配置（私有化）             │   │  - 数据治理策略                    │
│  - 租户用量监控（SaaS 运营）      │   │  - 项目管理                        │
│  - 数据边界监控（混合）           │   │                                   │
└──────────────────────────────────┘   └──────────────────────────────────┘
```

**原则：** Admin App 不展示任何业务数据内容（OntologyObject），只做配置和治理。Business App 不做任何系统配置操作。

---

## 四、安全策略差异

| 安全策略 | Admin App | Business App |
|---------|-----------|--------------|
| MFA 强制 | ✅ 必须 | ❌ 可选 |
| IP 白名单 | ✅ 建议配置 | ❌ 不限制 |
| Session 超时 | 30 分钟（更严格）| 2 小时 |
| CSP 策略 | 更严格（禁止 inline script）| 标准 |
| CORS 允许来源 | 仅 admin 域名 | 仅 app 域名 |
| JWT Audience | `aud: admin` | `aud: app` |
| API Gateway 路由 | `/admin/**` 路由限制只接受 `aud:admin` Token | 业务路由不接受 `aud:admin` Token |
| Cookie Domain | `.admin.palantir.io` | `.palantir.io` |

**JWT Audience 隔离的意义：** 即使用户拿到了 Business App 的 Token，也无法调用 Admin API，因为 api-gateway 在 Admin 路由上会检查 `aud` 字段。

---

## 五、两个 App 之间的跳转

用户在两个域之间切换时，通过 SSO 跳转，无需重新登录：

```
Business App → Admin App：
  用户点击"管理控制台"→
  携带当前 Access Token → POST /auth/exchange?target=admin →
  auth-svc 颁发新的 aud:admin Token →
  302 跳转到 admin.{tenant}.palantir.io?token={admin_token}

Admin App → Business App：
  用户点击"返回应用" →
  同理，交换一个 aud:app Token →
  302 跳转到 {tenant}.palantir.io
```

**Token Exchange 接口：**
- 只允许同 tenant 内的跨域跳转
- Exchange 后原 Token 不失效（同时持有两个短期 Token 合法）
- Admin Token 有效期更短（15 分钟，不自动续期超过 30 分钟）

---

## 六、各自的路由结构

### Admin App 路由

```
/                         → 重定向到 /dashboard
/dashboard                → 概览（模块状态 + 活跃告警）

/members                  → 成员管理列表（US-TN-04）
/members/invite           → 邀请成员（US-TN-03）
/members/:id              → 成员详情

/org                      → 组织架构（部门树）（US-DP-01）
/org/:dept_id             → 部门详情 + 成员 + DeptAdmin 设置

/modules                  → 模块状态总览（US-PA-07）
/modules/config           → 模块启停配置（US-PA-08）

/edition                  → Edition & License 管理（US-PA-09、US-PD-02）

/audit                    → 审计日志（US-PA-06）
/audit/analytics          → 审计分析（US-DG-04）

/deployment               → 部署配置（US-PD-01，仅私有化）
/deployment/hybrid        → 混合连接配置（US-HY-01、US-HY-02）
/deployment/ai            → LLM & Embedding 配置（US-EP-02）

/extensions               → 扩展模块管理（US-EP-01，仅 Enterprise+）

/tenants                  → 租户列表（仅 SaaS 运营 ops 域）
/tenants/:id              → 租户详情 + 用量（US-SA-03）
```

### Business App 路由

```
/                         → 重定向（按 Persona 判断）

/home                     → 首页（TenantAdmin 全企业视图 / DeptAdmin 部门视图 / 个人项目列表）

/projects                 → 项目列表（发现项目）（US-PJ-04）
/projects/:id             → 项目首页（成员 + 数据概览）
/projects/:id/members     → 项目成员管理（US-PJ-02）

/agent                    → Agent 全屏对话（US-AN-01）
/agent/sessions/:id       → 历史会话详情

/ontology                 → Ontology 图浏览（US-AN-04）
/ontology/schema          → Schema 管理（US-DE-01、US-DE-02）
/ontology/objects         → 对象列表
/ontology/objects/:id     → 对象详情（含血缘、历史）

/ingest                   → 数据源列表（US-DE-03）
/ingest/:source_id        → 数据源详情（映射 + 任务 + 历史）

/functions                → Function 列表（US-AB-01）
/functions/:id            → Function 编辑器（Monaco）

/workflows                → Workflow 列表（US-AB-03）
/workflows/:id/design     → Workflow 设计器（React Flow）
/workflows/:id/runs       → 执行历史（US-AB-05）

/governance               → 治理首页（字段分类总览）（US-DG-03）
/governance/policies      → ABAC 策略列表（US-DG-02）
/governance/permissions   → EntityType 权限配置（US-DG-01）

/files                    → 文件库（US-DS-01）

/settings                 → 个人设置（密码、通知偏好、API Keys）
```

---

## 七、代码仓库结构

两个 App 共享组件库和 API 客户端，各自独立构建：

```
palantir/
└── frontend/
    ├── packages/
    │   ├── ui/                   # 共享组件库（Button、Table、Monaco、ReactFlow 封装）
    │   │   └── src/
    │   │       ├── components/
    │   │       ├── hooks/
    │   │       └── theme/        # CSS Token（支持 Enterprise+ 白标）
    │   │
    │   └── api-client/           # openapi-typescript 生成 + 封装
    │       ├── generated/        # 自动生成，不手写
    │       └── client.ts         # base URL + auth header + token refresh
    │
    ├── apps/
    │   ├── admin/                # Admin 控制台 React App
    │   │   ├── src/
    │   │   │   ├── pages/        # 对应上方 Admin 路由
    │   │   │   ├── features/     # members / org / modules / audit ...
    │   │   │   └── main.tsx
    │   │   └── vite.config.ts    # VITE_API_BASE = admin API endpoint
    │   │
    │   └── app/                  # 业务 React App
    │       ├── src/
    │       │   ├── pages/        # 对应上方 Business 路由
    │       │   ├── features/     # agent / ontology / ingest / functions ...
    │       │   └── main.tsx
    │       └── vite.config.ts    # VITE_API_BASE = app API endpoint
    │
    └── package.json              # pnpm workspace
```

---

## 八、Module 可见性在 Business App 中的体现

Business App 启动时调用 `GET /meta/modules`，根据结果动态裁剪路由和导航：

```typescript
// Business App 启动逻辑
const modules = await fetchModules();
const persona  = currentUser.persona;

const visibleRoutes = ALL_ROUTES
  .filter(r => r.requiredModule === null || modules.enabled.includes(r.requiredModule))
  .filter(r => r.requiredPersona === null || persona.includes(r.requiredPersona));
```

**效果：**
- Lite Edition 的用户 → LeftNav 只有 Ontology + 治理，没有 Agent / Ingest / Workflow 入口
- 禁用 workflow-svc 的部署 → Workflow 菜单项消失，URL 直接访问返回 501 页面（"此功能未启用"）

---

## 九、两个 App 的 Nginx 配置要点（私有化）

```nginx
# Admin App
server {
    listen 443 ssl;
    server_name admin.palantir.acme.com;

    # 更严格的安全 Header
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains";
    add_header Content-Security-Policy "default-src 'self'; script-src 'self'";
    add_header X-Frame-Options DENY;       # Admin 绝不允许被 iframe

    # IP 白名单（可选，仅内网访问）
    allow 10.0.0.0/8;
    deny all;

    location / {
        root /var/www/admin;
        try_files $uri /index.html;
    }
}

# Business App
server {
    listen 443 ssl;
    server_name app.palantir.acme.com;

    add_header X-Frame-Options SAMEORIGIN;  # 允许同域 iframe（嵌入场景）

    location / {
        root /var/www/app;
        try_files $uri /index.html;
    }
}
```

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本：域名规划、安全差异、路由结构、代码仓库结构、Module 可见性 |
