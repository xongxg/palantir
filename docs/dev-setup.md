# 开发环境启动指南

## 前提条件

- Rust (stable, 推荐 1.78+)
- Node.js 18+ 与 pnpm
- SQLite（系统自带即可，Rust 编译时静态链接）

---

## 一、后端启动

后端通过根目录的 `run.sh` 启动。该脚本统一配置所有环境变量（S3 凭证、数据库路径、监听地址），并管理进程 PID，**不要直接用 `cargo run`**。

### 首次构建

```bash
cargo build -p palantir_ingest_api
```

### 默认启动（端口 8080）

```bash
./run.sh
```

### 指定端口

```bash
# 位置参数（推荐）
./run.sh 0.0.0.0:9090

# 或环境变量
INGEST_ADDR=0.0.0.0:9090 ./run.sh
```

每个端口实例数据完全隔离：
- 数据库：`.run/<port>/palantir.db`
- PID 文件：`.run/<port>/palantir.pid`

### 修改环境变量

直接编辑 `run.sh` 中对应的变量：

```bash
# ── Platform Storage（平台目标桶）────────────────────────────────────────
export PLATFORM_S3_ENDPOINT=http://your-minio-host:9000
export PLATFORM_S3_BUCKET=mybucket
export PLATFORM_S3_AK=your-access-key
export PLATFORM_S3_SK=your-secret-key

# ── 监听地址 ──────────────────────────────────────────────────────────────
export INGEST_ADDR=${INGEST_ADDR:-0.0.0.0:8080}
```

### Release 构建（生产/性能测试）

```bash
cargo build --release -p palantir_ingest_api
PALANTIR_BIN=./target/release/palantir_ingest_api ./run.sh
```

---

## 二、前端启动

### 安装依赖（首次）

```bash
cd frontend
pnpm install
```

### 配置

所有配置集中在 `frontend/apps/app/.env.local`（不提交 git，从模板复制）：

```bash
cp frontend/apps/app/.env.example frontend/apps/app/.env.local
```

`.env.local` 内容：

```bash
# 前端 dev server 端口（默认 3000）
VITE_PORT=3000

# 后端地址（默认 http://localhost:8080）
VITE_API_BASE=http://localhost:8080
```

改完直接生效，无需修改任何代码或配置文件。

### 启动

```bash
cd frontend/apps/app
pnpm dev
```

---

## 三、生产构建

```bash
# 构建前端静态文件（输出到 frontend/dist/）
cd frontend/apps/app
pnpm build

# 后端直接 serve 静态文件，访问 http://localhost:8080 即可
./run.sh
```

---

## 四、常用端口一览

| 服务 | 默认端口 | 配置方式 |
|------|---------|---------|
| 后端 API | 8080 | `run.sh` 中 `INGEST_ADDR`，或 `./run.sh 0.0.0.0:<port>` |
| 前端 dev server | 3000 | `.env.local` → `VITE_PORT` |
| 后端地址（前端 proxy） | — | `.env.local` → `VITE_API_BASE` |
| MinIO / RustFS | 9000 | MinIO 自身配置 |
