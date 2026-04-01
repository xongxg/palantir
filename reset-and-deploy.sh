#!/bin/bash
set -euo pipefail

# ── 参数 ───────────────────────────────────────────────────────────────────────
PORT=${1:-8080}
INSTANCE_DIR=".run/${PORT}"
DB_PATH="${INSTANCE_DIR}/palantir.db"
PID_FILE="${INSTANCE_DIR}/palantir.pid"

echo "======================================================"
echo " Palantir Reset & Deploy  [port $PORT]"
echo "======================================================"

# ── 1. 停止运行中的实例 ────────────────────────────────────────────────────────
if [ -f "$PID_FILE" ]; then
    OLD_PID=$(cat "$PID_FILE")
    if kill -0 "$OLD_PID" 2>/dev/null; then
        echo "[reset] stopping instance (pid $OLD_PID)..."
        kill "$OLD_PID"
        sleep 1
    fi
    rm -f "$PID_FILE"
    echo "[reset] stopped."
else
    echo "[reset] no running instance found."
fi

# ── 2. 清空 SQLite 数据库 ──────────────────────────────────────────────────────
if [ -f "$DB_PATH" ]; then
    echo "[reset] removing $DB_PATH ..."
    rm -f "$DB_PATH"
    echo "[reset] database cleared."
else
    echo "[reset] no database found at $DB_PATH, skip."
fi

# ── 3. 重新编译 ────────────────────────────────────────────────────────────────
echo "[deploy] building..."
cargo build -p palantir_ingest_api
echo "[deploy] build done."

# ── 4. 重启服务 ────────────────────────────────────────────────────────────────
echo "[deploy] starting on port $PORT ..."
INGEST_ADDR="0.0.0.0:${PORT}" ./run.sh
