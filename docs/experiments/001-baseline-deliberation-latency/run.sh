#!/usr/bin/env bash
set -euo pipefail

# Reproduces the numbers in this directory's README.md. No overrides
# on criterion — defaults are the source of truth so the file is
# comparable across machines.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUT_DIR="${ROOT_DIR}/docs/experiments/001-baseline-deliberation-latency/results"

mkdir -p "${OUT_DIR}"
cd "${ROOT_DIR}"

cargo bench -p choreo-core --bench trace_context | tee "${OUT_DIR}/trace_context.txt"
cargo bench -p choreo-app  --bench deliberate   | tee "${OUT_DIR}/deliberate.txt"
