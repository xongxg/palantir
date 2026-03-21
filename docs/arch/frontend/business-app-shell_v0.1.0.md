# Business App — Shell 结构与页面设计

> 版本：v0.1.0 | 日期：2026-03-19
> 关联：domain-separation_v0.1.0.md、multi-device-strategy_v0.1.0.md、ADR-16（前端选型）、ADR-33（模块化部署）

---

## 一、Shell 整体布局

Business App 采用三区布局：LeftNav + 主内容区 + 底部 Agent 条。

```
┌─────────────────────────────────────────────────────────────────┐
│  TopBar                                                          │
│  [Logo] [项目切换器 ▾]         [搜索 ⌘K]  [通知 🔔]  [用户 ▾]  │
├───────┬─────────────────────────────────────────────────────────┤
│       │                                                          │
│ Left  │  主内容区                                                 │
│ Nav   │  (路由对应的页面)                                         │
│       │                                                          │
│ 48px  │                                                          │
│(图标) │                                                          │
│或     │                                                          │
│ 220px │                                                          │
│(展开) │                                                          │
│       │                                                          │
├───────┴─────────────────────────────────────────────────────────┤
│  Agent 输入条（常驻，Desktop+）                                   │
│  [✨ 问 AI...                              ] [🎤] [⏎]           │
└─────────────────────────────────────────────────────────────────┘
```

**断点行为：**

| 宽度 | LeftNav | Agent 条 | 详情面板 |
|------|---------|---------|---------|
| ≥1280px (Desktop+) | 展开 220px | 常驻底部条 | 右侧固定面板 |
| 1024-1279px (Laptop) | 图标模式 48px，Hover 展开 | 常驻底部条 | Overlay 覆盖层 |
| 768-1023px (Tablet) | 隐藏，底部 Tab Bar | 悬浮 FAB 按钮 | 底部 Sheet |
| <768px (Mobile) | 底部 Tab Bar 4项 | 全屏 Agent 页 | 全屏页面 |

---

## 二、LeftNav 导航结构

LeftNav 的菜单项按两个维度动态裁剪：
1. **Persona 过滤**：只显示该 Persona 有权访问的模块
2. **Module 过滤**：只显示已启用的服务模块（`GET /meta/modules`）

```
LeftNav 完整项目（按角色可见性）：

🏠  Home             [所有 Persona]
💬  Agent            [所有 Persona，需 agent-svc]
🕸   Ontology         [所有 Persona]
    ├── 图浏览
    ├── Schema 管理    [Data Engineer, Data Governance]
    └── 对象列表
📥  Ingest            [Data Engineer，需 ingest-svc]
⚡  Functions         [App Builder，需 function-svc]
🔄  Workflows         [App Builder，需 workflow-svc]
📊  治理              [Data Governance，需 ontology-svc]
    ├── 字段分类
    ├── ABAC 策略
    └── 权限配置
📁  文件库            [Data Engineer, Data Scientist，需 embedding-svc]
🔔  通知              [所有 Persona]
⚙️  个人设置          [所有 Persona]
```

**代码实现：**

```typescript
// 导航配置表（声明式，不写判断逻辑）
const NAV_ITEMS: NavItemDef[] = [
  { key: 'home',       icon: HomeIcon,      label: '首页',    path: '/home',
    personas: null, module: null },
  { key: 'agent',      icon: SparklesIcon,  label: 'Agent',   path: '/agent',
    personas: null, module: 'agent-svc' },
  { key: 'ontology',   icon: NetworkIcon,   label: 'Ontology', path: '/ontology',
    personas: null, module: null },
  { key: 'ingest',     icon: DatabaseIcon,  label: '数据接入', path: '/ingest',
    personas: ['data_engineer'], module: 'ingest-svc' },
  { key: 'functions',  icon: FunctionIcon,  label: 'Functions', path: '/functions',
    personas: ['app_builder'], module: 'function-svc' },
  { key: 'workflows',  icon: WorkflowIcon,  label: 'Workflows', path: '/workflows',
    personas: ['app_builder'], module: 'workflow-svc' },
  { key: 'governance', icon: ShieldIcon,    label: '治理',    path: '/governance',
    personas: ['data_governance', 'platform_admin'], module: null },
  { key: 'files',      icon: FolderIcon,    label: '文件库',  path: '/files',
    personas: ['data_engineer', 'data_scientist'], module: 'embedding-svc' },
  { key: 'notify',     icon: BellIcon,      label: '通知',    path: '/notifications',
    personas: null, module: null },
];

function useVisibleNavItems(modules: string[], persona: Persona): NavItemDef[] {
  return NAV_ITEMS.filter(item =>
    (item.module === null || modules.includes(item.module)) &&
    (item.personas === null || item.personas.includes(persona))
  );
}
```

---

## 三、按 Persona 的代码分割（必须实现）

**原则：Analyst 不应该下载 Workflow 设计器的 JS，Data Engineer 不应该下载 Monaco 之外的代码。**

Monaco Editor 约 2MB，React Flow 约 300KB。错误的打包策略会让首屏 JS 达到 4MB+。

### 分割方案

```typescript
// router.tsx — 路由级 lazy 加载
import { lazy } from 'react';

// App Builder workbench（含 Monaco + React Flow，最重，约 2.5MB gzip 前）
const FunctionEditor   = lazy(() => import('./features/functions/FunctionEditor'));
const WorkflowDesigner = lazy(() =>
  import(/* webpackChunkName: "workbench-app-builder" */ './features/workflows/WorkflowDesigner')
);

// Data Engineer workbench（含 Ingest 配置 + Schema 管理）
const IngestSourceConfig = lazy(() => import('./features/ingest/SourceConfig'));
const SchemaManager      = lazy(() => import('./features/ontology/SchemaManager'));

// 通用模块（所有 Persona，打入主 bundle）
import AgentPanel    from './features/agent/AgentPanel';  // SSE 轻量
import OntologyGraph from './features/ontology/Graph';    // React Flow 只读，单独 chunk
```

### Vite 分割配置

```typescript
// vite.config.ts
export default defineConfig({
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('monaco-editor'))                        return 'vendor-monaco';
          if (id.includes('reactflow') || id.includes('@xyflow')) return 'vendor-reactflow';
          if (id.includes('@radix-ui'))                           return 'vendor-ui';
          if (id.includes('@palantir/api-client'))                return 'api-client';
        }
      }
    }
  }
});
```

### 首屏加载目标

| Persona | 首屏 JS（gzip）| 懒加载 chunk |
|---------|---------------|-------------|
| Analyst | ~180KB | — |
| Data Engineer | ~280KB | Monaco（按需） |
| App Builder | ~220KB | Monaco + ReactFlow（进入编辑页时） |
| Data Scientist | ~200KB | — |
| Data Governance | ~180KB | — |

Monaco 和 ReactFlow 仅在用户实际进入编辑页面时触发加载，配合 `<Suspense fallback={<EditorSkeleton />}>`。

---

## 四、CSS Token 系统与 Enterprise+ 白标

### 4.1 权衡结论

采用 **全局 CSS Variables（仅颜色+字体）** 方案，不引入 runtime theme provider。

白标范围边界：

```
✅ 主题色替换（按钮、链接、高亮）
✅ Logo / Favicon / 产品名称替换
✅ 字体替换
❌ 布局重设计（不是白标范畴）
❌ 逐组件单独定制（维护成本爆炸）
```

shadcn/ui 本身基于 CSS Variables 构建，完全兼容此方案，无额外运行时成本。

**开发规范**：组件代码里不允许写字面量颜色值，只允许用语义 Token。

### 4.2 Token 分层

```css
/* Layer 1：Primitive Token（不暴露给客户，内部使用） */
--color-blue-600: #2563eb;
--color-blue-700: #1d4ed8;
--color-slate-100: #f1f5f9;

/* Layer 2：语义 Token（客户替换这一层） */
--brand-primary:       var(--color-blue-600);
--brand-primary-hover: var(--color-blue-700);
--brand-primary-text:  #ffffff;
--brand-secondary:     var(--color-slate-100);
--font-sans:           'Inter', system-ui, sans-serif;
--font-mono:           'JetBrains Mono', monospace;

/* Layer 3：组件内部引用语义 Token */
/* .btn-primary { background: var(--brand-primary); color: var(--brand-primary-text); } */
```

### 4.3 运行时注入（启动时）

```typescript
interface BrandConfig {
  product_name:        string;   // 替换 "Palantir"
  logo_url:            string;
  favicon_url:         string;
  primary_color:       string;   // hex
  primary_hover_color: string;
  primary_text_color:  string;
  secondary_color:     string;
  font_family?:        string;
  font_url?:           string;   // Google Fonts 或企业 CDN
}

function applyBrandTokens(brand: BrandConfig) {
  const root = document.documentElement;
  root.style.setProperty('--brand-primary',       brand.primary_color);
  root.style.setProperty('--brand-primary-hover', brand.primary_hover_color);
  root.style.setProperty('--brand-primary-text',  brand.primary_text_color);
  root.style.setProperty('--brand-secondary',     brand.secondary_color);
  if (brand.font_url) {
    document.head.appendChild(
      Object.assign(document.createElement('link'), { rel: 'stylesheet', href: brand.font_url })
    );
    root.style.setProperty('--font-sans', `'${brand.font_family}', system-ui, sans-serif`);
  }
}
```

---

## 五、核心页面结构

### 5.1 首页（/home）

```
┌──────────────────────────────────────────────┐
│  项目概览卡片列表（最近参与的 N 个项目）         │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│  │ 项目 A   │ │ 项目 B   │ │ + 新建   │      │
│  │ 3成员    │ │ 5成员    │ │          │      │
│  └──────────┘ └──────────┘ └──────────┘      │
├──────────────────────────────────────────────┤
│  我的待办（审批、通知摘要）                      │
│  [申请加入项目 X — Alice]  [审批]  [拒绝]      │
├──────────────────────────────────────────────┤
│  最近活动 Feed（Workflow 运行 / 摄入完成等）    │
└──────────────────────────────────────────────┘
```

### 5.2 Agent 页（/agent）

```
┌──────────────────────────────────────────────┐
│  历史会话列表（左侧 sidebar）  │  对话主区      │
│  ┌─────────────────┐          │               │
│  │ 2026-03-19      │          │  [消息气泡]    │
│  │ 分析合同数据     │          │  [工具调用轨迹] │
│  │ + 新建会话       │          │  ▶ 展开执行步骤│
│  └─────────────────┘          ├───────────────┤
│                               │  输入区        │
│                               │  [textarea]   │
│                               │  [附件] [发送] │
└──────────────────────────────────────────────┘
```

工具调用轨迹（折叠展示，Desktop 展开，Mobile 隐藏）：

```typescript
interface TraceStep {
  type:        'tool_call' | 'tool_result' | 'thinking' | 'citation';
  tool?:       string;   // 'search_ontology' | 'run_function' | ...
  input?:      unknown;
  output?:     unknown;
  duration_ms?: number;
  status:      'running' | 'done' | 'error';
}
```

### 5.3 Ontology 图页（/ontology）

```
┌──────────────────────────────────────────────────┐
│  工具栏：[筛选 EntityType ▾] [搜索节点] [缩放]    │
├──────────────────────────────────────────────────┤
│   React Flow 画布（只读 / Data Engineer 可编辑）  │
│   ●Contract ─── ●Company                         │
│       │                                           │
│   ●Person ────────────── ●Project                 │
├──────────────────────────────────────────────────┤
│  右侧详情面板（选中节点后出现）                     │
│  EntityType: Contract                             │
│  字段: id, title, amount, parties...              │
│  关系: PARTY_OF → Person (n)                      │
└──────────────────────────────────────────────────┘
```

### 5.4 Function 编辑器（/functions/:id）— 仅 Desktop+ & App Builder

```
┌────────────┬──────────────────────┬──────────────┐
│  Function  │  Monaco Editor       │  右侧 Schema  │
│  列表      │  // TypeScript       │  面板        │
│  搜索框     │  export default      │  EntityType  │
│  + 新建     │  async function(ctx) │  字段速查     │
│  [foo]     │  { ... }             │  AI 辅助建议  │
│  [bar]     ├──────────────────────┤              │
│            │  测试面板              │              │
│            │  [输入参数] [Run ▶]   │              │
└────────────┴──────────────────────┴──────────────┘
```

### 5.5 Workflow 设计器（/workflows/:id/design）— 仅 Desktop & App Builder

```
┌──────────────┬──────────────────────────┬─────────────┐
│  节点面板     │  React Flow 设计画布      │  属性面板    │
│  触发器：     │   [Trigger] → [Filter]   │  选中节点：  │
│  [定时]      │       ↓                  │  条件表达式: │
│  [Webhook]   │   [FuncNode] → [End]     │  输入映射:  │
│  操作节点：   │   拖拽到画布 →            │  [...]      │
│  [Function]  │                          │             │
└──────────────┴──────────────────────────┴─────────────┘
```

---

## 六、全局交互模式

### 命令面板（⌘K / Ctrl+K）

所有页面均可触发，替代传统搜索框：

```typescript
const COMMANDS: Command[] = [
  { value: 'nav-agent',    label: '打开 Agent',     action: () => navigate('/agent') },
  { value: 'search-obj',   label: '搜索对象...',     action: openObjectSearch },
  { value: 'new-project',  label: '新建项目',        action: openNewProjectModal },
  { value: 'new-workflow', label: '新建 Workflow',   action: () => navigate('/workflows/new') },
  ...projects.map(p => ({
    value: `switch-${p.id}`,
    label: `切换到项目：${p.name}`,
    action: () => switchProject(p.id),
  })),
];
```

### 通知中心（TopBar 🔔）

点击展开 Drawer，实时 SSE 推送：

```typescript
type NotificationKind =
  | 'workflow_failed'
  | 'ingest_completed'
  | 'project_join_request'
  | 'project_join_approved'
  | 'agent_task_done'
  | 'system_alert';          // 仅 Platform Admin
```

### 项目切换器（TopBar 项目名 ▾）

```
[项目 A ▾]
  ──────────────
  ● 项目 A（当前）
    项目 B
    项目 C
  ──────────────
  + 创建新项目
  🔍 发现项目
  ──────────────
  全企业视图（TenantAdmin 专属）
```

---

## 七、App 启动流程

```typescript
async function bootstrap() {
  // 1. 验证 Token（aud:app），过期则 → 登录页
  const user = await authSvc.getMe();

  // 2. 并行加载启动所需数据
  const [modules, brand, projects] = await Promise.all([
    fetchModules(),     // GET /meta/modules → 动态路由裁剪
    fetchBrandConfig(), // GET /meta/brand   → CSS Token 注入
    fetchMyProjects(),  // GET /projects?mine=true
  ]);

  // 3. 注入 CSS Token（白标）
  applyBrandTokens(brand);

  // 4. 初始化全局 Store
  useAppStore.setState({ user, modules, projects });

  // 5. 渲染
  render(<App />);
}
```

---

## 八、组件库分层（packages/ui）

```
primitives/   Button, Input, Table, Badge, Avatar, Dialog, Sheet, Drawer, Tooltip
compound/     CommandPalette, DataTable, MonacoEditor(lazy), FlowCanvas(只读), FlowDesigner(可编辑)
layout/       AppShell, SplitPane, ResizablePanel, PageContainer
hooks/        useBreakpoint, useModules, usePersona, useProject
theme/        tokens.css（CSS Custom Properties 定义）, applyBrand.ts（运行时注入）
```

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本：Shell 布局、LeftNav 动态菜单、Persona 代码分割、CSS Token 白标方案、核心页面结构、启动流程、组件库分层 |
