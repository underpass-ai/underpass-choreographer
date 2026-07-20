#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLUGIN_DIR="${ROOT_DIR}/plugins/choreographer"
FIXTURE="${ROOT_DIR}/tests/plugin/choreographer-smoke.jsonl"

cd "${ROOT_DIR}"
python3 -m json.tool "${PLUGIN_DIR}/.codex-plugin/plugin.json" >/dev/null
python3 -m json.tool "${PLUGIN_DIR}/.mcp.json" >/dev/null
bash scripts/plugin/build-local-choreographer-plugin.sh

responses="$("${PLUGIN_DIR}/scripts/run-embedded-mcp.sh" <"${FIXTURE}")"

response_contains() {
  local needle="$1"
  if command -v rg >/dev/null 2>&1; then
    rg -Fq -- "${needle}"
  else
    grep -Fq -- "${needle}"
  fi
}

if [[ "$(printf '%s\n' "${responses}" | wc -l)" -ne 3 ]]; then
  echo "choreographer plugin smoke expected three MCP responses" >&2
  exit 1
fi

if ! response_contains '"backend":"embedded"' <<<"${responses}"; then
  echo "choreographer plugin smoke did not initialize the embedded backend" >&2
  exit 1
fi

if ! response_contains '"name":"choreo_run_ceremony"' <<<"${responses}"; then
  echo "choreographer plugin smoke did not advertise the ceremony tool" >&2
  exit 1
fi

if ! response_contains '"name":"choreo_approve_ceremony_guard"' <<<"${responses}"; then
  echo "choreographer plugin smoke did not advertise incremental authorization" >&2
  exit 1
fi

if ! response_contains '"completed":true' <<<"${responses}"; then
  echo "choreographer plugin smoke did not complete the ceremony" >&2
  exit 1
fi

echo "choreographer Codex plugin smoke passed"
