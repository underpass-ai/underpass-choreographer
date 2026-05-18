#!/usr/bin/env bash
#
# Publish dry-run gate for the two crates that ship to crates.io:
#   - `choreo-mcp-proto` (vendored proto crate)
#   - `choreo-mcp`       (stdio MCP adapter)
#
# Catches packaging regressions (missing metadata, missing files,
# accidental path-only deps) before the publish-distribution
# workflow tries to push to crates.io.
#
# `choreo-mcp-proto` uses the full `cargo publish --dry-run` flow:
# compiles the staged tarball as a stand-alone crate.
#
# `choreo-mcp` does NOT — it depends on `choreo-mcp-proto` which is
# not yet on the registry, so cargo's pre-publish verify step would
# always fail. The real publish-distribution workflow serializes the
# two jobs (proto first, then mcp with a 30s wait for index
# propagation), which is the only way registry-order deps can land.
# Here we run `cargo package -l` to validate the file list + the
# `Cargo.toml` metadata; that catches missing keys / accidental
# excludes without the registry round-trip.
#
# No CARGO_REGISTRY_TOKEN required — neither command uploads.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

echo "::group::cargo publish --dry-run -p choreo-mcp-proto"
cargo publish --dry-run -p choreo-mcp-proto
echo "::endgroup::"

echo "::group::cargo package -l -p choreo-mcp"
cargo package --list -p choreo-mcp
echo "::endgroup::"
