# choreo-mcp

Hand-rolled stdio MCP (Model Context Protocol) adapter that exposes
the Underpass Choreographer gRPC API to coding agents.

End-user installation, configuration snippets for Codex CLI / Claude
Desktop, and the env-var reference live in
[`docs/operations/mcp-stdio.md`](../../docs/operations/mcp-stdio.md).
This README is the developer-oriented twin: it covers running the
adapter from a checkout, the test surface, and the design choices
worth knowing when you touch the code.

## Run from a checkout

```bash
# fixture mode — no choreographer needed
CHOREO_MCP_BACKEND=fixture cargo run -p choreo-mcp --locked

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
| `choreo_run_council_decision`     | `RunCouncilDecision`              | validated council decision |
| `choreo_register_contract`        | `RegisterContract`                | contract registry |
| `choreo_list_contracts`           | `ListContracts`                   | contract registry |
| `choreo_delete_contract`          | `DeleteContract`                  | contract registry |
| `choreo_get_status`               | `GetStatus`                       | observability |
| `choreo_get_metrics`              | `GetMetrics`                      | observability |

Mappings live in `src/grpc/{json_to_proto.rs,proto_to_json.rs}` —
**hand-written field-by-field**. A new proto field is a one-PR
change: add the schema key in `protocol.rs`, add the request mapper
in `json_to_proto.rs`, add the response mapper in `proto_to_json.rs`.

## Design choices

1. **No MCP SDK.** Tokio + serde_json + tonic + a handful of small
   helpers. The wire protocol stays in lock-step with the proto
   contract because the team owns every byte.

2. **Backend trait as the single seam.** `ChoreoMcpToolBackend` has
   exactly one impl in production (`GrpcChoreoMcpBackend`) and one
   for tests (`FixtureChoreoMcpBackend`). Selection is env-driven,
   fail-fast — there is no silent fallback to fixtures when the
   gRPC endpoint is misconfigured.

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

Live gRPC backend integration tests against a real choreographer
ship under
[`crates/choreo-tests-integration`](../choreo-tests-integration/)
(forthcoming in the smoke/integration slice).

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
