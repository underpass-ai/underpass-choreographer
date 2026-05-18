# Choreographer Backlog

Snapshot date: 2026-04-25; honest re-audit 2026-05-11; PIR framing
dropped 2026-05-12 (PIR is owned by a separate project — this backlog
tracks Choreographer's own stack-readiness, not any one downstream
consumer).

Companion documents:

- [`stack-gap-analysis.md`](./stack-gap-analysis.md)
- [`operations/mcp-stdio.md`](./operations/mcp-stdio.md) — installable
  stdio MCP adapter UX.

The goal is to keep Choreographer trustworthy as a stack peer:
real execution, real context, structured council outputs, causal
metadata, provider-backed councils, honest transport, TLS, an
agent-facing surface (gRPC + MCP), and reproducible stack E2E.

## Executive summary

As of 2026-05-11 the eight stack-readiness areas resolve as follows:

| # | Area | State |
|---|---|---|
| 1 | real Runtime execution | done (adapter + env-driven wiring) |
| 2 | typed external context input | done (typed `ExternalContextBundle` flowing trigger -> task -> deliberation) |
| 3 | structured, contract-validated council outputs | done (structured-output mode + deterministic `NoValidProposal` failure) |
| 4 | complete causal metadata propagation | done (Epic 5) |
| 5 | provider-backed council materialization | done (`DispatchingAgentFactory` wired with `noop`/`anthropic`/`openai`/`vllm` arms) |
| 6 | honest and durable transport semantics | done (AsyncAPI now declares plain core NATS; JetStream deferred) |
| 7 | real TLS / mTLS posture | done (gRPC server TLS in `none`/`server`/`mutual` modes; chart honest; Runtime client TLS shipped 2026-05-12; handshake-level integration test still deferred) |
| 8 | stack-level end-to-end proofs | done (E2E covers seeded council, deliberation, causal metadata over NATS, `Orchestrate → RuntimeExecutor → stub-runtime`, `ExternalContextBundle` round-trip, and the positive structured-output `RunCouncilDecision` path via the `stub-llm` sidecar; real-provider vLLM council remains `make e2e-provider-vllm`) |

Two surfaces beyond the eight areas:

- **MCP stdio adapter** — `crates/choreo-mcp` ships a hand-rolled
  stdio MCP server that exposes every RPC of `underpass.choreo.v1`
  as a `choreo_*` tool. End-user docs live at
  [`docs/operations/mcp-stdio.md`](./operations/mcp-stdio.md); per-
  client snippets for Codex CLI and Claude Desktop live under
  `docs/operations/mcp/`. Foundation merged 2026-05-12;
  distribution slice (install + smoke scripts + top-README link)
  merged 2026-05-13; crates.io distribution + real-kernel container
  integration test merged 2026-05-14 (Bundle B —
  `crates/choreo-mcp-proto/` vendored proto crate, tag-gated
  `publish-crate-{proto,mcp}` workflow jobs, per-PR
  `publish-dry-run` gate, `tests/real_kernel.rs` behind the
  `container-tests` Cargo feature).
- **Downstream product integrations (PIR, payments incident response,
  custom agentic flows)** are **out of scope for this repo**. The
  product owns its own deliberation surface; Choreographer's job is to
  expose a clean, fully-typed gRPC API plus the MCP wrapping so any
  agentic consumer can drive it.

Genuinely open work: Epic 9 (a dedicated council-decision RPC if and
when a consumer asks for more than the generic `Deliberate` /
`Orchestrate`), the handshake-level TLS integration test (currently
the wiring is exercised by env-loading unit tests + helm-lint, not a
real cert handshake), and the Epic 11 follow-ups (a stack scenario
that asserts schema validation + ExternalContextBundle round-trip,
and merging the provider-runner E2E into the same compose stack).
Milestones A, B, and C are all complete as of 2026-05-12. The
`choreo-mcp` crates.io distribution debt is cleared as of
2026-05-14 (Bundle B): vendored proto crate + tag-gated publish jobs
+ per-PR publish-dry-run + real-kernel container integration test.

The recommended remaining execution order is:

- Phase 3: handshake-level TLS integration test
- Phase 4: dedicated agent-facing RPC + report artifact (only if a
  consumer asks for them)
- Phase 5: stack E2E with a real council and the Runtime executor

## Out of scope

This backlog does not include:

- moving kernel graph semantics into Choreographer
- making Choreographer domain-specific (payments, incidents, …)
- implementing any downstream product (PIR, payments incident
  response, etc.) in this repository

## Priorities

### P0 — hard blockers (remaining)

These items still block downstream consumer integration.

- dedicated consumer-facing RPC surface (Epic 9)
- stack E2E proof with real provider council + ExternalContextBundle round-trip + schema-validated structured output in the same compose stack (Epic 11 follow-ups)

Already cleared: Runtime executor adapter (Epic 1), Kernel context
boundary (Epic 2), structured council output contracts (Epic 3),
causal metadata model (Epic 5), provider-backed agent factory
composition (Epic 6), honest transport semantics (Epic 7 — declared
plain NATS), gRPC server TLS/mTLS posture (Epic 8 server side).

### P1 — required before production

- contract-aware validators — JSON Schema variant landed in PR #54;
  bounded-event-shape variant landed 2026-05-14 (size + depth +
  object-keys + array-len + string-len caps, wired into the default
  validator chain in `compose.rs`).
- release-gate stack smoke — done 2026-05-14 (Bundle A): a new
  `compose-smoke` job in `publish-distribution.yml` runs the
  9-scenario `make e2e-compose` end-to-end on every `v*` tag push
  and gates both `publish-image` and `publish-helm-chart` behind
  it. Plain `main` pushes skip the smoke so the `latest`/`main`
  image rolls keep working.

### P2 — useful after first integration

- bus-native downstream coupling
- per-proposal streaming for expert councils
- richer score explainability

## Session log

Append-only summary of which PRs advanced which epic per working
session. Quick orientation for future sessions; per-epic progress
notes still live in each epic block below.

### 2026-05-12

8 PRs merged, in order:

- **#44** `feat(core): first-class causal task metadata` — Epic 5.
  `TaskMetadata` value object on `Task` (source/causation/correlation
  ids, council/output contract ids, execution profile) +
  `EventEnvelope.causation_id`. Proto + AsyncAPI additive.
- **#45** `test(e2e): assert causal metadata propagates over NATS` —
  Epic 5 stack-E2E sub-gap. e2e-runner scenario 4 publishes a
  `TriggerEvent` on NATS with known ids, asserts the outbound
  `DeliberationCompleted` carries them. Validated with
  `make e2e-compose`.
- **#46** `docs(backlog): honest re-audit of PIR readiness epics` —
  audited Epics 1–12 against actual code; corrected outdated "not
  done" labels on epics already shipped in commit `fab9bfb` (PR #43).
- **#47** `feat(adapters): provider-backed agent factory composition` —
  Epic 6. `DispatchingAgentFactory` recognises `noop` / `anthropic` /
  `openai` / `vllm` from env + per-descriptor overrides. Wired in
  `compose.rs`.
- **#48** `docs(spec): declare plain NATS broker semantics honestly` —
  Epic 7 option A. AsyncAPI `servers.nats.description` retitled to
  declare plain core pub/sub; JetStream deferred to a future epic
  if bus coupling demand emerges.
- **#49** `feat(transport): gRPC server TLS/mTLS wiring` — Epic 8
  server side. `GrpcTlsConfig` on `ServiceConfig`, env-driven mode
  selection (`none`/`server`/`mutual`), `ServerTlsConfig`
  application in `runtime.rs`, chart template wires the secret +
  env vars, helm-lint gate 4 added.
- **#50** `feat(mcp): hand-rolled stdio MCP adapter for the
  choreographer API` — Epic 13 foundation. Crate `choreo-mcp`
  exposes the 12 RPCs of `underpass.choreo.v1` as MCP tools over
  JSON-RPC/stdio. Hand-rolled (no SDK), fixture + gRPC backends,
  field-for-field JSON↔proto mappers, TLS auto-detection, buffered
  streaming.
- **#51** `docs(mcp): distribution + UX for the choreo-mcp adapter` —
  Epic 13 distribution slice. Install wrapper, smoke script, canonical
  user-facing docs at `docs/operations/mcp-stdio.md`, per-client
  snippets for Codex CLI and Claude Desktop, top-level README link.
  Backlog reframed (this file): PIR framing dropped; Epic 13 added.

State at session close: Milestones A (foundations) + B (mostly,
report artifact pending) + C (Epic 6 + 7 done; Epic 8 server done,
outbound client TLS open) substantially advanced. MCP adapter live
end-to-end with `cargo install --git` UX (crates.io publication
deferred — proto vendoring required). Bundle B (2026-05-14) cleared
the registry leg: `choreo-mcp-proto` + `choreo-mcp` ship to crates.io
through tag-gated publish jobs.

## Phase 1 — Runtime And Context Foundations

### Epic 1. Runtime executor adapter

Status: done

Current state:

- `RuntimeExecutor` adapter implements `ExecutorPort` against the
  Underpass Runtime gRPC; creates ephemeral sessions, invokes tools,
  closes sessions, maps `Succeeded`/`Failed`/`Denied`/transport errors
  distinctly
- `ExecutorBackendConfig` selected from `CHOREO_EXECUTOR_KIND=noop|runtime`
  plus principal env vars; `NoopExecutor` stays as explicit fallback
- adapter unit tests cover success, denial, transport error, env
  loading, and option-vs-attributes precedence; compose-level test
  wires the runtime adapter against a stub gRPC server

Relevant code:

- [`crates/choreo-adapters/src/runtime.rs`](../crates/choreo-adapters/src/runtime.rs)
- [`crates/choreo/src/compose.rs`](../crates/choreo/src/compose.rs) (`wire_executor`)
- [`crates/choreo-core/src/ports/executor.rs`](../crates/choreo-core/src/ports/executor.rs)

Progress as of 2026-05-11: implementation landed in commit `fab9bfb`
(PR #43). All four acceptance criteria + the three required tests
listed below are present in the repo today.

#### Deliverables

1. add a Runtime gRPC executor adapter in `choreo-adapters`
2. map winner proposals plus execution options into runtime session creation
3. support runtime session metadata for:
   - external incident id
   - external incident run id
   - specialist / council identity
   - tool / governance / success profile
4. map runtime invocation outcomes into `ExecutionOutcome`
5. classify:
   - transport errors
   - governed denials
   - runtime failures
   - runtime success

#### Acceptance criteria

- `compose()` can wire Runtime executor behind configuration
- `NoopExecutor` remains available as explicit fallback, not as the only path
- orchestration against a fake or bufconn runtime returns real execution ids
- denial and failure semantics are distinguishable in tests

#### Tests required

- unit tests for request mapping
- adapter integration test against stub gRPC server
- use-case integration test proving `OrchestrateUseCase` emits correct events
  for success and failure with the runtime adapter wired

### Epic 2. Kernel context boundary

Status: done (option A — caller-materialized context)

Current state:

- typed `ExternalContextBundle` (with `ContextSummary`, `ContextItem`,
  `ContextReference`, bounded sizes, and serde + roundtrip tests)
  lives in the core
- `Task` carries `Option<ExternalContextBundle>` through
  `new_with_context` / `new_with_metadata`
- proto exposes the bundle on `Task` and on `TriggerEvent`; gRPC mappers
  consume it
- `AutoDispatchService` propagates the trigger's bundle into the task;
  `DeliberateUseCase` threads it into `DraftRequest.external_context`
  with a covering test

Relevant code:

- [`crates/choreo-core/src/entities/external_context.rs`](../crates/choreo-core/src/entities/external_context.rs)
- [`crates/choreo-core/src/entities/task.rs`](../crates/choreo-core/src/entities/task.rs)
- [`crates/choreo-adapters/src/grpc/mappers/task.rs`](../crates/choreo-adapters/src/grpc/mappers/task.rs)
- [`crates/choreo-adapters/src/grpc/mappers/event.rs`](../crates/choreo-adapters/src/grpc/mappers/event.rs)
- [`crates/choreo-app/src/services/auto_dispatch.rs`](../crates/choreo-app/src/services/auto_dispatch.rs)
- [`crates/choreo-app/src/usecases/deliberate.rs`](../crates/choreo-app/src/usecases/deliberate.rs)

Progress as of 2026-05-11: implementation landed in commit `fab9bfb`
(PR #43). Option B (a Kernel adapter port owned by Choreographer)
remains explicitly deferred — the backlog recommended option A.

#### Deliverables

1. define one explicit context ingestion boundary:
   - option A: caller fetches kernel context and passes it to Choreographer
   - option B: Choreographer can fetch context itself through a new port
2. choose one as the production path for the first downstream integration
3. define a stable structured bundle shape for expert councils:
   - incident summary
   - prior findings
   - prior decisions
   - evidence references
   - failed remediations
4. make that bundle addressable and testable

#### Recommendation

For the first slice, prefer caller-materialized context:

- the consumer remains the kernel-first owner
- Choreographer remains domain-agnostic
- the integration boundary is cleaner

That means Choreographer must still gain a first-class notion of
"structured external context bundle", but it does not have to own
Kernel transport in v1.

#### Acceptance criteria

- one typed council input can carry a bounded incident context bundle
- the council path can consume that bundle without lossy string stuffing
- the chosen bundle shape has contract tests

#### Tests required

- serialization tests for the bundle shape
- one end-to-end deliberation test with a realistic external context bundle

## Phase 2 — Contract-Shaped Deliberation

### Epic 3. Structured council outputs

Status: done

Current state:

- `OutputContract` value object with `OutputFieldRule` and
  `OutputFormat::JsonObject`; serde + validation tests in place
- `TaskConstraints::with_output_contract` carries the contract; the
  proto contract surfaces it inside `Constraints`
- `DeliberateUseCase` switches into structured-output mode when a
  contract is set; valid proposals are reprioritized before any winner
  selection so invalid outputs cannot leak as winners
- deterministic failure: `DomainError::NoValidProposal { contract_id }`;
  `OrchestrateUseCase` maps it to `TaskFailed` with
  `error_kind = "deliberation.no_valid_proposal"`
- regression tests prove invalid proposals lose to valid ones even at
  higher score, and that an all-invalid run fails deterministically

Relevant code:

- [`crates/choreo-core/src/value_objects/output_contract.rs`](../crates/choreo-core/src/value_objects/output_contract.rs)
- [`crates/choreo-core/src/entities/proposal.rs`](../crates/choreo-core/src/entities/proposal.rs)
- [`crates/choreo-app/src/usecases/deliberate.rs`](../crates/choreo-app/src/usecases/deliberate.rs) (`prioritize_valid_outputs`, `pick_winner`)

Progress as of 2026-05-11: implementation landed in commit `fab9bfb`
(PR #43).

#### Deliverables

1. define a structured-output mode for councils
2. allow a council invocation to declare:
   - output contract id
   - JSON schema or equivalent validator
   - allowed decision set
3. ensure winner selection happens only among valid proposals
4. define deterministic failure semantics when:
   - every proposal is invalid
   - schema validation fails
   - allowed decision validation fails

#### Acceptance criteria

- a council can be run in "contract output" mode
- invalid outputs do not leak out as winners
- caller can distinguish:
   - no valid proposal
   - transport failure
   - provider failure
   - validation failure

#### Tests required

- schema validator tests
- allowed-decision validator tests
- deliberation test where one invalid proposal loses to a valid one
- deliberation test where all proposals are invalid and the run fails
  deterministically

### Epic 4. Contract-aware validators

Status: done

Current state (2026-05-12):

- five validators wired through `Vec<Arc<dyn ValidatorPort>>` in
  `compose.rs`: `ContentNonEmptyValidator`, `JsonObjectOutputValidator`,
  `RequiredFieldsValidator`, `AllowedStringValuesValidator`,
  `JsonSchemaValidator`
- `JsonObjectOutputValidator` enforces JSON-object root for
  `OutputFormat::JsonObject`
- `RequiredFieldsValidator` enforces required fields from
  `OutputContract.fields`
- `AllowedStringValuesValidator` enforces enum / allowed-decision
  membership
- `JsonSchemaValidator` compiles `OutputContract.json_schema`
  (new proto field 4 + new `OutputContract::new_with_schema`
  constructor) and validates every proposal output against the
  embedded schema. **Subsumes the "bounded event proposal shape"
  deliverable** — bounded shapes are expressed as standard JSON
  Schema (`maxLength`, `maxItems`, `additionalProperties: false`,
  `pattern`, `enum`, …) rather than a bespoke validator.
- validator reports stay domain-agnostic (only
  `kind`/`passed`/`summary`/`Attributes`); JSON-Schema failures cap
  reported violations at 16 with `instance_path` + `schema_path` +
  `reason` per entry, and a one-line summary picks the first
  violation
- unit tests cover both legacy paths (no contract, no schema) and the
  new JSON-Schema paths (satisfying output, missing required field,
  `maxItems` violation, malformed schema body, malformed proposal)

Relevant code:

- [`crates/choreo-adapters/src/validators.rs`](../crates/choreo-adapters/src/validators.rs)
- [`crates/choreo/src/compose.rs`](../crates/choreo/src/compose.rs)
- [`crates/choreo-core/src/value_objects/output_contract.rs`](../crates/choreo-core/src/value_objects/output_contract.rs)
- [`crates/choreo-adapters/src/grpc/mappers/output_contract.rs`](../crates/choreo-adapters/src/grpc/mappers/output_contract.rs)
- [`api/examples/output-contracts/`](../api/examples/output-contracts/) — canonical Report-shape schema + README

Progress as of 2026-05-12: format-level slice (`ContentNonEmpty` +
`JsonObject` + `RequiredFields` + `AllowedStringValues`) landed in
`fab9bfb`; JSON Schema slice landed today on top of new proto field
`OutputContract.json_schema` (field 4, additive).

#### Deliverables — all met

- general JSON Schema validator — done via `JsonSchemaValidator` +
  the `jsonschema` workspace dep
- bounded event proposal shape validator — done via JSON Schema
  constraints (`maxLength`, `maxItems`, `additionalProperties: false`)
- report artifact shape validator — done via JSON Schema with the
  canonical Report shape at
  `api/examples/output-contracts/report.schema.json` (Epic 10
  delivery — see below)

#### Acceptance criteria

- validators are composable through the existing validation pipeline (done)
- validator reports remain domain-agnostic from Choreographer's perspective (done)
- consumer-facing council contracts can be enforced with no
  handwritten post-processing hacks (done — JSON Schema covers
  nested objects, arrays, enums, patterns, bounds)

### Epic 5. Task / council metadata model

Status: done for the domain-agnostic core slice

Current state:

- `Task` now has integration-neutral `TaskMetadata`
- `EventEnvelope` carries both `correlation_id` and `causation_id`
- inbound trigger metadata is converted into task metadata
- lifecycle events preserve causal metadata through deliberation and orchestration

Progress as of 2026-04-26:

- added first-class `TaskMetadata`
- added `source_event_id`, `causation_id`, and `correlation_id` propagation
- added proto and gRPC mapper support for task metadata
- kept application-owned identifiers out of the core; product/domain ids remain
  in `Task.attributes` or `ExternalContextBundle.metadata`
- added tests proving causal metadata reaches deliberation, dispatch, completion,
  and failure events
- wired `execution_profile` into executor options, with explicit call options
  taking precedence

Relevant code:

- [`crates/choreo-core/src/entities/task.rs`](../crates/choreo-core/src/entities/task.rs)
- [`crates/choreo-core/src/events/envelope.rs`](../crates/choreo-core/src/events/envelope.rs)

#### Deliverables

Introduce a first-class metadata surface that can carry:

- source event id
- causation id
- correlation id
- council contract id
- output contract id
- execution profile metadata

Application-specific identifiers must remain outside the core metadata model.
Use `Task.attributes` or `ExternalContextBundle.metadata` for product/domain
ids such as incidents, cases, claims, shipments, studies, or similar concepts.

#### Acceptance criteria

- metadata survives trigger -> task -> deliberation -> orchestration -> outbound event
- metadata can be inspected in tests without parsing arbitrary blobs
- execution-profile metadata is wired into the executor path where applicable

## Phase 3 — Provider And Composition Readiness

### Epic 6. Provider-backed agent factory composition

Status: done

Current state:

- `DispatchingAgentFactory` (in `crates/choreo-adapters/src/agents/factory.rs`)
  implements `AgentFactoryPort` and dispatches on `descriptor.kind`:
  - `"noop"` — always available
  - `"anthropic"` — gated on `agent-anthropic` feature + `CHOREO_ANTHROPIC_API_KEY`
  - `"openai"` — gated on `agent-openai` feature + `CHOREO_OPENAI_API_KEY`
  - `"vllm"` — gated on `agent-vllm` feature + `CHOREO_VLLM_MODEL` + `CHOREO_VLLM_ENDPOINT`
- per-descriptor overrides: `provider.model`, `provider.endpoint`,
  `provider.max_tokens` on the descriptor's `attributes`
- credentials live ONLY in env (descriptors are persisted in Postgres,
  so secrets must not flow through them)
- the binary wires `DispatchingAgentFactory::from_env()` unconditionally;
  startup log emits `agent_kinds=...` listing the supported set
- `supported_kinds()` accessor returns the live list so operators can
  see which kinds the deployment will accept on `RegisterAgent`

Relevant code:

- [`crates/choreo-adapters/src/agents/factory.rs`](../crates/choreo-adapters/src/agents/factory.rs)
- [`crates/choreo/src/compose.rs`](../crates/choreo/src/compose.rs)
- [`crates/choreo-adapters/src/agents/`](../crates/choreo-adapters/src/agents/)

Progress as of 2026-05-11: implementation landed in the next PR.
`NoopAgentFactory` remains available as a single-kind factory for
tests; the production binary uses `DispatchingAgentFactory` only.

#### Deliverables

1. add a dispatching `AgentFactoryPort` composition root
2. support configured real `kind` values in the binary
3. make expert councils materializable from descriptors
4. ensure persistence + rehydration of real agent descriptors works

#### Acceptance criteria

- `RegisterAgent` can materialize at least one real provider kind
- persisted descriptors can be resolved back into live agents
- the binary documents which provider kinds are truly supported

#### Tests required

- composition tests for provider dispatch
- persistence rehydration tests for provider-backed descriptors

## Phase 4 — Transport And Security Honesty

### Epic 7. Honest broker semantics

Status: done (option 1 — declared plain NATS)

Current state:

- `NatsMessaging` uses `Client::publish_with_headers` (core NATS);
  `NatsTriggerSubscriber` uses `client.subscribe` (core NATS,
  fire-and-forget). No JetStream stream / durable consumer / ack /
  replay policy is used by the adapter.
- AsyncAPI now declares the broker as **plain core NATS pub/sub**
  with the matching disclaimer in the `servers.nats.description`
  field; the implementation–spec gap from the original audit is closed.
- `docs/stack-gap-analysis.md` §4 retitled "Broker semantics declared
  honestly as plain NATS".
- The docker-compose / kubernetes test fixtures may still start the
  NATS server with `-js`; this is harmless (server-side JetStream
  capability is independent of whether the client opens it) and
  preserves an upgrade path if option 2 is chosen later.

Decision rationale: the expected first downstream integration uses direct
gRPC (Epic 9), not the bus. Plain NATS is sufficient. Implementing
real JetStream semantics (stream + durable consumer + ack + replay)
is deferred to a future epic gated on actual bus-coupling demand.

Relevant code:

- [`crates/choreo-adapters/src/nats/messaging.rs`](../crates/choreo-adapters/src/nats/messaging.rs)
- [`crates/choreo-adapters/src/nats/subscriber.rs`](../crates/choreo-adapters/src/nats/subscriber.rs)
- [`specs/asyncapi/choreographer.asyncapi.yaml`](../specs/asyncapi/choreographer.asyncapi.yaml)

#### Deliverables

Choose one:

1. explicitly declare Choreographer as plain NATS
2. or implement true JetStream semantics:
   - stream
   - durable consumer
   - ack
   - replay / delivery policy

#### Recommendation

For a critical downstream integration, prefer real JetStream semantics if bus coupling
is expected later. If not, document plain NATS honestly and keep the first downstream
integration on direct gRPC.

#### Acceptance criteria

- code, docs, tests, and spec all say the same thing
- transport semantics are exercised in integration tests

### Epic 8. TLS / mTLS parity

Status: done end-to-end (server + client); only the handshake-level
integration test remains deferred.

Current state (2026-05-11):

- gRPC server in `crates/choreo/src/runtime.rs` builds with
  `ServerTlsConfig::new().identity(...)` (server mode) or additionally
  `client_ca_root(...)` (mutual mode), driven by the new
  `GrpcTlsConfig` enum in `ServiceConfig`. PEM files are read at
  startup; a misconfigured deployment fails fast.
- `EnvConfiguration` reads `CHOREO_GRPC_TLS_MODE` (`none`/`server`/`mutual`),
  `CHOREO_GRPC_TLS_CERT_PATH`, `CHOREO_GRPC_TLS_KEY_PATH`, and (for mutual)
  `CHOREO_GRPC_TLS_CLIENT_CA_PATH`. Validation surfaces missing-path
  combinations as `DomainError::EmptyField` and an invalid mode as
  `InvariantViolated`.
- Chart template (`charts/choreographer/templates/deployment.yaml`) mounts
  `tls.existingSecret` read-only at `/etc/choreographer/tls` and passes
  the matching env vars; rendering with `tls.mode != "none"` but no
  `existingSecret` fails the helm template with an explicit message.
- `scripts/ci/helm-lint.sh` gate 4 asserts the rendered manifest for
  both `server` and `mutual` modes carries the expected env vars and
  volume mount, and that `server` mode does NOT carry the client-CA
  env var.
- `values.yaml` `tls.mode` and `tls.existingSecret` are now honest
  configuration with documented secret layout.

Progress as of 2026-05-12: Runtime client TLS shipped. The adapter
now:

- carries a `RuntimeClientTlsConfig` (variants `Disabled`/`Server`/
  `Mutual`) on `RuntimeExecutorConfig`, mirroring the env-driven
  auto-detection used by `crates/choreo-mcp` (`https://` endpoint
  → server; `_CERT_PATH`/`_KEY_PATH` → mutual; explicit
  `CHOREO_RUNTIME_TLS_MODE` wins);
- rewrites `http://` to `https://` at connect time when TLS is on so
  callers can flip a single env var;
- reads PEM files at startup and applies `ClientTlsConfig` on the
  tonic `Endpoint` (with `ca_certificate`, `identity`, and
  `domain_name` per mode);
- surfaces PEM-read errors as a dedicated `TlsReadFailed { path,
  source }` variant of `RuntimeExecutorConnectError`;
- ships 7 env-loading + URI-upgrade unit tests covering disabled,
  server auto-upgrade on `https://`, mutual auto-upgrade on
  client-paths, explicit-mode override, mutual requiring both paths,
  invalid-mode rejection, and the URI rewriter.

Remaining work — **all done 2026-05-14 (Bundle A)**:

- Rust integration test that performs an actual TLS handshake
  against the choreographer using `rcgen` to mint a self-signed
  CA + server + client leaves in memory. Lives in
  `crates/choreo-tests-integration/tests/tls_server_handshake.rs`
  (server mode) and `tls_mutual_handshake.rs` (mutual mode: a
  client with identity is accepted, a client without identity
  is rejected). The fixture
  `crates/choreo-tests-integration/src/tls_fixture.rs` exports
  `mint_tls(server_san)`; `GrpcFixture::start_with_tls(setup)`
  serves over TLS without process-env mutation.

Relevant code:

- [`crates/choreo/src/runtime.rs`](../crates/choreo/src/runtime.rs)
- [`crates/choreo-adapters/src/config.rs`](../crates/choreo-adapters/src/config.rs)
- [`crates/choreo-core/src/ports/configuration.rs`](../crates/choreo-core/src/ports/configuration.rs)
- [`charts/choreographer/templates/deployment.yaml`](../charts/choreographer/templates/deployment.yaml)
- [`scripts/ci/helm-lint.sh`](../scripts/ci/helm-lint.sh)
- [`crates/choreo-adapters/src/runtime.rs`](../crates/choreo-adapters/src/runtime.rs) (Runtime client — TLS shipped 2026-05-12)

#### Deliverables

1. add server TLS/mTLS wiring if the chart surface is kept
2. or remove unsupported chart keys until real
3. ensure client-side TLS exists for Runtime calls

#### Acceptance criteria

- every declared TLS value has a code path behind it
- there is at least one integration test for enabled TLS mode

## Phase 5 — Consumer-Facing Integration Surface

### Epic 9. Specialist-grade RPC surface

Status: done (2026-05-12)

Current state:

- proto exposes `RunCouncilDecision` plus contract CRUD
  (`RegisterContract` / `ListContracts` / `DeleteContract`) on top
  of the original generic RPCs
- the dedicated council-decision RPC takes a council selector
  (`council_id` or `specialty`), a registered `contract_id`, an
  optional `ExternalContextBundle`, a `ValidationMode` (Strict /
  Warn), causal metadata, and a free-form description
- response carries `task_id`, the validated `winner`
  (`DeliberationResult`), a `ValidationOutcomeSummary`, per-candidate
  `CandidateSummary` ordered by rank, duration, and the echoed
  `validation_mode`
- the `RunCouncilDecisionUseCase` composes the existing
  `DeliberateUseCase` with the new `ContractRegistryPort`; Strict
  mode propagates `NoValidProposal`, Warn mode returns the
  top-ranked candidate with `passed=false`
- the in-memory `ContractRegistry` is the source of truth today;
  contracts are seeded at startup from `CHOREO_CONTRACT_DIR`
- the MCP stdio adapter exposes four matching tools
  (`choreo_run_council_decision`, `choreo_register_contract`,
  `choreo_list_contracts`, `choreo_delete_contract`)

Relevant code:

- [`crates/choreo-proto/proto/underpass/choreo/v1/choreo.proto`](../crates/choreo-proto/proto/underpass/choreo/v1/choreo.proto)
- [`crates/choreo-app/src/usecases/run_council_decision.rs`](../crates/choreo-app/src/usecases/run_council_decision.rs)
- [`crates/choreo-core/src/ports/contract_registry.rs`](../crates/choreo-core/src/ports/contract_registry.rs)
- [`crates/choreo-adapters/src/grpc/mappers/run_council_decision.rs`](../crates/choreo-adapters/src/grpc/mappers/run_council_decision.rs)
- [`crates/choreo-tests-integration/tests/run_council_decision_rpc.rs`](../crates/choreo-tests-integration/tests/run_council_decision_rpc.rs)

#### Deliverables — all met

- [x] `RunCouncilDecision` RPC with council selector, structured
  external context bundle, output contract id, validation mode, and
  metadata
- [x] Response carries the validated structured winner, validation
  outcome summary, candidate summaries, and trace metadata

#### Acceptance criteria — all met

- [x] consumers can call a dedicated RPC without abusing generic
  trigger fan-out
- [x] council result is already validated when returned

### Epic 10. Report artifact support

Status: done (via JSON Schema, no bespoke entity)

Decision (2026-05-12): rather than add a `Report` / `HumanHandoffReport`
entity to `choreo-core` and a parallel proto message — which would
have baked product vocabulary into the core — Report becomes an
**output contract shape** expressed as a JSON Schema. Consumers bind
the canonical schema (or any variant) via
`Constraints.output_contract.json_schema`; the `JsonSchemaValidator`
adapter from Epic 4 enforces the shape on every proposal.

Current state:

- canonical Report-shape schema lives at
  [`api/examples/output-contracts/report.schema.json`](../api/examples/output-contracts/report.schema.json)
  with required `report_id` + `executive_summary` + `findings` +
  `recommended_actions`, and optional `timeline` + `remediations_attempted`
  + `open_risks` + `evidence_references`. Every field is bounded
  (`maxLength`, `maxItems`) so a runaway model cannot blow the
  payload. `additionalProperties: false` everywhere keeps the shape
  honest.
- companion [`api/examples/output-contracts/README.md`](../api/examples/output-contracts/README.md)
  documents the wiring (Rust + proto) and when to choose JSON Schema
  vs. field-level rules.
- the schema doubles as the test fixture for the JsonSchemaValidator
  smoke and as the reference for downstream consumers.

Relevant code:

- [`api/examples/output-contracts/`](../api/examples/output-contracts/)
- [`crates/choreo-core/src/value_objects/output_contract.rs`](../crates/choreo-core/src/value_objects/output_contract.rs) (`json_schema()` accessor)
- [`crates/choreo-adapters/src/validators.rs`](../crates/choreo-adapters/src/validators.rs) (`JsonSchemaValidator`)
- [`crates/choreo-proto/proto/underpass/choreo/v1/choreo.proto`](../crates/choreo-proto/proto/underpass/choreo/v1/choreo.proto) (`OutputContract.json_schema` proto field 4)

#### Deliverables — all met

The original deliverable list called for a "structured report result
type" with minimum fields: executive_summary, incident_timeline,
findings, remediations_attempted, open_risks, recommended_human_actions,
evidence_references. Every one of those is present in the canonical
schema (renamed to drop product vocabulary where appropriate, e.g.
`recommended_actions` instead of `recommended_human_actions`).

#### Acceptance criteria

- report output is schema-validated — done via `JsonSchemaValidator`
- report output can be persisted or returned without lossy
  flattening — done via the proposal's free-form `content` field
  carrying the JSON document verbatim; no flattening since the
  schema is JSON-native

## Phase 6 — Stack E2E Readiness

### Epic 11. Choreographer stack E2E

Status: positive structured-output leg landed 2026-05-14 (stub-LLM
sidecar + scenario 8); the "real provider council" leg
(vLLM/Anthropic/OpenAI in the compose stack) remains optional and is
still covered separately by `make e2e-provider-vllm`.

Current state:

- `crates/choreo-e2e-runner/src/main.rs` runs eight scenarios against
  a real gRPC + NATS stack with stub-runtime + stub-llm sidecars:
  1. seeded council is visible
  2. `Deliberate` on the seeded specialty returns a winner
  3. `DeleteCouncil` on an unknown specialty returns `deleted=false`
  4. inbound `TriggerEvent` over NATS produces an outbound
     `DeliberationCompleted` carrying the same `correlation_id` and
     `causation_id` (scenario added 2026-05-11, PR #45)
  5. `Orchestrate` routes the winning proposal through the
     configured `RuntimeExecutor` to the `stub-runtime` sidecar
     (added 2026-05-12). Asserts the response carries the
     stub's canned `execution_id` and that the winner proposal
     is non-empty.
  6. `Orchestrate` with a strict JSON-Schema output contract rejects
     `NoopAgent`'s free-form text. Asserts the gRPC call returns
     `FailedPrecondition`, AND that the outbound bus carries a
     `TaskFailed` envelope with `error_kind =
     "deliberation.no_valid_proposal"` (added 2026-05-12). Proves
     the JsonSchemaValidator wired by Epic 4 is in the stack's
     validator chain.
  7. `Deliberate` with an `ExternalContextBundle` attached round-trips
     the bundle id into the outbound `DeliberationCompleted` envelope.
  8. `RunCouncilDecision` in Strict mode against the canonical Report
     contract passes with a winner whose proposal content validates
     against `api/examples/output-contracts/report.schema.json`. The
     agent is an `openai`-kind descriptor pointing at the `stub-llm`
     sidecar (added 2026-05-14); the outbound envelope omits
     `external_context_bundle_id` (no bundle was sent).
- the `stub-runtime` sidecar ships in this repo as
  `crates/choreo-e2e-runner/src/bin/stub_runtime.rs` +
  `tests/e2e/stub-runtime.Dockerfile`. It serves the canonical
  `underpass.runtime.v1.{SessionService,InvocationService}` and
  always returns a successful canned Session / Invocation. Lets
  Choreographer's `RuntimeExecutor` exercise a real gRPC peer
  without dragging the real underpass-runtime image into this
  repo's test path.
- the compose stack now wires `CHOREO_EXECUTOR_KIND=runtime` +
  `CHOREO_RUNTIME_GRPC_ENDPOINT=http://stub-runtime:50053` so
  scenarios 2 / 5 exercise the full Deliberate -> winner ->
  RuntimeExecutor -> gRPC InvokeTool -> outcome path. Validated
  locally with `CONTAINER_RUNTIME=podman-compose make e2e-compose`:
  the stub-runtime logs `CreateSession`, `InvokeTool(stub.echo)`,
  and `CloseSession` once per orchestration.
- the seed council still uses `NoopAgent`; scenarios 1–7 stay on
  that path. Scenario 8 dynamically registers an `openai`-kind agent
  pointing at the `stub-llm` sidecar so the positive structured-
  output path is exercised without a real provider in the compose
  stack. Real-provider councils against vLLM remain exercised
  separately by `make e2e-provider-vllm` (Epic-6 provider runner).

Relevant code:

- [`crates/choreo-e2e-runner/src/main.rs`](../crates/choreo-e2e-runner/src/main.rs)
- [`crates/choreo-e2e-runner/src/bin/stub_runtime.rs`](../crates/choreo-e2e-runner/src/bin/stub_runtime.rs)
- [`crates/choreo-e2e-runner/src/bin/stub_llm.rs`](../crates/choreo-e2e-runner/src/bin/stub_llm.rs)
- [`tests/e2e/stub-runtime.Dockerfile`](../tests/e2e/stub-runtime.Dockerfile)
- [`tests/e2e/stub-llm.Dockerfile`](../tests/e2e/stub-llm.Dockerfile)
- [`tests/e2e/docker-compose.e2e.yaml`](../tests/e2e/docker-compose.e2e.yaml)

#### Deliverables

Add a reproducible test proving:

```text
bounded external trigger
  -> context bundle
    -> real council
      -> validated structured result
        -> runtime execution or bounded output
```

Status of the chain:

- bounded external trigger → ✅ scenario 4 (NATS) + scenario 5 (gRPC).
- context bundle → ✅ scenario 7 round-trips the bundle id into the
  outbound `DeliberationCompleted` envelope (2026-05-14).
- real council → ✅ via `make e2e-provider-vllm` (provider runner).
- validated structured result → ✅ scenario 6 covers the rejection
  path (JsonSchemaValidator fires, `error_kind =
  "deliberation.no_valid_proposal"` on the bus, `FailedPrecondition`
  on gRPC); scenario 8 (2026-05-14) covers the positive path via the
  `stub-llm` sidecar — `RunCouncilDecision` in Strict mode returns
  a Report-shaped winner that validates against the canonical
  schema.
- runtime execution → ✅ scenario 5 (stub-runtime).

#### Remaining follow-ups (out of Milestone D's critical path)

- scenario 7: `Deliberate` with an `ExternalContextBundle` attached
  → ✅ done (2026-05-14). `DeliberationCompletedEvent` now carries
  the optional `external_context_bundle_id` and the e2e-runner asserts
  the bundle id round-trips to the outbound bus envelope.
- positive structured-output scenario → ✅ done (2026-05-14).
  Scenario 8 brings up a `stub-llm` sidecar (OpenAI Chat Completions
  shape, always returns a JSON Report payload that satisfies the
  canonical schema) and asserts the positive path through
  `RunCouncilDecision` in Strict mode.
- merge the provider-runner E2E (vLLM) into the same compose stack
  → ✅ done 2026-05-14 (Bundle A). Scenario 9 registers a
  `kind=vllm` agent pointing at the existing `stub-llm` sidecar
  (both adapters speak `POST /v1/chat/completions`), so a single
  `make e2e-compose` now covers both the openai-shaped and the
  vllm-shaped paths. `make e2e-provider-vllm` stays for operators
  who want to validate against a REAL vLLM endpoint.
- compose-level operations doc (`docs/operations/compose-e2e.md`):
  the stub-llm sidecar, its OpenAI-compat surface, the hard-coded
  Report payload, and the `STUB_LLM_LISTEN` override all need a
  prose home. The doc itself does not exist yet — leaving as a
  follow-up so this slice stays additive.

### Epic 12. Consumer integration smoke prerequisites

Status: done (2026-05-14).

#### What shipped

A consumer-shaped smoke harness — `crates/choreo-consumer-smoke` —
that drives the choreographer's public surface (gRPC over `tonic` +
optional core NATS over `async-nats`) the way a real downstream
consumer would. Distributed as a library the choreographer's own
integration tests reuse and a CLI a consumer can point at a live
cluster.

Two chains:

- **Chain 1** (Warn-mode reevaluation): trigger-style envelope
  publish on `choreo.trigger.<specialty>`, `RunCouncilDecision` in
  Warn mode with a deterministic kernel-rehydration-shaped bundle,
  then six typed assertions including correlation / causation
  propagation on the outbound `choreo.deliberation.completed`.
- **Chain 2** (Strict-mode handoff report): registers the canonical
  Report `OutputContract`
  (`api/examples/output-contracts/report.schema.json`), runs Strict
  mode, asserts the rejection path against the NoopAgent stack
  (free-form text fails the JSON Schema → `FailedPrecondition`
  mentioning the contract id).

Deliverables:

- New crate `crates/choreo-consumer-smoke` (lib + bin).
- 11 lib unit tests + 2 binary unit tests + 2 integration tests
  (`tests/chain1_warn_against_fixture.rs`,
  `tests/chain2_strict_rejection_against_fixture.rs`) that reuse
  `choreo_tests_integration::grpc_fixture::GrpcFixture`.
- `Harness::from_parts(channel, nats)` helper so the integration
  tests can share the fixture's `tonic::Channel`.
- Operations doc: `docs/operations/consumer-smoke.md`.
- `make consumer-smoke` Makefile target.

#### Remaining gaps (out of Epic 12's scope, tracked under Epic 11)

- **Bundle round-trip** (`bundle_seam_documented` Skipped). Owned by
  Epic 11 scenario 7. Once that lands the assertion flips to Passed.
- **Chain 2 positive path** (`report_payload_validates` Skipped on
  today's stack). The stub-LLM sidecar shipped under Epic 11
  scenario 8 (2026-05-14) — follow-up consumers can now register an
  `openai`-kind agent against `http://stub-llm:8000` to exercise the
  positive path. Wiring `choreo-consumer-smoke` chain 2 to that
  sidecar (so the harness flips Skipped → Passed) is the remaining
  work; the underlying capability is in place.
- **Provider-runner E2E merged into `make e2e-compose`** so a single
  command exercises real provider council + real (stub) runtime in
  one shot — also Epic 11.

#### Relevant code

- [`crates/choreo-consumer-smoke/`](../crates/choreo-consumer-smoke/)
- [`docs/operations/consumer-smoke.md`](./operations/consumer-smoke.md)

### Epic 13. MCP stdio adapter

Status: foundation done (2026-05-12); distribution slice done
(2026-05-14, Bundle B — crates.io publication + real-kernel
container integration test).

Current state:

- `crates/choreo-mcp` exposes every RPC of `underpass.choreo.v1` as
  a `choreo_*` MCP tool (12 tools 1:1 with the gRPC service).
- JSON-RPC 2.0 over stdin/stdout, no MCP SDK — the wire protocol is
  hand-rolled so it stays in lock-step with the proto contract.
- `ChoreoMcpToolBackend` trait has two impls: fixture (canned
  responses for client wiring) and gRPC (live tonic client with
  optional TLS).
- Field-for-field JSON ↔ proto mappers in `src/grpc/{json_to_proto,
  proto_to_json}.rs` — 100% API respected.
- `StreamDeliberation` buffered into one response (frames array +
  winner extracted from the last `result`-typed frame). MCP stdio is
  sync.
- 6 env vars (`CHOREO_MCP_BACKEND` + 5 `CHOREO_MCP_GRPC_TLS_*`) with
  the same auto-detection pattern as the sibling rehydration-mcp.
- 21 unit tests + workspace clippy clean.

Distribution slice (done 2026-05-14):

- `scripts/mcp/install-choreo-mcp.sh` — registry mode by default
  (`cargo install choreo-mcp`); `CHOREO_MCP_INSTALL_MODE=git` falls
  back to the `--git`/`--branch`/`--tag`/`--rev` source path.
- `scripts/mcp/choreo-stdio-smoke.sh` — one `tools/call` + grep marker
  for both fixture and live modes.
- `docs/operations/mcp-stdio.md` — canonical user-facing UX.
- `docs/operations/mcp/codex.md`, `docs/operations/mcp/claude-desktop.md`
  — per-client config snippets.
- `crates/choreo-mcp/README.md` — developer-oriented twin.
- top-level `README.md` link to `docs/operations/mcp-stdio.md`.

Relevant code:

- [`crates/choreo-mcp/`](../crates/choreo-mcp/)
- [`crates/choreo-mcp-proto/`](../crates/choreo-mcp-proto/) — vendored
  proto crate that lets `choreo-mcp` ship to crates.io independent of
  the internal workspace's `choreo-proto`.
- [`crates/choreo-mcp/tests/real_kernel.rs`](../crates/choreo-mcp/tests/real_kernel.rs)
  — `container-tests`-gated integration test that boots the
  published choreographer image and drives the MCP binary
  end-to-end (16 tools listed, 4 read-only RPCs called).
- [`docs/operations/mcp-stdio.md`](./operations/mcp-stdio.md)

#### Deliverables

1. `crates.io` publication — **done 2026-05-14**. The proto tree is
   now vendored into a dedicated `choreo-mcp-proto` crate
   ([`crates/choreo-mcp-proto/`](../crates/choreo-mcp-proto/)); the
   `publish-distribution.yml` workflow gained tag-only
   `publish-crate-{proto,mcp}` jobs that serialize on
   `compose-smoke` (proto first, then mcp with 30 s for registry
   index propagation). A separate `publish-dry-run` job in
   `quality-gate.yml` runs `cargo publish --dry-run -p choreo-mcp-proto`
   + `cargo package --list -p choreo-mcp` on every PR so packaging
   regressions surface before the release-tag workflow.
2. Real-kernel integration test — **done 2026-05-14**. Lives at
   [`crates/choreo-mcp/tests/real_kernel.rs`](../crates/choreo-mcp/tests/real_kernel.rs),
   gated by the `container-tests` Cargo feature; `cargo test
   --workspace` stays fast and network-free.

## Proposed execution order

### Milestone A — real execution and context

Must finish:

- Epic 1
- Epic 2
- Epic 5

Exit condition:

- Choreographer is no longer an isolated deliberation prototype

**Cleared 2026-05-11.** Runtime executor adapter, kernel context
boundary (option A), and causal metadata model are all done.

### Milestone B — contract-grade councils

Must finish:

- Epic 3
- Epic 4
- Epic 10

Exit condition:

- Choreographer can return consumer-safe structured decisions

**Cleared 2026-05-12.** Epic 3 done; Epic 4 done end-to-end
(JSON Schema validator subsumes the bounded-shape deliverable); Epic
10 done via the canonical Report-shape JSON Schema at
`api/examples/output-contracts/report.schema.json` — no bespoke
entity added to the core.

### Milestone C — production honesty

Must finish:

- Epic 6
- Epic 7
- Epic 8

Exit condition:

- composition, transport, and security claims match reality

**Cleared 2026-05-12.** Epics 6, 7, 8 all done end-to-end. Handshake-
level integration test remains as an Epic 8 follow-up but is not on
the milestone-C critical path.

### Milestone D — Consumer-facing surface

Must finish:

- Epic 9
- Epic 11

Exit condition:

- consumers have a clean RPC surface to integrate with

**Open.** Epic 9 not started; Epic 11 partial (4 scenarios but no real
council and no runtime executor wired in the stack).

### Milestone E — integration-ready

Must finish:

- Epic 12

Exit condition:

- it is reasonable to begin consumer integration work

**Cleared 2026-05-14.** Epic 12 done — consumer-smoke harness lives
at `crates/choreo-consumer-smoke` with two chains, a CLI, and two
integration tests against the in-process `GrpcFixture`. The
remaining "open" assertions (`bundle_seam_documented` and Chain 2's
positive `report_payload_validates`) are intentionally `Skipped`
with explicit reasons that point at Epic 11's pending scenarios; they
are not on Epic 12's critical path.

## Suggested issue breakdown

### Cleared waves (2026-05-11)

- runtime gRPC executor adapter — done
- wire runtime executor in composition root — done
- execution metadata model — done (`TaskMetadata.execution_profile`)
- external context bundle type — done
- structured output mode — done
- incident / run / causation metadata propagation — done

### Open waves

#### Wave 4a — real provider factories — done

- dispatching `AgentFactoryPort` recognising
  `noop`/`anthropic`/`openai`/`vllm` shipped as `DispatchingAgentFactory`
- wired in `compose.rs` behind the existing Cargo features
- env-driven config + per-descriptor `provider.*` overrides
- 10 unit tests in `agents/factory.rs`

Open follow-up: explicit Postgres persistence-rehydration test for a
non-noop descriptor (would require provider credentials in CI; not
strictly required by Epic 6's acceptance, since the dispatcher uses
the existing `RegisterAgentUseCase` path that already has rehydration
tests via `NoopAgentFactory`).

#### Wave 4b — transport honesty — done

- AsyncAPI rewritten to declare plain core NATS pub/sub semantics
  consistent with the current adapter; `stack-gap-analysis.md` §4
  retitled accordingly. JetStream remains the upgrade path if the
  bus-coupling requirement later demands durability.

#### Wave 4c — TLS honesty — done

- gRPC server: `GrpcTlsConfig` wired through `ServiceConfig`,
  `EnvConfiguration` validates the mode combinations, `runtime.rs`
  applies `ServerTlsConfig` (server or mutual). Chart template now
  mounts the secret and passes env vars; helm-lint gate 4 asserts
  the rendered manifest for both modes; 6 env-loading unit tests
  pin the validation paths.
- Runtime gRPC client TLS in `RuntimeExecutor::connect` (shipped
  2026-05-12): mirrors the MCP adapter's pattern with auto-detection
  + 7 env-loading / URI-upgrade unit tests + explicit
  `TlsReadFailed { path, source }` error variant.
- Open follow-up: handshake-level integration test against a
  choreographer instance with a self-signed cert (likely with
  `rcgen` as a dev-dep).

#### Wave 5 — Consumer-facing surface

- add a dedicated `RunCouncilDecision` (or equivalent) RPC backed by
  the structured-output mode
- add JSON Schema validator and a report-shape validator
- add `Report` / `HumanHandoffReport` entity + proto + persistence
- extend the e2e-runner to drive a real council + the Runtime executor

## Gating rule

The following rule should be treated as hard policy:

> No downstream product that requires structured, audited deliberation
> output should depend on Choreographer until Milestones A, B, and C
> are complete.

Status 2026-05-12: Milestones A, B, and C are all complete.
Milestones D (Epic 9 consumer-facing RPC + Epic 11 real-council /
runtime E2E legs) are the remaining open items. The crates.io
publication leg of Epic 13 cleared on 2026-05-14 (Bundle B).

Why the rule still applies in spirit even with B and C now cleared:

- without Milestone D (Epic 9's consumer-facing RPC + Epic 11's
  real-council / runtime E2E legs) no consumer has been proven able
  to drive Choreographer through its public surfaces at production
  scale.

## Final recommendation

If only one sentence is carried forward from this document, it should be:

> Choreographer's job is to be a trustworthy stack peer — real Runtime,
> real context, structured outputs, honest transport, agent-callable
> through gRPC and MCP, with stack E2E — and nothing more. Downstream
> products integrate; they are not implemented here.
