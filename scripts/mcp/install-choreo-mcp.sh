#!/usr/bin/env bash
#
# Install the `choreo-mcp` binary.
#
# Two modes, controlled by `CHOREO_MCP_INSTALL_MODE`:
#
#   - `registry` (default): `cargo install choreo-mcp` from crates.io.
#                           This is the canonical end-user path. The
#                           first registry release lands the next time
#                           a `v*` tag is pushed.
#   - `git`:                `cargo install --git ...` from the source
#                           repository. Supports pinning to a branch,
#                           tag, or revision via env vars (mutually
#                           exclusive). Use this when validating an
#                           unreleased change against a checkout.
#
# Usage:
#   bash scripts/mcp/install-choreo-mcp.sh                    # registry
#   CHOREO_MCP_INSTALL_MODE=git    bash scripts/mcp/install-choreo-mcp.sh
#   CHOREO_MCP_INSTALL_MODE=git CHOREO_MCP_TAG=v0.1.0    bash …
#   CHOREO_MCP_INSTALL_MODE=git CHOREO_MCP_BRANCH=main   bash …
#   CHOREO_MCP_INSTALL_MODE=git CHOREO_MCP_REV=<git-sha> bash …
#
# CARGO_INSTALL_ROOT (optional): change where cargo writes the binary.

set -euo pipefail

MODE="${CHOREO_MCP_INSTALL_MODE:-registry}"

case "${MODE}" in
  registry)
    cmd=(cargo install choreo-mcp --locked --force)
    if [[ -n "${CHOREO_MCP_VERSION:-}" ]]; then
      cmd+=(--version "${CHOREO_MCP_VERSION}")
    fi
    ;;
  git)
    GIT_URL="${CHOREO_MCP_GIT_URL:-https://github.com/underpass-ai/underpass-choreographer}"
    BRANCH="${CHOREO_MCP_BRANCH:-}"
    TAG="${CHOREO_MCP_TAG:-}"
    REV="${CHOREO_MCP_REV:-}"

    selected_refs=0
    [[ -n "${BRANCH}" ]] && selected_refs=$((selected_refs + 1))
    [[ -n "${TAG}" ]] && selected_refs=$((selected_refs + 1))
    [[ -n "${REV}" ]] && selected_refs=$((selected_refs + 1))

    if [[ "${selected_refs}" -gt 1 ]]; then
      echo "set only one of CHOREO_MCP_BRANCH, CHOREO_MCP_TAG, or CHOREO_MCP_REV" >&2
      exit 2
    fi

    cmd=(cargo install --git "${GIT_URL}" choreo-mcp --locked --force)

    if [[ -n "${BRANCH}" ]]; then
      cmd+=(--branch "${BRANCH}")
    elif [[ -n "${TAG}" ]]; then
      cmd+=(--tag "${TAG}")
    elif [[ -n "${REV}" ]]; then
      cmd+=(--rev "${REV}")
    fi
    ;;
  *)
    echo "unknown CHOREO_MCP_INSTALL_MODE: ${MODE} (expected: registry, git)" >&2
    exit 2
    ;;
esac

if [[ -n "${CARGO_INSTALL_ROOT:-}" ]]; then
  cmd+=(--root "${CARGO_INSTALL_ROOT}")
fi

"${cmd[@]}"
