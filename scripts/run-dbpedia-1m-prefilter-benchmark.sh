#!/usr/bin/env bash
set -euo pipefail

: "${HELIX_DBPEDIA_1M_LOG:?set HELIX_DBPEDIA_1M_LOG to a persistent log path}"
: "${HELIX_DBPEDIA_1M_FBIN:?set HELIX_DBPEDIA_1M_FBIN to the fixture path}"
: "${HELIX_DBPEDIA_1M_DB_PARENT:?set HELIX_DBPEDIA_1M_DB_PARENT to a scratch directory}"

benchmark_repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
mkdir -p "$(dirname "$HELIX_DBPEDIA_1M_LOG")"
mkdir -p "$(dirname "$HELIX_DBPEDIA_1M_FBIN")"
mkdir -p "$HELIX_DBPEDIA_1M_DB_PARENT"
exec >>"$HELIX_DBPEDIA_1M_LOG" 2>&1

benchmark_started=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
echo "DBPEDIA_1M_BENCHMARK phase=start timestamp=$benchmark_started repo=$benchmark_repo"
trap 'benchmark_status=$?; benchmark_finished=$(date -u +"%Y-%m-%dT%H:%M:%SZ"); echo "DBPEDIA_1M_BENCHMARK phase=finish timestamp=$benchmark_finished status=$benchmark_status"' EXIT

cd "$benchmark_repo"
git status --short --branch
git rev-parse HEAD
sw_vers
/usr/sbin/sysctl -n hw.memsize
df -h "$HELIX_DBPEDIA_1M_DB_PARENT"

if [[ ! -f "$HELIX_DBPEDIA_1M_FBIN" ]]; then
    python3 scripts/prepare-dbpedia-vector-fixture.py \
        --rows 1000000 \
        "$HELIX_DBPEDIA_1M_FBIN"
fi

export RUST_BACKTRACE=1
cargo test \
    --release \
    --package db \
    --features production-scale \
    --test production_index_lifecycle_scale \
    traversal_vector_prefilter_1m_scale_contract \
    -- \
    --ignored \
    --exact \
    --nocapture
