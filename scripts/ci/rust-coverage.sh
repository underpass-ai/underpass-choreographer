#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

: "${COVERAGE_MIN:=80}"

mkdir -p target/llvm-cov

cargo llvm-cov clean --workspace
cargo llvm-cov --workspace --locked --no-report
cargo llvm-cov report --locked --lcov --output-path target/llvm-cov/lcov.info

# Enforce minimum unit coverage. Target band is 80–90 % across the
# production crates. The gate is deliberately parsed from the JSON
# summary so we fail fast on regressions in CI.
SUMMARY_JSON="target/llvm-cov/summary.json"
cargo llvm-cov report --locked --json --summary-only --output-path "${SUMMARY_JSON}"

COVERAGE_PCT="$(
  python3 -c "
import json, sys
with open('${SUMMARY_JSON}') as f:
    data = json.load(f)
total = data['data'][0]['totals']['lines']['percent']
print(f'{total:.2f}')
"
)"

echo ">>> coverage (lines): ${COVERAGE_PCT}% (minimum ${COVERAGE_MIN}%)"

python3 - "${COVERAGE_PCT}" "${COVERAGE_MIN}" <<'PY'
import sys
pct = float(sys.argv[1])
threshold = float(sys.argv[2])
if pct + 1e-9 < threshold:
    sys.stderr.write(f"coverage gate failed: {pct}% < {threshold}%\n")
    sys.exit(1)
PY
