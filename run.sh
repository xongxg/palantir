#!/bin/bash
set -euo pipefail

# ── Platform Storage (mybucket) ────────────────────────────────────────────────
# mybucket1 = 用户原始数据源 (read-only, ingest source)
# mybucket  = 平台接收数据目标桶 (write, platform dataset storage)
# ── Listen address ─────────────────────────────────────────────────────────────
# Override: INGEST_ADDR=0.0.0.0:3000 ./run.sh  or  ./run.sh 0.0.0.0:3000
export INGEST_ADDR=${INGEST_ADDR:-0.0.0.0:8080}

export PLATFORM_S3_ENDPOINT=http://43.165.67.145:9000
export PLATFORM_S3_BUCKET=mybucket
export PLATFORM_S3_AK=23357101e03b90d5
export PLATFORM_S3_SK=eea7eb7fc8f64d2de95231945436d04d

# ── Startup banner ─────────────────────────────────────────────────────────────
echo "======================================================"
echo " Palantir Ingest API"
echo "======================================================"
echo " Listen address       : $INGEST_ADDR"
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
echo "[run.sh] starting binary: $BINARY $*"
exec "$BINARY" "$@"
