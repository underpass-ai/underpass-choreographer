# Compose E2E

`make e2e-compose` is the repository-owned end-to-end proof for the
Choreographer stack. It runs the public gRPC API, core NATS events, the
Runtime executor adapter, JSON-Schema output contracts, and
provider-shaped agents without external credentials.

Use it when changing user-visible coordination behavior, gRPC mapping,
NATS envelopes, executor wiring, output validation, or provider adapter
composition.

## Quickstart

Prerequisites:

- Docker with the Compose plugin, Podman with compose support, or
  `podman-compose`.
- Enough local resources to build the Choreographer image plus the two
  test sidecars.

Run:

```sh
make e2e-compose
```

The wrapper auto-detects `docker compose`, `podman compose`, then
`podman-compose`. To force a runtime:

```sh
CONTAINER_RUNTIME=docker make e2e-compose
CONTAINER_RUNTIME=podman make e2e-compose
CONTAINER_RUNTIME=podman-compose make e2e-compose
```

If `podman compose` delegates to Docker Compose and cannot find the
rootless Podman socket, either start `podman.socket` for the user
session or force `CONTAINER_RUNTIME=podman-compose`.

## Scenario Selection

The runner reads `CHOREO_E2E_SCENARIOS`. The default is `compose`, which
keeps `make e2e-compose` on the full 1-9 suite. To run a subset:

```sh
CHOREO_E2E_SCENARIOS=cluster-connectivity make e2e-compose
CHOREO_E2E_SCENARIOS=structured-output make e2e-compose
CHOREO_E2E_SCENARIOS=ceremony-vllm make e2e-compose
CHOREO_E2E_SCENARIOS=1-4,8 make e2e-compose
```

Supported selectors:

| Selector | Scenarios | Intended Use |
|---|---:|---|
| `compose` / `all` | 1-9 | Full repo-owned compose proof. |
| `cluster-connectivity` | 1-4 | gRPC, seeded council, delete-idempotence, NATS trigger/envelope smoke. |
| `runtime-stub` | 5 | Runtime executor adapter against `stub-runtime`. |
| `structured-output` | 6-9 | Strict schema rejection plus positive Report contracts through `stub-llm`. |
| `vllm-real` / `council-vllm` / `vllm-real-multi-agent` | 10 | Council deliberation through `kind=vllm`; compose uses `stub-llm`, Kubernetes uses a real vLLM endpoint. |
| `ceremony` / `ceremony-diagram` | 11 | YAML ceremony execution and Mermaid trace with deterministic agents. |
| `ceremony-vllm` / `gemma-ceremony` | 12 | YAML ceremony execution through `kind=vllm`; compose uses `stub-llm`, Kubernetes uses the configured real vLLM/Gemma endpoint. |
| `daily-standup` / `daily` / `standup` | 13 | Daily standup ceremony — multi-step, multi-agent panels through `kind=vllm`. |
| `technical-debate` / `debate` | 14 | Technical debate ceremony. |
| `sprint-planning` / `planning` | 15 | Sprint planning ceremony. |
| `speaker-talk-qa` / `speaker-qa` | 16 | Speaker talk + Q&A ceremony. |
| `1`, `scenario-5`, `s8`, `1-4` | selected numbers | Targeted debugging. |

The script writes compose logs to `tests/e2e/compose.log` during
cleanup. A passing run exits `0` after the `e2e-runner` prints
`E2E scenarios passed`.

## New-User Validated Result

A new user can obtain a validated result without reading Rust by
running only:

```sh
make e2e-compose
```

The validated-result path is scenario 8. It registers the canonical
Report JSON Schema, registers an `openai`-kind agent pointed at
`stub-llm`, creates a `report` council, and calls
`RunCouncilDecision` in `STRICT` mode. The expected success signals are:

```text
scenario 8: structured-output Report contract passes against a stub OpenAI-shaped agent
RunCouncilDecision succeeded with Report-shaped winner ... candidates_passed=1 candidates_total=1
E2E scenarios passed
```

Scenario 9 repeats the same positive Report-contract path through the
`vllm` adapter shape. These scenarios require no external credentials;
they prove the product-owned path with deterministic local fixtures, not
a real provider deployment.

Implementation entry points:

- [`../../Makefile`](../../Makefile) — `e2e-compose` target.
- [`../../scripts/ci/e2e-compose.sh`](../../scripts/ci/e2e-compose.sh)
  — compose runtime detection, cleanup, and log capture.
- [`../../tests/e2e/docker-compose.e2e.yaml`](../../tests/e2e/docker-compose.e2e.yaml)
  — stack definition.
- [`../../crates/choreo-e2e-runner/src/main.rs`](../../crates/choreo-e2e-runner/src/main.rs)
  — runner entrypoint and scenario dispatch.
- [`../../crates/choreo-e2e-runner/src/scenario_selection.rs`](../../crates/choreo-e2e-runner/src/scenario_selection.rs)
  — selector parsing and scenario groups.
- [`../../crates/choreo-e2e-runner/src/scenarios/`](../../crates/choreo-e2e-runner/src/scenarios/)
  — the scenario assertions, split by surface.

## Stack

The compose file starts five meaningful services:

| Service | Purpose |
|---|---|
| `nats` | Core NATS broker for inbound triggers and outbound events. The image starts with `-js`, but the adapter uses plain core NATS pub/sub semantics. |
| `stub-runtime` | Minimal `underpass.runtime.v1` gRPC peer for `RuntimeExecutor`. |
| `stub-llm` | OpenAI-compatible HTTP peer returning deterministic Report-shaped JSON. |
| `choreographer` | Service under test, seeded with one `triage` council and configured for NATS + Runtime executor + OpenAI/vLLM provider kinds. |
| `e2e-runner` | Drives assertions through public gRPC and NATS surfaces only. |

The runner uses:

- `CHOREOGRAPHER_ENDPOINT=http://choreographer:50055`
- `CHOREO_SEED_SPECIALTY=triage`
- `CHOREO_REPORT_SCHEMA_PATH=/etc/choreo/report.schema.json`

## Scenarios

| # | Surface | What It Proves |
|---|---|---|
| 1 | `ListCouncils` | The service booted and seeded the configured `triage` council. |
| 2 | `Deliberate` | A task against the seeded council returns ranked results and a winner. |
| 3 | `DeleteCouncil` | Deleting a missing specialty is non-destructive and returns `deleted=false`. |
| 4 | NATS trigger -> `DeliberationCompleted` | `correlation_id` and `causation_id` propagate from inbound trigger to outbound event. |
| 5 | `Orchestrate` -> `RuntimeExecutor` -> `stub-runtime` | A winning proposal is handed to the Runtime executor and returns the stub invocation id. |
| 6 | `Orchestrate` with strict JSON Schema | Free-form `NoopAgent` output is rejected deterministically and publishes `TaskFailed` with `error_kind=deliberation.no_valid_proposal`. |
| 7 | `Deliberate` with `ExternalContextBundle` | The caller-supplied context bundle id round-trips to the outbound `DeliberationCompleted` envelope. |
| 8 | `RunCouncilDecision` + OpenAI-shaped agent | A strict Report contract passes against an `openai` agent pointed at `stub-llm`; the winner validates against the Report schema. |
| 9 | `RunCouncilDecision` + vLLM-shaped agent | The same strict Report path passes through a `vllm` agent pointed at `stub-llm`, proving the OpenAI-compatible vLLM adapter shape. |

The scenarios are intentionally additive. Scenario 6 proves the
negative structured-output path; scenarios 8 and 9 prove the positive
structured-output path with provider-shaped agents.

## Stub Runtime

`stub-runtime` is a small gRPC server built from
[`../../crates/choreo-e2e-runner/src/bin/stub_runtime.rs`](../../crates/choreo-e2e-runner/src/bin/stub_runtime.rs).
It implements the Runtime services needed by `RuntimeExecutor`:

- `CreateSession` returns `stub-session-1`.
- `InvokeTool` returns `stub-invocation-1` with status `SUCCEEDED`.
- `CloseSession` returns `closed=true`.

The compose service listens on `0.0.0.0:50053` through
`STUB_RUNTIME_GRPC_ADDR`. Choreographer reaches it through:

```text
CHOREO_EXECUTOR_KIND=runtime
CHOREO_RUNTIME_GRPC_ENDPOINT=http://stub-runtime:50053
```

Scenario 5 sets `runtime.tool_name=stub.echo`. That tool name is a
test fixture. It is not a claim that a real `underpass-runtime`
deployment contains `stub.echo`.

## Stub LLM

`stub-llm` is an OpenAI-compatible HTTP server built from
[`../../crates/choreo-e2e-runner/src/bin/stub_llm.rs`](../../crates/choreo-e2e-runner/src/bin/stub_llm.rs).
It exposes:

- `POST /v1/chat/completions`
- `GET /health`

The chat-completions route always returns one deterministic assistant
message. Its `choices[0].message.content` is a JSON string that
satisfies the canonical Report schema.

The listener is controlled by `STUB_LLM_LISTEN`; compose sets it to
`0.0.0.0:8000`. The sidecar ignores bearer tokens and does not call a
real model. It exists only to prove Choreographer's provider-shaped
adapter path and structured-output validation without external
credentials.

## Report Contract

Scenarios 8 and 9 use
[`../../api/examples/output-contracts/report.schema.json`](../../api/examples/output-contracts/report.schema.json).
The runner image copies it to `/etc/choreo/report.schema.json`, then
registers it with `RegisterContract`.

The schema requires:

- `report_id`
- `executive_summary`
- at least one entry in `findings`
- at least one entry in `recommended_actions`

It is generic by design. Application identifiers and domain vocabulary
belong in `Task.attributes`, `ExternalContextBundle.metadata`, or the
calling product, not in the Choreographer core.

## Provider Shapes

Compose enables both provider kinds at boot:

```text
CHOREO_OPENAI_API_KEY=stub-key-not-used
CHOREO_VLLM_MODEL=stub-report-vllm-v1
CHOREO_VLLM_ENDPOINT=http://stub-llm:8000
```

Scenario 8 registers an `openai` agent with:

```text
provider.endpoint=http://stub-llm:8000
provider.model=stub-report-v1
provider.api_key=stub-key-not-used
```

Scenario 9 registers a `vllm` agent with:

```text
provider.endpoint=http://stub-llm:8000
provider.model=stub-report-vllm-v1
```

Both adapters speak the OpenAI Chat Completions wire shape, so one
sidecar can validate both paths. This proves Choreographer's adapter
contract and registration flow. It does not prove latency,
authentication, model behavior, or network policy for a real external
provider.

For a real vLLM endpoint, use the operator-run flow:

```sh
make e2e-provider-vllm
```

For a full Choreographer council plus YAML-mounted ceremony against real
vLLM, use:

```sh
make e2e-council-vllm
```

That path uses the gRPC E2E runner. The MCP parity path uses the same
contract and assertions but enters through the `choreo-mcp` stdio
adapter:

```sh
CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055 \
CHOREO_VLLM_ENDPOINT=https://vllm.example.com \
CHOREO_VLLM_MODEL=google/gemma-4-31B-it \
  make e2e-mcp-council-vllm
```

Use it when the claim being tested is "MCP can drive the same
multi-agent ceremony as gRPC", not just "the vLLM adapter can reach a
provider".

## When To Use Which E2E

| Command | Use For | External Dependencies |
|---|---|---|
| `make e2e-compose` | Repo-owned stack proof with deterministic fixtures. | Local container runtime only. |
| `make e2e-provider-vllm` | Adapter-level validation against a real vLLM endpoint. | Kubernetes access plus real vLLM endpoint configuration. |
| `make e2e-council-vllm` | gRPC council validation and YAML ceremony execution against multiple real vLLM agents. | Deployed Choreographer with `kind=vllm`, plus real vLLM endpoint configuration. |
| `make e2e-mcp-council-vllm` | MCP parity validation for the same multi-agent vLLM ceremony. | Deployed Choreographer reachable from `choreo-mcp`, plus real vLLM endpoint configuration. |
| Kubernetes smoke job | Connectivity smoke after Helm install. | Cluster, deployed Choreographer, and matching runtime/provider fixtures if running compose-shaped scenarios. |

Do not present `make e2e-compose` as proof of real provider
performance. It proves that Choreographer's public surfaces and
provider-shaped wiring are coherent under deterministic test fixtures.

## Troubleshooting

- **No compose runtime found**: install Docker Compose, Podman compose,
  or `podman-compose`, or set `CONTAINER_RUNTIME` explicitly.
- **Scenario 5 fails against a real cluster**: the compose runner
  expects `stub.echo`; use the compose stack or provide an equivalent
  Runtime fixture.
- **Scenarios 8 or 9 fail with provider kind unsupported**: confirm the
  Choreographer binary was built with the OpenAI/vLLM feature flags and
  booted with the provider env vars needed by `DispatchingAgentFactory`.
- **Need detailed logs**: inspect `tests/e2e/compose.log` after the
  wrapper exits.
