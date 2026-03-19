# ADR-23: API Gateway 安全防御

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

api-gateway 承接所有外部流量，如何防御各类攻击并统一鉴权授权？

## 决策

**分五层防御，tower 中间件栈实现，分 P0/P1/P2 落地。**

## 防御层次

```
外部请求
  ↓
① 网络层：TLS + IP 过滤
  ↓
② 接入层：Rate Limiting + 请求合法性校验
  ↓
③ 认证层：JWT 验证 + Token 吊销检查
  ↓
④ 授权层：auth-svc（RBAC + ABAC + ReBAC）
  ↓
⑤ 业务层：各服务（已通过前四层）
```

## ① 网络层

| 措施 | 说明 |
|------|------|
| TLS 强制 | HTTPS only，HTTP 请求 301 重定向，下发 HSTS Header |
| IP 黑白名单 | Redis 存储，热更新，不重启 |
| 请求体大小限制 | 超限直接 413，防超大 payload 打垮服务 |

## ② 接入层

**Rate Limiting 三维度（governor crate + Redis）：**

```
per IP：       防 DDoS（1000 req/min）
per User：     防 API 滥用（100 req/min）
per Endpoint： 敏感端点更严（/auth/* 10 req/min）
```

**安全响应头：**

```
Strict-Transport-Security: max-age=31536000
X-Frame-Options: DENY
X-Content-Type-Options: nosniff
Content-Security-Policy: default-src 'self'; script-src 'self'
```

**CORS：**

```rust
CorsLayer::new()
    .allow_origin(["https://app.palantir.com".parse()?])
    .allow_methods([Method::GET, Method::POST])
```

## ③ 认证层（JWT）

```
Bearer Token → gateway 验证：
  1. 签名验证（RS256，公钥验证）
  2. 过期检查（exp claim）
  3. Token 吊销检查（Redis 黑名单）
  4. Audience 校验（aud = "palantir-api"）
  ↓
注入 X-User-Id / X-Roles Header → 传给下游服务
```

**Token 双层设计：**

```
Access Token：  15min 有效期（短）
Refresh Token： 7天，单次使用，用后轮换（Rotation）
```

**Token 吊销（Redis 黑名单）：**

```rust
// 登出时写入 Redis，TTL = token 剩余有效期
redis.set_ex(format!("blacklist:{jti}"), "1", remaining_ttl).await?;

// 每次验证时检查
if redis.exists(format!("blacklist:{jti}")).await? {
    return Err(Unauthorized);
}
```

## ④ 防攻击专项

| 攻击类型 | 防御措施 |
|---------|---------|
| 暴力破解 | 登录端点严格限流 + 连续失败锁定（Redis 计数）|
| Replay 攻击 | JWT jti（唯一 ID）+ 短 exp |
| 注入攻击 | 输入 UTF-8 合法性校验 + 长度限制，业务层参数化查询 |
| CSRF | SameSite=Strict Cookie + CSRF Token（浏览器客户端）|
| 路径遍历 | URL normalize + 拒绝 `../` 序列 |
| 慢速攻击（Slowloris）| Header 读取超时 5s，Body 读取超时 30s |

## ⑤ 审计日志

每个请求记录，写入 auth-svc 审计链（ADR-09）：

```
who（user_id + ip）+ what（method + path）+ when + status + latency
```

## Gateway 中间件完整栈

```rust
Router::new()
    .layer(TraceLayer)               // 请求追踪
    .layer(TimeoutLayer::new(30s))   // 全局超时
    .layer(RequestSizeLimitLayer)    // 请求体大小
    .layer(CorsLayer)                // CORS
    .layer(SecurityHeadersLayer)     // 安全响应头
    .layer(RateLimitLayer)           // 限流（Redis）
    .layer(IpFilterLayer)            // IP 黑白名单
    .layer(JwtAuthLayer)             // JWT 验证 + 吊销检查
    .layer(AuditLogLayer)            // 审计日志
```

## 分阶段落地

| 阶段 | 内容 |
|------|------|
| P0 | TLS + JWT 验证 + 请求体大小限制 + CORS |
| P1 | Rate Limiting（Redis）+ IP 黑名单 + Token 吊销 |
| P2 | 暴力破解锁定 + 审计日志 + 慢速攻击防御 |

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
