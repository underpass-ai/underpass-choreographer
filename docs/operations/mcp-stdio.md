# MCP Stdio Adapter

Status: installable stdio adapter for the Underpass Choreographer gRPC API.

The repo ships a stdio MCP server in
[`crates/choreo-mcp`](../../crates/choreo-mcp). It exposes every RPC of
`underpass.choreo.v1` as an MCP tool, so coding agents (Codex CLI,
Claude Desktop) can drive the choreographer without re-implementing
gRPC.

Companion docs:

- [Codex CLI configuration](./mcp/codex.md)
- [Claude Desktop configuration](./mcp/claude-desktop.md)

## Tools

The 16 MCP tools are 1:1 with the choreographer's gRPC service:

| MCP tool                          | gRPC RPC                              | Purpose |
|-----------------------------------|---------------------------------------|---------|
| `choreo_deliberate`               | `Deliberate`                          | Run a council deliberation; returns ranked proposals. |
| `choreo_stream_deliberation`      | `StreamDeliberation`                  | Same as above but every phase-transition frame buffered into one response (stdio is sync). |
| `choreo_get_deliberation_result`  | `GetDeliberationResult`               | Fetch a previously-executed deliberation by task id. |
| `choreo_orchestrate`              | `Orchestrate`                         | Deliberate AND execute the winner through the wired executor. |
| `choreo_create_council`           | `CreateCouncil`                       | Create / replace a council for a specialty. |
| `choreo_list_councils`            | `ListCouncils`                        | Enumerate registered councils. |
| `choreo_delete_council`           | `DeleteCouncil`                       | Idempotent delete. |
| `choreo_register_agent`           | `RegisterAgent`                       | Add an agent to a council (`noop` / `anthropic` / `openai` / `vllm`). |
| `choreo_unregister_agent`         | `UnregisterAgent`                     | Remove an agent. |
| `choreo_process_trigger_event`    | `ProcessTriggerEvent`                 | Submit a domain event; fans out to deliberations. |
| `choreo_get_status`               | `GetStatus`                           | Service health, version, uptime, optional stats. |
| `choreo_get_metrics`              | `GetMetrics`                          | Statistics snapshot. |
| `choreo_run_council_decision`     | `RunCouncilDecision`                  | Epic 9: run a council against a registered output contract; returns the validated winner plus per-candidate breakdown. |
| `choreo_register_contract`        | `RegisterContract`                    | Epic 9: register an `OutputContract` in the contract registry. |
| `choreo_list_contracts`           | `ListContracts`                       | Epic 9: enumerate registered contracts. |
| `choreo_delete_contract`          | `DeleteContract`                      | Epic 9: idempotent contract delete. |

The choreographer API is **respected at 100%** — every proto field has
an explicit JSON key in both the tool input schema and the response.
No flattening, no silent drops. Enums (e.g. `DeliberationPhase`) map
to stable string labels (`DELIBERATION_PHASE_PROPOSING`, …).

## Modes

Backend selection is driven by `CHOREO_MCP_BACKEND`:

- **`grpc`** (default) — talks to a real choreographer. The endpoint
  env var is mandatory; the binary exits with code 2 if it is missing.
- **`fixture`** — returns canned responses for every tool. Useful for
  client wiring, demos, and tool-choice validation **without** a
  running choreographer.

```bash
CHOREO_MCP_BACKEND=fixture cargo run -p choreo-mcp --locked
```

## Installation

For users outside the repo, install as a Cargo binary from crates.io:

```bash
cargo install choreo-mcp --locked
```

The first registry release lands the next time a `v*` tag is pushed
through `publish-distribution.yml`. The companion crate
`choreo-mcp-proto` (vendored proto types) publishes immediately
before; both are gated on the `compose-smoke` release smoke test.

The repo helper wraps the same path. Default mode is `registry`;
pass `CHOREO_MCP_INSTALL_MODE=git` for the dev fallback against the
source repo (useful for validating an unreleased change):

```bash
bash scripts/mcp/install-choreo-mcp.sh

# dev / pinned-ref mode:
CHOREO_MCP_INSTALL_MODE=git CHOREO_MCP_TAG=v0.1.0 bash scripts/mcp/install-choreo-mcp.sh
CHOREO_MCP_INSTALL_MODE=git CHOREO_MCP_REV=<git-sha> bash scripts/mcp/install-choreo-mcp.sh
```

After install, the adapter is just `choreo-mcp` on PATH:

```bash
CHOREO_MCP_GRPC_ENDPOINT=https://choreographer.example.com choreo-mcp
```

## Live gRPC mode

Plain (no TLS):

```bash
CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055 \
  cargo run -p choreo-mcp --locked
```

`https://` endpoints auto-enable server TLS using system / webpki roots:

```bash
CHOREO_MCP_GRPC_ENDPOINT=https://choreographer.example.com \
  cargo run -p choreo-mcp --locked
```

Private CAs and direct mTLS are explicit:

```bash
CHOREO_MCP_GRPC_ENDPOINT=https://choreographer.underpass.svc:50055 \
CHOREO_MCP_GRPC_TLS_MODE=mutual \
CHOREO_MCP_GRPC_TLS_CA_PATH=/var/run/choreo-tls/ca.crt \
CHOREO_MCP_GRPC_TLS_CERT_PATH=/var/run/choreo-tls/tls.crt \
CHOREO_MCP_GRPC_TLS_KEY_PATH=/var/run/choreo-tls/tls.key \
CHOREO_MCP_GRPC_TLS_DOMAIN_NAME=choreographer-grpc \
  cargo run -p choreo-mcp --locked
```

### Env var reference

| Var                              | Purpose                                                                  |
|----------------------------------|--------------------------------------------------------------------------|
| `CHOREO_MCP_BACKEND`             | `grpc` (default) or `fixture`.                                           |
| `CHOREO_MCP_GRPC_ENDPOINT`       | URL the MCP connects to. Required when `BACKEND=grpc`.                   |
| `CHOREO_MCP_GRPC_TLS_MODE`       | `disabled` / `server` / `mutual`. Auto-derived when omitted.             |
| `CHOREO_MCP_GRPC_TLS_CA_PATH`    | PEM CA bundle. Implies `server` mode when set.                           |
| `CHOREO_MCP_GRPC_TLS_CERT_PATH`  | Client cert PEM (mutual). Implies `mutual` mode when set.                |
| `CHOREO_MCP_GRPC_TLS_KEY_PATH`   | Client key PEM (mutual). Implies `mutual` mode when set.                 |
| `CHOREO_MCP_GRPC_TLS_DOMAIN_NAME`| TLS SNI/domain override when cert CN/SAN differs from the URL host.      |

`RUST_LOG=choreo_mcp=debug` enables structured per-tool-call tracing
on stderr (stdout is reserved for JSON-RPC).

## Smoke test

```bash
# Fixture mode (no choreographer needed)
CHOREO_MCP_BACKEND=fixture \
CHOREO_MCP_BIN=choreo-mcp \
  bash scripts/mcp/choreo-stdio-smoke.sh

# Live mode
CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055 \
CHOREO_MCP_BIN=choreo-mcp \
  bash scripts/mcp/choreo-stdio-smoke.sh
```

The script issues one `tools/call`, asserts `"jsonrpc":"2.0"` is
present, `"isError":true` is absent, and an expected marker is
present.

## Manual JSON-RPC check

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | CHOREO_MCP_BACKEND=fixture cargo run -q -p choreo-mcp --locked
```

Expected:

- the server writes one JSON-RPC response per input line;
- fixture mode returns deterministic responses only when explicitly
  selected;
- live mode returns MCP tool errors instead of crashing if the gRPC
  endpoint is unreachable.

## Client configuration

The two officially-supported clients have dedicated guides:

- [Codex CLI](./mcp/codex.md) — TOML config + `codex mcp add` form.
- [Claude Desktop](./mcp/claude-desktop.md) — `claude_desktop_config.json`
  with per-OS paths.

Both share the same env-driven backend selection; the only difference
is the file location the client expects.

## Streaming caveat

`choreo_stream_deliberation` corresponds to `StreamDeliberation`, a
server-streaming RPC. MCP stdio is synchronous request/response, so
the adapter buffers the entire stream into a single response:

```json
{
  "task_id": "...",
  "frames": [ /* every DeliberationUpdate in order */ ],
  "winner": { /* extracted from the last result-typed frame */ }
}
```

There is no `progress`-style live emission. If your agent needs
incremental frames, call gRPC directly.
