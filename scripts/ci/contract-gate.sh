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
VENDORED_PROTO="crates/choreo-mcp-proto/proto/underpass/choreo/v1/choreo.proto"

echo ">>> [contract-gate] buf format check"
buf format --diff --exit-code "${PROTO_DIR}"

echo ">>> [contract-gate] buf lint (proto)"
buf lint

echo ">>> [contract-gate] buf breaking (proto, against origin/main)"
if git rev-parse --verify origin/main >/dev/null 2>&1; then
  buf breaking --against ".git#branch=origin/main,subdir=${PROTO_DIR}"
else
  echo "::notice::no origin/main reference; skipping breaking check"
fi

echo ">>> [contract-gate] vendored proto is the same contract"
# `choreo-mcp-proto` vendors the contract so `choreo-mcp` can be
# published to crates.io with its proto dependency already on the
# registry. Two copies of one contract drift silently: the published
# MCP adapter would speak a wire the server no longer serves, and
# nothing would say so until a call failed in someone else's cluster.
if ! diff -u "${PROTO_DIR}/underpass/choreo/v1/choreo.proto" \
  "${VENDORED_PROTO}" >/dev/null; then
  echo "::error::${VENDORED_PROTO} has drifted from ${PROTO_DIR}" >&2
  diff -u "${PROTO_DIR}/underpass/choreo/v1/choreo.proto" "${VENDORED_PROTO}" >&2 || true
  echo "fix: cp ${PROTO_DIR}/underpass/choreo/v1/choreo.proto ${VENDORED_PROTO}" >&2
  exit 1
fi

echo ">>> [contract-gate] asyncapi validate"
asyncapi validate "${ASYNCAPI_SPEC}"

echo ">>> [contract-gate] OK"
