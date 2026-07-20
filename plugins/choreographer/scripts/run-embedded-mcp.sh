#!/usr/bin/env bash
set -euo pipefail

PLUGIN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${PLUGIN_ROOT}/bin/choreo-mcp"

if [[ ! -x "${BINARY}" ]]; then
  echo "choreographer plugin: missing executable ${BINARY}" >&2
  echo "choreographer plugin: build the local plugin bundle before installing it" >&2
  exit 127
fi

export CHOREO_MCP_BACKEND=embedded
exec "${BINARY}"
