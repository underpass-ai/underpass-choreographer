#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLUGIN_DIR="${ROOT_DIR}/plugins/choreographer"
BINARY="${ROOT_DIR}/target/release/choreo-mcp"

cd "${ROOT_DIR}"
cargo build --release --locked -p choreo-mcp --no-default-features --features embedded
mkdir -p "${PLUGIN_DIR}/bin"
cp "${BINARY}" "${PLUGIN_DIR}/bin/choreo-mcp"
chmod +x "${PLUGIN_DIR}/bin/choreo-mcp"

echo "choreographer plugin bundle ready at ${PLUGIN_DIR}"
