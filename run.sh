#!/bin/bash
set -euo pipefail

# ── Listen address ─────────────────────────────────────────────────────────────
# 单实例：  ./run.sh
# 指定端口：INGEST_ADDR=0.0.0.0:3000 ./run.sh
# 多实例：  INGEST_ADDR=0.0.0.0:8080 ./run.sh &
#           INGEST_ADDR=0.0.0.0:8081 ./run.sh &
export INGEST_ADDR=${INGEST_ADDR:-0.0.0.0:8080}

# ── Per-instance isolation ─────────────────────────────────────────────────────
# 每个端口独立的数据目录和 PID 文件，互不干扰
PORT=${INGEST_ADDR##*:}
INSTANCE_DIR=".run/${PORT}"
mkdir -p "$INSTANCE_DIR"

PID_FILE="$INSTANCE_DIR/palantir.pid"
# 数据库路径也按端口隔离（二进制通过环境变量读取）
export PALANTIR_DB="$INSTANCE_DIR/palantir.db"

# ── Platform Storage ───────────────────────────────────────────────────────────
export PLATFORM_S3_ENDPOINT=http://43.165.67.145:9000
export PLATFORM_S3_BUCKET=mybucket
export PLATFORM_S3_AK=23357101e03b90d5
export PLATFORM_S3_SK=eea7eb7fc8f64d2de95231945436d04d

# ── Startup banner ─────────────────────────────────────────────────────────────
echo "======================================================"
echo " Palantir Ingest API  [port $PORT]"
echo "======================================================"
echo " Listen address       : $INGEST_ADDR"
echo " DB path              : $PALANTIR_DB"
echo " Platform S3 endpoint : $PLATFORM_S3_ENDPOINT"
echo " Platform S3 bucket   : $PLATFORM_S3_BUCKET"
echo " Platform S3 AK       : ${PLATFORM_S3_AK:0:6}..."
echo " Platform S3 SK       : ******"
echo "------------------------------------------------------"
echo " Source bucket (RO)   : mybucket1  (user raw data)"
echo " Target bucket (RW)   : mybucket   (platform storage)"
echo "======================================================"
echo ""

BINARY=${PALANTIR_BIN:-./target/debug/palantir_ingest_api}

if [ ! -f "$BINARY" ]; then
    echo "[run.sh] binary not found, building..."
    cargo build -p palantir_ingest_api
fi

# ── Kill only the instance on THIS port (via PID file) ────────────────────────
if [ -f "$PID_FILE" ]; then
    OLD_PID=$(cat "$PID_FILE")
    if kill -0 "$OLD_PID" 2>/dev/null; then
        echo "[run.sh:$PORT] stopping existing instance (pid $OLD_PID)"
        kill "$OLD_PID"
        sleep 1
    fi
    rm -f "$PID_FILE"
fi

# ── Start & record PID ────────────────────────────────────────────────────────
echo "[run.sh:$PORT] starting: $BINARY $*"
"$BINARY" "$@" &
echo $! > "$PID_FILE"
echo "[run.sh:$PORT] pid $(cat $PID_FILE) → $PID_FILE"
wait
