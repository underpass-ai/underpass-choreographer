#!/usr/bin/env bash
set -euo pipefail

# Reproduces the 4×4 agents/rounds grid recorded in this
# directory's README.md. Uses the same moderate criterion budget
# (2 s measurement, 50 samples, 1 s warm-up) we used to collect
# the reference numbers — running with defaults produces tighter
# intervals but takes longer.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUT_DIR="${ROOT_DIR}/docs/experiments/002-deliberation-scale-sweep/results"

mkdir -p "${OUT_DIR}"
cd "${ROOT_DIR}"

cargo bench -p choreo-app --bench deliberate -- \
    --warm-up-time 1 \
    --measurement-time 2 \
    --sample-size 50 \
    | tee "${OUT_DIR}/deliberate-grid.txt"
