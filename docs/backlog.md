# Choreographer Backlog

Snapshot date: 2026-04-25; honest re-audit 2026-05-11; PIR framing
dropped 2026-05-12 (PIR is owned by a separate project — this backlog
tracks Choreographer's own stack-readiness, not any one downstream
consumer); ceremony engine + LLM-as-judge scorer landed 2026-06-06→09
(Milestone F, below).

Choreographer is agnostic and independent. References to PIR, KMP,
Runtime, or other repositories in this backlog are historical context,
study material, or examples of possible integrations. They are not
required dependencies for Choreographer as a product.

Companion documents:

- [`stack-gap-analysis.md`](./stack-gap-analysis.md)
- [`operations/mcp-stdio.md`](./operations/mcp-stdio.md) — installable
  stdio MCP adapter UX.

The goal is to keep Choreographer trustworthy as a stack peer:
real execution, real context, structured council outputs, causal
metadata, provider-backed councils, honest transport, TLS, an
agent-facing surface (gRPC + MCP), and reproducible stack E2E.

## Executive summary

> **Update 2026-07-05:** the table below is accurate as of 2026-05-14 and the
> session log stops at 2026-05-12. Shipped since then, not yet reflected in
> the epics: the observability suite (#102–#113: Prometheus `/metrics` with
> deliberation/judge/provider/ceremony families, OTel deliberation traces,
> meeting record in `CeremonyStepExecution.output` — see
> `choreographer-observability-design.md`) and the evidence-gate work
> (#115–#120: step-level `output_contract` in ceremony YAML, evidence
> grounding validator, fenced-JSON tolerance, observability/authoring
> runbooks, chart NOTES banner).

As of 2026-05-14 the eight stack-readiness areas resolve as follows:

| # | Area | State |
|---|---|---|
| 1 | real Runtime execution | done (adapter + env-driven wiring) |
| 2 | typed external context input | done (typed `ExternalContextBundle` flowing trigger -> task -> deliberation) |
| 3 | structured, contract-validated council outputs | done (structured-output mode + deterministic `NoValidProposal` failure) |
| 4 | complete causal metadata propagation | done (Epic 5) |
| 5 | provider-backed council materialization | done (`DispatchingAgentFactory` wired with `noop`/`anthropic`/`openai`/`vllm` arms) |
| 6 | honest and durable transport semantics | done (AsyncAPI now declares plain core NATS; JetStream deferred) |
| 7 | real TLS / mTLS posture | done (gRPC server TLS in `none`/`server`/`mutual` modes; chart honest; Runtime client TLS shipped 2026-05-12; handshake-level server + mutual TLS tests shipped 2026-05-14) |
| 8 | stack-level end-to-end proofs | done (E2E covers seeded council, deliberation, causal metadata over NATS, `Orchestrate -> RuntimeExecutor -> stub-runtime`, `ExternalContextBundle` round-trip, positive structured-output `RunCouncilDecision` through the `stub-llm` sidecar, and OpenAI-shaped + vLLM-shaped provider paths; real external vLLM validation remains `make e2e-provider-vllm`) |

Two surfaces beyond the eight areas:

- **MCP stdio adapter** — `crates/choreo-mcp` ships a hand-rolled
  stdio MCP server that exposes every RPC of `underpass.choreo.v1`
  as a `choreo_*` tool. End-user docs live at
  [`docs/operations/mcp-stdio.md`](./operations/mcp-stdio.md); per-
  client snippets for Codex CLI and Claude Desktop live under
  `docs/operations/mcp/`. Foundation merged 2026-05-12; the
  distribution slice ships install + smoke scripts and a top-README
  link.
- **Downstream product integrations (PIR, payments incident response,
  custom agentic flows)** are **out of scope for this repo**. The
  product owns its own deliberation surface; Choreographer's job is to
  expose a clean, fully-typed gRPC API plus the MCP wrapping so any
  agentic consumer can drive it.

Genuinely open work after the 2026-05-18 compose-E2E and consumer-smoke
positive-path work is narrower: keep the operator-facing
`make e2e-provider-vllm` flow for real external vLLM endpoints and
finish the release-candidate publication gates.
Milestones A, B, C, D, and E are all complete as of 2026-05-14. The
`choreo-mcp` Git-install UX and fixture/live smoke path are present;
crates.io publication remains release-candidate work tracked in
`docs/product-publication-checklist.md`.

## Milestone F — ceremony engine + judge scoring (landed 2026-06-06 → 2026-06-09)

Two stack capabilities beyond the original eight areas shipped to `main`:

- **Ceremony engine.** `RunCeremony` (RPC #17 of `underpass.choreo.v1`)
  executes a YAML-defined ceremony as a finite-state machine —
  states/steps/transitions/guards/roles — with pluggable step handlers,
  multi-agent panels, a run-time context brief injected into each agent's
  task, and a Mermaid sequence diagram in the response. Catalog ceremonies
  (daily standup, technical debate, sprint planning, speaker + Q&A) run
  end-to-end in CI, driven by the `choreo-run-ceremony` operator tool
  (`crates/choreo-e2e-runner`). Domain in
  `crates/choreo-core/src/entities/ceremony_definition.rs` +
  `ceremony_instance.rs`; use case in
  `crates/choreo-app/src/usecases/run_ceremony_use_case.rs`.
- **LLM-as-judge scoring.** Winner selection is a pluggable `ScoringPort`.
  The default ranks by validator pass-fraction (which ties valid proposals
  and picks an arbitrary winner); the opt-in `JudgeAwareScoring`
  (`CHOREO_JUDGE_ENABLED`, `CHOREO_JUDGE_THRESHOLD`) is fed by an
  `LlmJudgeValidator` that rates intrinsic quality and makes that the
  score. Fail-fast: enabling it without a vLLM endpoint/model refuses to
  start. Persisted durably in the `underpass-runtime` Helm overlay and
  proven live against an in-cluster vLLM/Gemma endpoint.

Remaining release-candidate work (crates.io publication, real external
vLLM gates) is unchanged and tracked in
`docs/product-publication-checklist.md`.

The recommended remaining execution order is:

- real-provider validation as an operator-run E2E (`make e2e-provider-vllm`)
- release-candidate publication gates

## Out of scope

This backlog does not include:

- moving any context system's graph semantics into Choreographer
- making Choreographer domain-specific (payments, incidents, …)
- implementing any downstream product (PIR, payments incident
  response, etc.) in this repository

## Priorities

### P0 — hard blockers (cleared)

No repo-owned P0 blockers remain as of 2026-05-14. The former P0
items are now cleared:

- dedicated consumer-facing RPC surface (Epic 9)
- stack E2E proof with Runtime executor, provider-shaped council,
  `ExternalContextBundle` round-trip, and schema-validated structured
  output in the same compose stack (Epic 11)

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
  exposes every RPC of `underpass.choreo.v1` as MCP tools over
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
end-to-end with Git-install UX; crates.io publication required the
later vendored-proto distribution slice.

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

### Epic 2. External context boundary

Status: done (caller-materialized, context-provider-agnostic input)

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
(PR #43). A Choreographer-owned adapter for any specific context
system remains intentionally out of scope unless a concrete product
needs that boundary.

#### Deliverables

1. define one explicit context ingestion boundary:
   - caller fetches context and passes it to Choreographer
   - a future product-specific adapter can fetch context through a new
     port if needed
2. choose one as the production path for the first downstream integration
3. define a stable structured bundle shape for expert councils:
   - incident summary
   - prior findings
   - prior decisions
   - evidence references
   - failed remediations
4. make that bundle addressable and testable

#### Recommendation

Prefer caller-materialized context:

- the consumer remains the owner of its context system
- Choreographer remains domain-agnostic
- the integration boundary is cleaner

That means Choreographer must still gain a first-class notion of
"structured external context bundle", but it does not have to own
context-provider transport.

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

Status: done end-to-end (server + client + handshake-level
integration tests).

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

Status: done for the repo-owned stack E2E path as of 2026-05-14.
The compose stack covers the Runtime executor via `stub-runtime`, the
positive structured-output path via `stub-llm`, and both OpenAI-shaped
and vLLM-shaped provider adapters. A real external vLLM endpoint
remains an operator-run validation through `make e2e-provider-vllm`.

Current state:

- `crates/choreo-e2e-runner/src/main.rs` dispatches the selected
  scenarios; `crates/choreo-e2e-runner/src/scenarios/` contains the
  assertions against a real gRPC + NATS stack with stub-runtime +
  stub-llm sidecars:
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
  9. The same Report-contract success path runs through a `vllm`-kind
     agent descriptor pointed at the same `stub-llm` sidecar. This
     proves the vLLM-shaped adapter path in the same compose run; a
     real external vLLM endpoint remains covered by
     `make e2e-provider-vllm`.
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
  that path. Scenarios 8 and 9 dynamically register `openai` and
  `vllm` agent descriptors pointing at the `stub-llm` sidecar so the
  positive structured-output path is exercised without depending on a
  real external provider. Real-provider councils against vLLM remain
  exercised separately by `make e2e-provider-vllm`.

Relevant code:

- [`crates/choreo-e2e-runner/src/main.rs`](../crates/choreo-e2e-runner/src/main.rs)
- [`crates/choreo-e2e-runner/src/scenario_selection.rs`](../crates/choreo-e2e-runner/src/scenario_selection.rs)
- [`crates/choreo-e2e-runner/src/scenarios/`](../crates/choreo-e2e-runner/src/scenarios/)
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
- real council → ✅ scenario 9 via the vLLM adapter against the
  `stub-llm` OpenAI-compatible sidecar; real external vLLM remains
  an operator-run validation via `make e2e-provider-vllm`.
- validated structured result → ✅ scenario 6 covers the rejection
  path (JsonSchemaValidator fires, `error_kind =
  "deliberation.no_valid_proposal"` on the bus, `FailedPrecondition`
  on gRPC); scenario 8 (2026-05-14) covers the positive path via the
  `stub-llm` sidecar — `RunCouncilDecision` in Strict mode returns
  a Report-shaped winner that validates against the canonical
  schema.
- runtime execution → ✅ scenario 5 (stub-runtime).

#### Remaining follow-ups

- compose-level operations doc (`docs/operations/compose-e2e.md`):
  done 2026-05-18. It documents the compose-shaped scenarios, `stub-runtime`,
  `stub-llm`, Report schema, provider-shaped OpenAI/vLLM paths, and
  when to use `make e2e-compose` versus `make e2e-provider-vllm`.
- optional consumer-smoke positive-path mode: done 2026-05-18.
  `--chain positive-path` registers an `openai` or `vllm` agent
  against an OpenAI-compatible endpoint and validates a strict Report
  winner. The default smoke still targets the NoopAgent rejection path.

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

#### Remaining gaps (outside Epic 12's shipped surface)

- **Bundle round-trip** (`bundle_seam_documented`) remains `Skipped`
  in the consumer-smoke harness by design; Epic 11 scenario 7 already
  proves the stack-level round-trip in `make e2e-compose`.
- **Positive structured-output path** is opt-in. Chain 2 still proves
  rejection against the default NoopAgent stack, and
  `--chain positive-path` registers an `openai` or `vllm` agent
  against a consumer-supplied OpenAI-compatible endpoint so
  `report_payload_validates` can pass.
- **Provider-runner E2E merged into `make e2e-compose`** is done via
  scenario 9. `make e2e-provider-vllm` remains for validating a real
  external vLLM endpoint.

#### Relevant code

- [`crates/choreo-consumer-smoke/`](../crates/choreo-consumer-smoke/)
- [`docs/operations/consumer-smoke.md`](./operations/consumer-smoke.md)

### Epic 13. MCP stdio adapter

Status: done (foundation 2026-05-12; distribution 2026-05-14).

Current state:

- `crates/choreo-mcp` exposes every RPC of `underpass.choreo.v1` as
  a `choreo_*` MCP tool (17 tools 1:1 with the gRPC service).
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
- 7 env vars (`CHOREO_MCP_BACKEND`, `CHOREO_MCP_GRPC_ENDPOINT`, and
  5 `CHOREO_MCP_GRPC_TLS_*`) with the same auto-detection pattern as
  the sibling rehydration-mcp.
- 21 unit tests + workspace clippy clean.

Distribution UX slice (done 2026-05-14; registry-readiness extended
2026-05-18):

- `scripts/mcp/install-choreo-mcp.sh` — registry install wrapper by
  default (`cargo install choreo-mcp`), with
  `CHOREO_MCP_INSTALL_MODE=git` fallback for unreleased branches,
  tags, and revisions.
- `scripts/mcp/choreo-stdio-smoke.sh` — one `tools/call` + grep marker
  for both fixture and live modes.
- `docs/operations/mcp-stdio.md` — canonical user-facing UX.
- `docs/operations/mcp/codex.md`, `docs/operations/mcp/claude-desktop.md`
  — per-client config snippets.
- `crates/choreo-mcp/README.md` — developer-oriented twin.
- top-level `README.md` link to `docs/operations/mcp-stdio.md`.
- `crates/choreo-mcp-proto` — vendored proto crate so `choreo-mcp`
  can publish without depending on the internal `choreo-proto` crate.
- `scripts/ci/publish-dry-run.sh` and tag-gated publish jobs in
  `.github/workflows/publish-distribution.yml`.

Relevant code:

- [`crates/choreo-mcp/`](../crates/choreo-mcp/)
- [`docs/operations/mcp-stdio.md`](./operations/mcp-stdio.md)

#### Deliverables

Met:

1. Git fallback install path for unreleased `choreo-mcp` builds.
2. Fixture and live stdio smoke script.
3. End-user docs plus Codex CLI and Claude Desktop snippets.
4. crates.io publication readiness: vendored proto crate, tag-gated
   publish jobs, and per-PR publish dry-run.

Open release-candidate work:

1. Publish `choreo-mcp-proto` and then `choreo-mcp` from the first
   `v*` tag.
2. Verify registry install path (`cargo install choreo-mcp`) after the
   crates.io publish jobs complete.

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

**Cleared 2026-05-14.** Epics 6, 7, and 8 are all done end-to-end,
including the server and mutual TLS handshake integration tests.

### Milestone D — Consumer-facing surface

Must finish:

- Epic 9
- Epic 11

Exit condition:

- consumers have a clean RPC surface to integrate with

**Cleared 2026-05-14.** Epic 9 shipped `RunCouncilDecision` plus
contract CRUD, and Epic 11 now runs the nine-scenario compose E2E
path with Runtime executor, external context round-trip, structured
output success, and OpenAI-shaped + vLLM-shaped provider paths.

### Milestone E — integration-ready

Must finish:

- Epic 12

Exit condition:

- it is reasonable to begin consumer integration work

**Cleared 2026-05-14.** Epic 12 done — consumer-smoke harness lives
at `crates/choreo-consumer-smoke` with two chains, a CLI, and two
integration tests against the in-process `GrpcFixture`. The remaining
`Skipped` assertions are explicit harness-scope choices: bundle
round-trip is covered by Epic 11 scenario 7, and Chain 2's positive
path needs a structured JSON agent instead of the default NoopAgent
stack.

## Suggested issue breakdown

### Cleared waves (2026-05-11)

- runtime gRPC executor adapter — done
- wire runtime executor in composition root — done
- execution metadata model — done (`TaskMetadata.execution_profile`)
- external context bundle type — done
- structured output mode — done
- incident / run / causation metadata propagation — done

### Follow-up waves

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
- Handshake-level integration tests shipped 2026-05-14:
  `tls_server_handshake.rs` and `tls_mutual_handshake.rs` mint
  self-signed leaves through the shared TLS fixture and exercise real
  server + mutual TLS handshakes.

#### Wave 5 — Consumer-facing surface

- `RunCouncilDecision` plus contract CRUD — done
- JSON Schema validator and canonical Report schema — done
- no bespoke `Report` / `HumanHandoffReport` entity by design; Report
  stays a JSON Schema output contract so product vocabulary does not
  enter the core
- e2e-runner drives Runtime executor plus OpenAI-shaped and vLLM-shaped
  provider paths — done

## Gating rule

The following rule should be treated as hard policy:

> No downstream product that requires structured, audited deliberation
> output should depend on Choreographer until Milestones A, B, and C
> are complete.

Status 2026-05-18: Milestones A, B, C, D, and E are complete.
`choreo-mcp` has a Git-install UX and smoke coverage; crates.io
publication remains release-candidate work.

Why the rule still applies in spirit even with B and C now cleared:

- consumer-specific production readiness still requires each downstream
  product to run its own smoke against its provider credentials,
  Runtime catalog, context source, and output contracts.

## Final recommendation

If only one sentence is carried forward from this document, it should be:

> Choreographer's job is to be a trustworthy, agnostic coordination
> product — structured external context input, structured outputs,
> honest transport, optional execution adapters, and agent-callable
> surfaces through gRPC and MCP. Downstream products integrate; they
> are not implemented here.
