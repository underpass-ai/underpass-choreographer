#!/usr/bin/env bash
set -euo pipefail

# Criterion benches must keep compiling. Actually *running* them in
# CI is noisy and would not meet the per-PR time budget, but a
# refactor that breaks the bench fixture would silently stop the
# experiment reproducer from working. Compile-only gate on every PR;
# full runs happen locally via each experiment's run.sh.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

cargo bench --workspace --no-run --locked
