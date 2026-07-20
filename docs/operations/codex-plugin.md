# Codex plugin acceptance ladder

The `plugins/choreographer` bundle packages the embedded ceremony engine as a
local MCP stdio server for Codex. Acceptance is cumulative: a later level is
not considered valid unless every earlier level remains green.

## Levels

| Level | Boundary | Evidence |
|---|---|---|
| 1 | Embedded library | `cargo test -p choreo-embedded --locked` |
| 2 | MCP backend in process | The embedded server advertises only tools it can execute and completes a real ceremony. |
| 3 | MCP binary over stdio | A child process completes `initialize`, `tools/list`, and `tools/call`. |
| 4 | Dependency isolation | The embedded binary tree contains no gRPC, protobuf, NATS, or SQL client. |
| 5 | Plugin bundle | The manifest validates and the bundle launcher completes the same ceremony. |
| 6 | Codex installation | Codex installs the local marketplace entry and discovers the bundled MCP server in a new thread. |

Run levels 2–4 directly:

```bash
cargo test -p choreo-mcp --all-targets \
  --no-default-features --features embedded --locked
bash scripts/ci/embedded-dependency-boundary.sh
```

Build and execute level 5:

```bash
bash scripts/ci/choreographer-plugin-smoke.sh
```

The smoke builds an isolated release binary, places it at
`plugins/choreographer/bin/choreo-mcp`, starts it through the plugin's own
launcher, and verifies the three MCP responses. The binary is ignored by Git;
source, manifest, skill, launcher, and tests remain reviewable.

## Current capability

The installed plugin exposes `choreo_run_ceremony`. It accepts a declarative
YAML definition plus optional context and returns the final state, step trace,
and Mermaid sequence.

This first slice supports one-shot ceremonies only. Incremental commands for a
pause, later human authorization, and continuation remain a subsequent
acceptance level; the bundled skill explicitly refuses to imply that behavior
already exists.

## Installation boundary

The repo-local bundle is installed only after levels 1–5 pass. Installation
copies the validated bundle to a personal local plugin source, adds the
personal marketplace entry, and runs `codex plugin add`. Codex loads new plugin
skills and MCP tools at the start of a new thread, so the final functional
check intentionally happens there.
