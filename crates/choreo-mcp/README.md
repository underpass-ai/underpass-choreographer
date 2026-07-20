# choreo-mcp

Hand-rolled stdio MCP (Model Context Protocol) adapter that exposes
Underpass Choreographer capabilities to coding agents. It can connect
to the deployable gRPC service or run the ceremony engine in process.

End-user installation, configuration snippets for Codex CLI / Claude
Desktop, and the env-var reference live in
[`docs/operations/mcp-stdio.md`](../../docs/operations/mcp-stdio.md).
This README is the developer-oriented twin: it covers running the
adapter from a checkout, the test surface, and the design choices
worth knowing when you touch the code.

## Install (registry)

```bash
cargo install choreo-mcp --locked
```

This pulls `choreo-mcp` + the vendored `choreo-mcp-proto` from
crates.io. The dev fallback against this repo's source tree is
`CHOREO_MCP_INSTALL_MODE=git bash scripts/mcp/install-choreo-mcp.sh`.

## Run from a checkout

```bash
# fixture mode — no choreographer needed
CHOREO_MCP_BACKEND=fixture cargo run -p choreo-mcp --locked

# embedded ceremony mode — real engine, no external service
CHOREO_MCP_BACKEND=embedded \
  cargo run -p choreo-mcp --no-default-features --features embedded --locked

# live mode against a local choreographer
CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055 \
  cargo run -p choreo-mcp --locked

# live mode over mTLS
CHOREO_MCP_GRPC_ENDPOINT=https://choreographer.example.com \
CHOREO_MCP_GRPC_TLS_MODE=mutual \
CHOREO_MCP_GRPC_TLS_CA_PATH=/var/run/choreo-tls/ca.crt \
CHOREO_MCP_GRPC_TLS_CERT_PATH=/var/run/choreo-tls/tls.crt \
CHOREO_MCP_GRPC_TLS_KEY_PATH=/var/run/choreo-tls/tls.key \
  cargo run -p choreo-mcp --locked
```

The binary reads one JSON-RPC line at a time from stdin and writes
one response per non-notification message to stdout. Stderr is
structured JSON tracing (level controlled by `RUST_LOG`,
default `choreo_mcp=info`).

## Manual JSON-RPC walkthrough

```bash
printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | CHOREO_MCP_BACKEND=fixture cargo run -q -p choreo-mcp --locked
```

The `initialize` reply carries `serverInfo` and adapter-side metadata
(`backend`, `grpc_tls`) so the client can record what it negotiated
without an extra round trip.

## Tool dispatch table

| MCP tool                          | gRPC RPC                          | Notes |
|-----------------------------------|-----------------------------------|-------|
| `choreo_deliberate`               | `Deliberate`                      | sync |
| `choreo_stream_deliberation`      | `StreamDeliberation`              | buffered: stream collected into one response |
| `choreo_get_deliberation_result`  | `GetDeliberationResult`           | sync |
| `choreo_orchestrate`              | `Orchestrate`                     | sync |
| `choreo_create_council`           | `CreateCouncil`                   | control plane |
| `choreo_list_councils`            | `ListCouncils`                    | read |
| `choreo_delete_council`           | `DeleteCouncil`                   | idempotent control plane |
| `choreo_register_agent`           | `RegisterAgent`                   | control plane |
| `choreo_unregister_agent`         | `UnregisterAgent`                 | control plane |
| `choreo_process_trigger_event`    | `ProcessTriggerEvent`             | event ingest |
| `choreo_run_council_decision`     | `RunCouncilDecision`              | sync; structured-output decision |
| `choreo_register_contract`        | `RegisterContract`                | control plane |
| `choreo_list_contracts`           | `ListContracts`                   | read |
| `choreo_delete_contract`          | `DeleteContract`                  | idempotent control plane |
| `choreo_run_ceremony`             | `RunCeremony`                     | sync; runs a YAML ceremony to a terminal state |
| `choreo_get_status`               | `GetStatus`                       | observability |
| `choreo_get_metrics`              | `GetMetrics`                      | observability |

The embedded backend also exposes persistent, incremental ceremony controls
that intentionally have no gRPC mapping:

| MCP tool | Purpose |
|----------|---------|
| `choreo_start_ceremony` | Mount YAML and start without advancing. |
| `choreo_run_ceremony_step` | Execute and persist one step. |
| `choreo_approve_ceremony_guard` | Record an explicit human approval for a currently relevant human guard. |
| `choreo_apply_ceremony_transition` | Apply one enabled transition. |
| `choreo_get_ceremony_instance` | Inspect steps, transitions, and blocking human guards. |

These calls allow the host to pause between actions. Human guard approval is
never inferred by the server; the client must obtain the person's decision
before it invokes the approval tool.

Mappings live in `src/grpc/{json_to_proto.rs,proto_to_json.rs}` —
**hand-written field-by-field**. A new proto field is a one-PR
change: add the schema key in `protocol.rs`, add the request mapper
in `json_to_proto.rs`, add the response mapper in `proto_to_json.rs`.

> `choreo-mcp` builds against `choreo-mcp-proto`, a **vendored copy** of
> `underpass.choreo.v1` kept byte-identical to `crates/choreo-proto/proto`
> so this crate can publish independently. The two `.proto` files must be
> kept in sync by hand — there is no automated cross-copy diff. (The
> `tools_catalog_is_derived_one_for_one_from_grpc_service` test only
> enforces the tool↔RPC 1:1 mapping against this crate's own vendored
> copy.)

## Design choices

1. **No MCP SDK.** Tokio + serde_json + tonic + a handful of small
   helpers. The wire protocol stays in lock-step with the proto
   contract because the team owns every byte.

2. **Backend trait as the single seam.** `ChoreoMcpToolBackend` has
   live gRPC, embedded ceremony, and deterministic fixture adapters.
   Each backend filters `tools/list` to operations it can honor.
   Selection is env-driven and fail-fast — there is no silent fallback
   when the requested backend is misconfigured or not compiled.

3. **JSON-RPC stays sync.** MCP stdio is request/response; the
   adapter does not implement server progress notifications.
   `StreamDeliberation` buffers the full server stream into a
   single response with a `frames` array and a `winner` field
   extracted from the last `result`-typed frame.

4. **Field-for-field mapping.** No `serde_json::to_value(proto)`
   shortcuts. Enums collapse to stable string labels. A new proto
   field that lands without an MCP mapper update is a review-time
   miss, not a silent drop.

5. **Error result shape.** Tool errors come back as `isError: true`
   inside the tool result, per MCP spec — **not** as JSON-RPC
   errors. JSON-RPC `error` codes are reserved for protocol-level
   issues (parse error, missing params, unsupported method).

6. **Privacy-safe telemetry.** Tool error messages are
   SHA-256-prefix hashed before they go into metrics. The full
   message reaches the caller through the tool result text (where
   the agent wanted it) and the structured trace event (where the
   operator opted into debug logging).

## Tests

```bash
cargo test -p choreo-mcp --locked
```

- `src/protocol.rs::tests` — initialize / tools/list shape, every
  tool definition is present, success/error envelopes.
- `src/server.rs::tests` — JSON-RPC dispatch paths and error codes.
- `src/backend.rs::tests` — TLS mode parsing + URL upgrade.
- `src/fixture.rs::tests` — every tool has a canned fixture and the
  fixture envelope matches the live response shape.
- `src/observability.rs::tests` — error-kind labels and the recursive
  size approximator used in trace events.

### Real-kernel container integration test

A separate `tests/real_kernel.rs` boots the published
`ghcr.io/underpass-ai/underpass-choreographer:latest` image via
testcontainers, spawns this crate's binary against its mapped gRPC
port, and exercises `initialize`, `tools/list` (asserts the full 17-
tool catalog), and `tools/call` on the four simplest read-only RPCs.
The test is gated by the `container-tests` Cargo feature so the
default workspace `cargo test --workspace` stays fast + network-free.

```bash
cargo test -p choreo-mcp --features container-tests
```

The default `cargo test --workspace` does NOT compile testcontainers
or pull the image.

## Common pitfalls

- **Stdout pollution.** Anything written to stdout that is not a
  JSON-RPC response will desync the client. Use `tracing` (which
  writes to stderr); avoid `println!`.
- **Blocking the loop.** The dispatcher awaits each tool call
  serially. A long-running call blocks subsequent inputs — by
  design, since most agents wait on the previous response before
  emitting the next request.
- **Env var typos.** TLS auto-detection is permissive: setting
  `CHOREO_MCP_GRPC_TLS_CERT_PATH` alone (no key) is silently
  ignored, falling back to `server`. The startup log emits the
  active `grpc_tls` mode — check it when debugging.
