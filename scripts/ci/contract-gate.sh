#!/usr/bin/env bash
set -euo pipefail

# Contract gate: Choreographer is API-first. Sync (gRPC / protobuf) and
# async (AsyncAPI) specifications are the source of truth — generated code
# must stay in sync with them, and breaking changes must be detected here
# before any Rust code is built or tested.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

PROTO_DIR="crates/choreo-proto/proto"
ASYNCAPI_SPEC="specs/asyncapi/choreographer.asyncapi.yaml"

echo ">>> [contract-gate] buf format check"
buf format --diff --exit-code "${PROTO_DIR}"

echo ">>> [contract-gate] buf lint (proto)"
buf lint

echo ">>> [contract-gate] buf breaking (proto, against origin/main)"
if git rev-parse --verify origin/main >/dev/null 2>&1; then
  buf breaking --against ".git#branch=origin/main,subdir=${PROTO_DIR}" || {
    echo "::warning::proto breaking change detected vs origin/main"
    # Soft-fail window: keep hard fail commented out until first release.
    # exit 1
  }
else
  echo "::notice::no origin/main reference; skipping breaking check"
fi

echo ">>> [contract-gate] asyncapi validate"
asyncapi validate "${ASYNCAPI_SPEC}"

echo ">>> [contract-gate] OK"
