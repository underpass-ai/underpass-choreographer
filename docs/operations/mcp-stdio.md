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

## Quickstart — fixture mode

After installing `choreo-mcp`, the fastest client-wiring check needs
no running Choreographer and no gRPC endpoint:

```bash
CHOREO_MCP_BACKEND=fixture choreo-mcp
```

That starts the stdio MCP server and waits for JSON-RPC on stdin. For
a terminal smoke that exits immediately:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | CHOREO_MCP_BACKEND=fixture choreo-mcp
```

From a checkout, without installing the binary first:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | CHOREO_MCP_BACKEND=fixture cargo run -q -p choreo-mcp --locked
```

Fixture mode returns deterministic canned responses for every tool. It
is for MCP client setup, tool-choice validation, and demos; it is not a
live Choreographer integration test.

## Quickstart — live local gRPC

To test the MCP adapter against a real local Choreographer, use two
terminals.

Terminal 1 starts Choreographer with no external services and seeds one
demo council:

```bash
CHOREO_NATS_ENABLED=false CHOREO_SEED_SPECIALTIES=triage just run
```

If `just` is not installed, use the equivalent Cargo command:

```bash
CHOREO_NATS_ENABLED=false CHOREO_SEED_SPECIALTIES=triage \
  cargo run --locked -p choreo
```

Terminal 2 starts the MCP stdio adapter against the local gRPC endpoint:

```bash
CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055 choreo-mcp
```

For a one-shot terminal smoke from a checkout:

```bash
CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055 \
CHOREO_MCP_BIN=target/debug/choreo-mcp \
  bash scripts/mcp/choreo-stdio-smoke.sh
```

The smoke calls `choreo_list_councils` and expects the seeded `triage`
council. If `choreo-mcp` is already installed on PATH, omit
`CHOREO_MCP_BIN`.

## Tool Call Examples

### CreateCouncil

`choreo_create_council` creates a council for a specialty and asks the
server to seat `num_agents` agents. In live mode those agents must
already be resolvable. The gRPC handler mints ids in the form
`agent-<specialty>-<index>`, so `{"specialty":"triage","num_agents":1}`
expects `agent-triage-0` to exist.

Fixture-mode terminal check:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"choreo_create_council","arguments":{"specialty":"triage","num_agents":1}}}' \
  | CHOREO_MCP_BACKEND=fixture cargo run -q -p choreo-mcp --locked
```

Expected response shape:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "isError": false,
    "structuredContent": {
      "council": {
        "specialty": "triage",
        "num_agents": 1,
        "agents": []
      }
    }
  }
}
```

The fixture response is deterministic and does not mutate state. For a
live local call, first ensure the matching agent exists through seeding
or `choreo_register_agent`, then set
`CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055` instead of
`CHOREO_MCP_BACKEND=fixture`.

### RegisterAgent

`choreo_register_agent` registers an agent descriptor so later calls can
resolve that agent by id. It does not attach the agent to a council by
itself; `CreateCouncil` still controls council membership.

Fixture-mode terminal check:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"choreo_register_agent","arguments":{"specialty":"review","agent":{"agent_id":"agent-review-0","specialty":"review","kind":"noop"},"agent_config":{"label":"local noop reviewer"}}}}' \
  | CHOREO_MCP_BACKEND=fixture cargo run -q -p choreo-mcp --locked
```

Expected response shape:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "isError": false,
    "structuredContent": {
      "agent_id": "agent-fixture-1"
    }
  }
}
```

For a live local call, use `kind: "noop"` when you want a provider-free
agent. Provider-backed kinds such as `openai` or `vllm` require the
corresponding adapter and environment to be configured. Per-agent
factory options belong in top-level `agent_config`; the nested `agent`
object is only the public summary (`agent_id`, `specialty`, `kind`,
optional `attributes`). If the next step is `CreateCouncil`, keep the id
pattern `agent-<specialty>-<index>`; for the example above that means
creating a `review` council with `num_agents: 1`.

### RegisterContract

`choreo_register_contract` stores an `OutputContract` in the contract
registry. Later `RunCouncilDecision` calls reference it by
`contract_id` and validate the council winner against its field rules
and optional embedded JSON Schema.

Fixture-mode terminal check:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"choreo_register_contract","arguments":{"contract":{"contract_id":"contract-review-v1","format":"json_object","fields":{"status":{"required":true,"allowed_string_values":["accepted","needs_changes"]},"summary":{"required":true},"rationale":{"required":false}}}}}}' \
  | CHOREO_MCP_BACKEND=fixture cargo run -q -p choreo-mcp --locked
```

Expected response shape:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "isError": false,
    "structuredContent": {
      "contract_id": "contract-fixture-1"
    }
  }
}
```

For a live local call, keep the returned or requested `contract_id` and
pass it to `choreo_run_council_decision`. `format` is currently
`json_object`. Field rules can require named fields and constrain string
values; for stricter validation, include a `json_schema` string. The
canonical Report-shape example lives at
[`api/examples/output-contracts/report.schema.json`](../../api/examples/output-contracts/report.schema.json).

### RunCouncilDecision

`choreo_run_council_decision` runs a council and validates the winning
proposal against a previously registered contract. The call must include
`contract_id`, `description`, and exactly one selector:
`specialty` or `council_id`.

Fixture-mode terminal check:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"choreo_run_council_decision","arguments":{"specialty":"review","contract_id":"contract-review-v1","description":"Review the candidate change and return status, summary, and rationale.","validation_mode":"VALIDATION_MODE_STRICT"}}}' \
  | CHOREO_MCP_BACKEND=fixture cargo run -q -p choreo-mcp --locked
```

Expected response shape:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "isError": false,
    "structuredContent": {
      "task_id": "task-fixture-1",
      "winner": {
        "rank": 0,
        "proposal": {
          "proposal_id": "proposal-fixture-a",
          "author_agent_id": "agent-fixture-1",
          "content": "fixture answer",
          "metadata": {},
          "revision_count": 0
        },
        "validation": {
          "score": 1.0,
          "reports": [
            {
              "kind": "content-non-empty",
              "passed": true,
              "summary": "ok",
              "details": {}
            }
          ]
        }
      },
      "validation": {
        "passed": true,
        "candidates_passed": 1,
        "candidates_total": 1
      },
      "candidates": [
        {
          "proposal_id": "proposal-fixture-a",
          "author_agent_id": "agent-fixture-1",
          "score": 1.0,
          "reports": [
            {
              "kind": "content-non-empty",
              "passed": true,
              "summary": "ok",
              "details": {}
            }
          ],
          "rank": 0,
          "passed": true,
          "revision_count": 0
        }
      ],
      "duration_ms": 42,
      "validation_mode": "VALIDATION_MODE_STRICT"
    }
  }
}
```

For a live local call, the selected council must exist and the
`contract_id` must already be registered. `VALIDATION_MODE_STRICT`
fails the call when no candidate satisfies the contract; use
`VALIDATION_MODE_WARN` when the caller wants the best-ranked candidate
returned even if validation fails.

### Orchestrate

`choreo_orchestrate` runs the full path: deliberate on the task's
specialty, pick the winning proposal, and pass that winner to the
configured `ExecutorPort`. The call takes a `task` object and optional
opaque `execution_options`.

Fixture-mode terminal check:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"choreo_orchestrate","arguments":{"task":{"task_id":"task-review-orchestrate-1","description":"Review the candidate change and execute the accepted plan.","specialty":"review","constraints":{"rounds":1,"num_agents":1}},"execution_options":{"executor":"noop","trace_label":"mcp-orchestrate-demo"}}}}' \
  | CHOREO_MCP_BACKEND=fixture cargo run -q -p choreo-mcp --locked
```

Expected response shape:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "isError": false,
    "structuredContent": {
      "task_id": "task-fixture-1",
      "execution_id": "exec-fixture-1",
      "duration_ms": 73,
      "winner": {
        "rank": 0,
        "proposal": {
          "proposal_id": "proposal-fixture-a",
          "author_agent_id": "agent-fixture-1",
          "content": "fixture answer",
          "metadata": {},
          "revision_count": 0
        },
        "validation": {
          "score": 1.0,
          "reports": [
            {
              "kind": "content-non-empty",
              "passed": true,
              "summary": "ok",
              "details": {}
            }
          ]
        }
      },
      "candidates": [],
      "metadata": {
        "fixture": true
      }
    }
  }
}
```

For a live local call, `task.specialty` must point to an existing
council. The default local executor is `noop`; set the Runtime executor
environment only when you want the winner sent to an external Runtime
service. `execution_options` is forwarded to the configured executor and
takes precedence over overlapping execution-profile metadata.

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
| `choreo_register_agent`           | `RegisterAgent`                       | Register an agent descriptor (`noop` / `anthropic` / `openai` / `vllm`). |
| `choreo_unregister_agent`         | `UnregisterAgent`                     | Remove an agent. |
| `choreo_process_trigger_event`    | `ProcessTriggerEvent`                 | Submit a domain event; fans out to deliberations. |
| `choreo_run_council_decision`     | `RunCouncilDecision`                  | Run a council against a registered output contract; returns the validated winner plus per-candidate breakdown. |
| `choreo_register_contract`        | `RegisterContract`                    | Register an `OutputContract` in the contract registry. |
| `choreo_list_contracts`           | `ListContracts`                       | Enumerate registered contracts. |
| `choreo_delete_contract`          | `DeleteContract`                      | Idempotent contract delete. |
| `choreo_get_status`               | `GetStatus`                           | Service health, version, uptime, optional stats. |
| `choreo_get_metrics`              | `GetMetrics`                          | Statistics snapshot. |

The choreographer API is **respected at 100%** — every proto field has
an explicit JSON key in both the tool input schema and the response.
No flattening, no silent drops. Enums (e.g. `DeliberationPhase`) map
to stable string labels (`DELIBERATION_PHASE_PROPOSING`, …).

## Modes

Backend selection is driven by `CHOREO_MCP_BACKEND`:

- **`grpc`** (default) — talks to a real choreographer. The endpoint
  env var is mandatory; the binary exits with code 2 if it is missing.
- **`embedded`** — executes the real ceremony engine in process. The isolated
  build exposes `choreo_run_ceremony` and requires no Choreographer service,
  gRPC, protobuf, NATS, or database.
- **`fixture`** — returns canned responses for every tool. Useful for
  client wiring, demos, and tool-choice validation **without** a
  running choreographer.

```bash
CHOREO_MCP_BACKEND=fixture cargo run -p choreo-mcp --locked

CHOREO_MCP_BACKEND=embedded \
  cargo run -p choreo-mcp --no-default-features --features embedded --locked
```

## Installation

For users outside the repo, install as a Cargo binary from crates.io
after the first release has published the package:

```bash
cargo install choreo-mcp --locked
```

The repo helper uses the registry path by default:

```bash
bash scripts/mcp/install-choreo-mcp.sh
```

For unreleased changes, switch the helper to Git mode and pin a ref:

```bash
CHOREO_MCP_INSTALL_MODE=git bash scripts/mcp/install-choreo-mcp.sh

CHOREO_MCP_INSTALL_MODE=git CHOREO_MCP_TAG=v0.1.0 bash scripts/mcp/install-choreo-mcp.sh
CHOREO_MCP_INSTALL_MODE=git CHOREO_MCP_REV=<git-sha> bash scripts/mcp/install-choreo-mcp.sh
```

After install, the adapter is just `choreo-mcp` on PATH:

```bash
CHOREO_MCP_GRPC_ENDPOINT=https://choreographer.example.com choreo-mcp
```

### Distribution model

`choreo-mcp` depends on `choreo-mcp-proto`, a small vendored proto
crate that carries only the public `underpass.choreo.v1` API needed by
the MCP adapter. Release tags publish `choreo-mcp-proto` first, wait
for crates.io index propagation, and then publish `choreo-mcp`.

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
| `CHOREO_MCP_BACKEND`             | `grpc` (default), `embedded`, or `fixture`; the selected backend must be compiled. |
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

## Multi-Agent vLLM E2E

`make e2e-mcp-council-vllm` proves the same real-provider council
ceremony through MCP stdio instead of direct gRPC. It builds
`choreo-mcp` from the checkout when `CHOREO_MCP_BIN` is not set, then
uses `tools/call` requests for:

- `choreo_register_contract`
- `choreo_register_agent`, once per vLLM agent
- `choreo_create_council`
- `choreo_run_council_decision`

The final response must contain multiple candidates, at least one
schema-valid candidate, a schema-valid Report winner, distinct agent
authors, and `revision_count > 0` on the winner and every candidate.

```bash
CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055 \
CHOREO_VLLM_ENDPOINT=https://vllm.example.com \
CHOREO_VLLM_MODEL=google/gemma-4-31B-it \
CHOREO_VLLM_AGENT_COUNT=3 \
  make e2e-mcp-council-vllm
```

Use the same TLS env vars as live mode when the Choreographer endpoint
requires server TLS or mTLS. The Choreographer target must be built
with `agent-vllm` and booted with `CHOREO_VLLM_MODEL` plus
`CHOREO_VLLM_ENDPOINT` so `kind=vllm` is available; the E2E also sends
per-agent `provider.endpoint`, `provider.model`, and
`provider.max_tokens` overrides through MCP.

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
