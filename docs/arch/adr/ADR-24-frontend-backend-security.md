# ADR-24: 前后端通信安全

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

浏览器环境天然不安全，前后端通信中 Token 如何安全存储和传输？如何防御 XSS、CSRF？

## 决策

**Access Token 存内存，Refresh Token 存 HttpOnly Cookie，双 Token 分离策略。**

## Token 存储方案

| 存储方式 | XSS 风险 | CSRF 风险 | 结论 |
|---------|---------|---------|------|
| localStorage | ❌ 高 | ✅ 无 | 不用 |
| sessionStorage | ❌ 高 | ✅ 无 | 不用 |
| HttpOnly Cookie | ✅ 无 | ⚠️ 有 | 配合 SameSite 缓解 |
| 内存（JS 变量）| ✅ 无 | ✅ 无 | Access Token 用此 |

**双 Token 分离：**

```
Access Token（15min）→ 内存（JS 变量）
  - 每次请求放 Authorization: Bearer header
  - XSS 拿不到，刷新页面丢失 → 用 Refresh Token 静默续签

Refresh Token（7天）→ HttpOnly + Secure + SameSite=Strict Cookie
  - JS 完全不可读，XSS 无效
  - SameSite=Strict 阻断 CSRF
  - 只用于 /auth/refresh 端点
```

## 完整认证流程

```
用户登录
  ↓
POST /auth/login { username, password }
  ↓
响应：Access Token（body，存内存）
     Refresh Token（Set-Cookie: HttpOnly; Secure; SameSite=Strict）

业务请求
  ↓
Authorization: Bearer {access_token}

Access Token 过期
  ↓
POST /auth/refresh（自动携带 Cookie）
  ↓
新 Access Token（body）+ 新 Refresh Token（Cookie 轮换）

登出
  ↓
POST /auth/logout
  ↓
服务端：Refresh Token 写入黑名单 + 清除 Cookie
客户端：清空内存中的 Access Token
```

## XSS 防御

React JSX 默认自动转义，额外规范：

```typescript
// ❌ 禁止直接使用
<div dangerouslySetInnerHTML={{ __html: userContent }} />

// ✅ 用户生成内容必须过 DOMPurify
import DOMPurify from 'dompurify';
<div dangerouslySetInnerHTML={{ __html: DOMPurify.sanitize(userContent) }} />
```

CSP Header（gateway 下发）：

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self';
  style-src 'self';
  connect-src 'self' https://api.palantir.com;
  img-src 'self' data:;
```

## SSE / WebSocket 安全

**SSE（当前阶段）：**

```typescript
// ❌ 原生 EventSource 不支持自定义 Header
new EventSource('/v1/query');

// ✅ 用 fetch + ReadableStream 替代，支持 Authorization Header
const response = await fetch('/v1/query', {
    method: 'POST',
    headers: { Authorization: `Bearer ${accessToken}` },
    body: JSON.stringify({ query }),
});
const reader = response.body?.getReader();
```

**WebSocket（中期）：**

```typescript
// ❌ Token 不能放 URL（会被日志、浏览器历史记录）
new WebSocket('wss://api/ws?token=xxx');

// ✅ 连接建立后第一条消息发认证
const ws = new WebSocket('wss://api/ws');
ws.onopen = () => ws.send(JSON.stringify({ type: 'auth', token: accessToken }));
```

## 敏感数据传输规范

```
✅ 所有请求走 HTTPS
✅ PII 字段响应前脱敏（auth-svc AllowWithMask）
✅ 错误响应统一格式 { code, message }，不暴露内部堆栈
✅ 日志不记录 Token、密码、PII 字段
❌ 不在 URL 中传递敏感参数（会被日志和浏览器历史记录）
```

## 分阶段落地

| 阶段 | 内容 |
|------|------|
| P0 | HTTPS + Access Token 内存存储 + HttpOnly Cookie Refresh Token |
| P1 | CSP Header + Token 自动续签 + 登出黑名单 |
| P2 | SSE 用 fetch 替代 EventSource + WebSocket 认证握手 |
