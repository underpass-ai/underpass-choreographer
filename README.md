# Underpass Choreographer

> **⚠️ Project paused — this repository has moved.**
> Active development continues at **[underpass-ai/made](https://github.com/underpass-ai/made)** (MADE by Underpass — ceremony-driven multi-agent deliberation).
> This repository is kept for historical reference and is no longer maintained.

> Part of [Underpass AI](https://underpassai.com) — memory and execution infrastructure for reliable AI agents.

Event-driven coordination plane for councils of specialist agents. It runs
structured deliberations (propose → peer-critique → revise → validate →
score → winner) and longer declarative YAML ceremonies as explicit state
machines, enforces output contracts on what agents produce, can score with
an LLM judge, and ships observability that shows *why* an answer won.
Domain- and provider-agnostic (vLLM, Anthropic, OpenAI, local, rule-based,
human-in-the-loop). Kubernetes-first.

Lineage: started as a domain-agnostic Rust port of the `swe-ai-fleet`
orchestrator service, and has since grown well beyond that port.

## What it does

- **Structured deliberation** — a strict one-way FSM per council run
  (`Proposing → Revising → Validating → Scoring → Completed`) with
  deterministic peer critique between agents and a bounded, total-order
  score for ranking.
- **Declarative ceremonies** — longer multi-step meetings defined in YAML as
  explicit state machines (states, transitions, guards, retries, leases).
  The winning contribution of every step is an **API artifact** — a meeting
  record with a Mermaid diagram of the conversation — not a log line.
- **Output contracts** — JSON Schema plus field rules enforced by shipped
  validators, with deterministic rejection (`NoValidProposal`) when no
  proposal satisfies the contract: unsupported output does not become a
  decision.
- **Optional LLM-as-judge** — judge-aware scoring with fail-fast
  configuration, plus a judge *discrimination* metric that tells you whether
  the judge actually re-ranks proposals or just burns tokens.
- **Deliberation-native observability** — every deliberation is a replayable
  OpenTelemetry trace (the debate itself, span by span, exported over mTLS)
  and Prometheus metrics designed for this domain: winner-score
  distribution, `NoValidProposal` rate, per-step ceremony outcomes. See
  [`docs/choreographer-observability-design.md`](docs/choreographer-observability-design.md).
- **Two surfaces** — a contract-first gRPC API, and a stdio MCP server
  exposing the same RPCs 1:1 to coding agents (Codex CLI, Claude Desktop),
  with embedded-only ceremony controls and read-only Markdown reports where no
  remote RPC exists.

## Verify capabilities before making claims

“Choreographer supports X” is incomplete unless the statement identifies the
running distribution and backend, the tools exposed by that executable, who
performs external work, and which state survives a restart.

For MCP sessions, inspect the active `tools/list` result and start with
`choreo_discover_capabilities` when it is available. Its backend-filtered
catalog is authoritative for the installed executable surface. Discovery does
not prove that a real step handler, durable store, credentials, or external
authority have been configured.

See the
[capability-verification runbook](docs/operations/capability-verification.md)
before documenting or automating an integration.

## The Underpass platform

Three planes, three repos:

| Plane | Repo | Brand name | Role |
|---|---|---|---|
| Memory + context | [`rehydration-kernel`](https://github.com/underpass-ai/rehydration-kernel) | **Underpass KMP** (Kernel Memory Plane / Kernel Memory Protocol) | One possible producer of LLM-ready context bundles from a typed knowledge graph. |
| Coordination | this repo | **Underpass Choreographer** | Composes councils, runs deliberations, validates outputs, hands winners to an executor. |
| Execution + governed tools | [`underpass-runtime`](https://github.com/underpass-ai/underpass-runtime) | **Underpass Runtime** | Sessions, governed tool invocations, artifacts, policy decisions. |

Choreographer is agnostic and independently usable. It does not depend
on KMP, PIR, or any downstream product. It accepts caller-supplied
`ExternalContextBundle`s from any context source; KMP is one studied
producer, not a required dependency. Runtime execution is optional via
the `RuntimeExecutor` adapter. Choreographer does **not** embed any
product vocabulary (no stories, plans, incidents, claims hardcoded) —
all that is injected via configuration and proto messages.

## Start here

- [`docs/index.md`](docs/index.md) — full navigation hub for every
  doc in this repo, grouped by audience.
- [`docs/dev-loop.md`](docs/dev-loop.md) — local iteration loop,
  every command mirrors a CI gate.
- [`docs/release.md`](docs/release.md) — versioning + cut-a-release
  checklist.
- [`CHANGELOG.md`](CHANGELOG.md) — unreleased changes and release-note
  discipline.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contribution workflow,
  required gates, contract rules, and PR expectations.
- [`SECURITY.md`](SECURITY.md) — supported security scope,
  vulnerability reporting, and deployment hardening baseline.
- [`docs/operations/deploy-kubernetes.md`](docs/operations/deploy-kubernetes.md)
  — Helm install guide, including minimal standalone install and
  embedded NATS, TLS/mTLS, Postgres secret, provider env secrets,
  and Runtime executor options.
- [`docs/operations/support-matrix.md`](docs/operations/support-matrix.md)
  — supported Rust toolchain and release-support rules.
- [`docs/PRINCIPLES.md`](docs/PRINCIPLES.md) — honesty discipline.
- [`docs/experiments/`](docs/experiments/) — append-only lab
  notebook (baselines, scale sweeps, null results).
- [`docs/operations/mcp-stdio.md`](docs/operations/mcp-stdio.md) —
  installable stdio MCP adapter exposing the gRPC API to coding
  agents (Codex CLI, Claude Desktop).
- [`docs/embedded-choreographer.md`](docs/embedded-choreographer.md) —
  in-process ceremony engine, host adapter injection, and the boundary
  between embedded and deployable distributions.
- [`docs/operations/codex-plugin.md`](docs/operations/codex-plugin.md) —
  cumulative test ladder and local Codex plugin packaging.
- [`docs/operations/ceremony-authoring-runbook.md`](docs/operations/ceremony-authoring-runbook.md)
  — writing ceremony YAML: schema keys, rounds, sizing, output
  contracts, and verification.
- [`docs/operations/observability-runbook.md`](docs/operations/observability-runbook.md)
  — wiring traces, metrics, and logs in a deployment.
- [`docs/backlog.md`](docs/backlog.md) — epic-by-epic readiness
  status + session log.
- `justfile` at the repo root — `just` lists every recipe.

## Run Locally Without External Services

```sh
CHOREO_NATS_ENABLED=false just run
```

This starts the Choreographer binary with in-memory persistence,
noop messaging, and the default noop executor. It serves the gRPC API
on `localhost:50055` without requiring NATS, Postgres, Runtime, KMP,
PIR, or provider credentials. For an immediately exercisable local
council, add `CHOREO_SEED_SPECIALTIES=triage`.

If `just` is not installed:

```sh
CHOREO_NATS_ENABLED=false cargo run --locked -p choreo
```

For MCP client wiring without a running Choreographer:

```sh
CHOREO_MCP_BACKEND=fixture choreo-mcp
```

That starts the stdio MCP adapter in fixture mode. See
[`docs/operations/mcp-stdio.md`](docs/operations/mcp-stdio.md) for
terminal smoke commands and live gRPC configuration.

To execute the real ceremony engine over MCP without a service:

```sh
CHOREO_MCP_BACKEND=embedded \
  cargo run --locked -p choreo-mcp --no-default-features --features embedded
```

To point MCP at the local Choreographer from a second terminal:

```sh
CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055 choreo-mcp
```

## Workspace

| Crate | Purpose |
|---|---|
| `choreo-core` | Domain types, ports, events. No IO. |
| `choreo-app` | Use cases / application services. |
| `choreo-adapters` | NATS, gRPC clients, config, external integrations. |
| `choreo-embedded` | In-process ceremony facade with local defaults and injectable port adapters. |
| `choreo-proto` | Tonic-generated gRPC code (`underpass.choreo.v1`). |
| `choreo-mcp-proto` | Vendored `underpass.choreo.v1` proto crate used to publish `choreo-mcp` independently. |
| `choreo` | Binary: wires adapters, runs gRPC + NATS. |
| `choreo-mcp` | Stdio MCP adapter with live-gRPC, embedded ceremony, and deterministic fixture backends. |
| `choreo-e2e-runner` | Operator + E2E driver binaries (incl. `choreo-run-ceremony`) that exercise the service over its public gRPC surface. |
| `choreo-tests-integration` | Integration tests backed by testcontainers-managed services. Not shipped. |
| `choreo-consumer-smoke` | Standalone NATS consumer smoke check. Not shipped. |

## Principles

This project follows the same discipline as its siblings
[`underpass-runtime`](https://github.com/underpass-ai/underpass-runtime) and
[`rehydration-kernel`](https://github.com/underpass-ai/rehydration-kernel):

- **Honest documentation.** No marketing claims in code, docs, or commit
  messages. If a capability is not implemented and exercised, it is not
  described as if it were. "Planned", "in progress", and "prototype" are
  said out loud.
- **Everything is demonstrable and measurable.** Any claim about
  behaviour, performance, or quality must be backed by a reproducible
  test, benchmark, or experiment that lives in this repository and runs
  in CI. No hand-wave numbers. No unsubstantiated quality claims.
- **Scientific method for iteration.** Changes that alter behaviour
  follow: (1) hypothesis, (2) experiment design, (3) measurement,
  (4) result, (5) conclusion — recorded under `docs/experiments/`.
  We keep null results too.
- **Use-case agnostic.** No vocabulary of any particular domain (software
  engineering, clinical, supply chain, …) leaks into the Choreographer.
- **Provider-agnostic.** No LLM vendor (vLLM, Anthropic, OpenAI, local,
  rule-based, human-in-the-loop) is privileged over any other.
- **API-first.** The gRPC (`crates/choreo-proto/proto/…`) and AsyncAPI
  (`specs/asyncapi/…`) specifications are the source of truth. Generated
  code follows; breaking changes must be detected by the contract gate
  before any Rust code compiles.
- **Distribution via containers and Helm.** Images are built under
  `Dockerfile` (podman and docker supported); deployment is via the
  Helm chart under `charts/choreographer/`.
  - Pinned images only (a `latest` tag is refused unless
    `development.allowMutableImageTags` is set)
  - Non-root pod + container security contexts (runAsNonRoot,
    readOnlyRootFilesystem, `ALL` capabilities dropped,
    `seccompProfile: RuntimeDefault`)
  - `automountServiceAccountToken: false` (the binary does not
    call the Kubernetes API)
  - emptyDir on `/tmp` so any library tempfile write survives
    the read-only root filesystem
  - `networkPolicy.enabled` opt-in restricts inbound to the pod's
    declared ports and outbound to DNS, NATS, Postgres, and OTLP
    (plus any extra rules operators add)
  - `CHOREO_POSTGRES_URL` sourceable via `valueFrom.secretKeyRef`
    so the DSN never lands in values files
  - Optional `PodDisruptionBudget` gated on `pdb.enabled`
  - Chart-render CI (`scripts/ci/helm-lint.sh`) exercises every
    hardening feature and refuses a manifest that drops one.

### Quality gates

- Unit coverage: **minimum 80 % of lines**, target band 80–90 %, enforced
  by `scripts/ci/rust-coverage.sh`.
- Integration tests: **testcontainers-backed**, real services per run
  (no mocks at the integration boundary).
- End-to-end tests: a runner container drives scenarios either via
  `docker compose` or as a Kubernetes `Job` against a kind cluster
  with the Helm chart installed (contract-true path). Both paths
  are **manual only**, launched from the repository with
  `make e2e-compose` or `make e2e-kubernetes` — the per-PR gates
  (`clippy`, `test`, `contract`, `integration-nats`,
  `integration-postgres`, `container-image`, `helm-chart`) already
  cover the compile-and-unit surface; E2E is reserved for pre-
  release validation.

## Status

**What runs today** (enforced by CI, every claim is backed by a test or
gate in this repository):

- `choreo` binary starts, reads config from `CHOREO_*` env vars, and
  serves the full `underpass.choreo.v1` gRPC contract.
- Implemented RPCs: every RPC in the `underpass.choreo.v1` contract
  is backed by a use case — `Deliberate`, `StreamDeliberation`,
  `GetDeliberationResult`, `Orchestrate`, `CreateCouncil`,
  `ListCouncils`, `DeleteCouncil`, `RegisterAgent`,
  `UnregisterAgent`, `ProcessTriggerEvent`, `RunCouncilDecision`,
  `RegisterContract`, `ListContracts`, `DeleteContract`, `RunCeremony`,
  `GetStatus`, and `GetMetrics`. No RPC returns `UNIMPLEMENTED`. Caveats:
  (a) provider-backed `RegisterAgent` kinds require the matching Cargo
  feature and boot-time credentials; `noop` is always available.
  (b) `StreamDeliberation` emits phase transitions + a final
  `DeliberationResult` frame, not per-proposal/critique/revision
  events.
- Optional NATS messaging: when `CHOREO_NATS_ENABLED=true`, the service
  publishes all 5 outbound events (`choreo.task.*`,
  `choreo.deliberation.completed`, `choreo.phase.changed`) and
  consumes inbound `TriggerEvent`s from `choreo.trigger.>`.
  Otherwise a no-op messaging adapter is wired.
- Optional seeding: `CHOREO_SEED_SPECIALTIES=triage,reviewer`
  registers one `NoopAgent` and one single-agent council per specialty
  so a fresh deployment is immediately exercisable end-to-end.
- Ceremony orchestration: `RunCeremony` executes a YAML-defined ceremony
  as a finite-state machine — states, steps with pluggable handlers,
  guarded transitions, and roles. A step can drive a full council
  deliberation; prior turns thread into later steps' briefs; the
  response carries a Mermaid sequence diagram of the conversation.
  Catalog ceremonies (daily standup, technical debate, sprint planning,
  speaker + Q&A) run end-to-end in CI.
- Scoring: the winner of a deliberation is chosen by a pluggable
  `ScoringPort`. The default ranks by validator pass-fraction; an
  optional LLM-as-judge (`CHOREO_JUDGE_ENABLED`, with
  `CHOREO_JUDGE_THRESHOLD`) instead rates each proposal's intrinsic
  quality and makes that the score. Disabled by default, and fail-fast:
  enabling it without a vLLM endpoint/model refuses to start rather than
  silently degrading.

**Persistence**:

- When `CHOREO_POSTGRES_URL` is set, deliberations, councils, the
  agent registry, and operational statistics persist to Postgres;
  otherwise the in-memory defaults are wired. Persistence choice is
  binary: every backing is either Postgres or in-memory together, so
  no replica reads from a split source of truth. Migrations apply on
  startup — a fresh cluster is immediately exercisable. Schema lives
  under `crates/choreo-adapters/migrations/postgres/`.
- Agents persist as descriptors (`id`, `specialty`, `kind`,
  `attributes`); live `AgentPort` handles are rehydrated through the
  wired `AgentFactoryPort` on resolve, so no pickled provider state
  crosses the database boundary.
- Statistics counters use an `INSERT ... ON CONFLICT DO UPDATE
  ... x = x + 1` protocol so concurrent replicas accumulate into the
  same row without a read-modify-write race — verified by a 50-
  concurrent-record integration test.

**Agent factory** (provider-backed materialization):

- The binary wires `DispatchingAgentFactory`, which materializes
  `kind == "noop"` unconditionally plus any provider whose Cargo
  feature is compiled in AND whose credentials are present at boot:
  - `agent-anthropic` + `CHOREO_ANTHROPIC_API_KEY` → `kind=anthropic`
  - `agent-openai` + `CHOREO_OPENAI_API_KEY` → `kind=openai`
  - `agent-vllm` + `CHOREO_VLLM_MODEL` + `CHOREO_VLLM_ENDPOINT` (+ optional
    `CHOREO_VLLM_BEARER_TOKEN`) → `kind=vllm`
  Per-descriptor overrides via `provider.model`, `provider.endpoint`,
  `provider.max_tokens` attributes on the registered descriptor.
  Startup log emits `agent_kinds=` listing every kind the binary will
  accept on `RegisterAgent`.

**Caveats and observability**:

- **Prometheus metrics**: the binary serves an operational metric
  surface at `GET /metrics` (HTTP port `8080`) through a domain
  `MetricsRecorderPort` and a `PrometheusMetricsRecorder` adapter
  (explicit registry, no global recorder), exposed alongside the
  original `Statistics`-backed counters. The families cover
  deliberation quality (duration, winner-score distribution, terminal
  outcome), the LLM judge (latency, score, errors by kind,
  **discrimination** — does the judge re-rank or just burn tokens? —,
  tokens, scoring mode), the proposing providers (request latency,
  errors, in-flight gauge for vLLM serial saturation, tokens), the
  ceremony engine (outcomes, durations, per-step status, blocked
  transitions), NATS publish, and the Postgres pool. Recording is a
  synchronous, infallible side-channel that can never block or fail a
  deliberation. See `docs/choreographer-observability-design.md` for
  the catalogue, alerts, and dashboard design. (Deferred so far: gRPC
  front-door RED — already covered by the request traces — and
  per-query Postgres latency.)
- `StreamDeliberation` streams phase transitions only; per-proposal,
  per-critique, and per-revision streaming arrives in a later slice.
- Distributed tracing: the core use cases, gRPC handlers, NATS
  inbound subscriber, and `AutoDispatchService` emit `#[tracing::
  instrument]` spans with domain fields (`task_id`, `specialty`,
  `event_id`, `agent_id`, `kind`). A regression test pins the
  `deliberate` span name and fields.
- W3C Trace Context propagation **across NATS**: every outbound
  event carries a `traceparent` header stamped by the publisher
  (`TraceContext::generate()` when no upstream context is present).
  The inbound subscriber extracts `trace_id` and `span_id` from the
  header and surfaces them as fields on the `nats.trigger.inbound`
  span. Integration-tested against a real NATS container.
- W3C Trace Context propagation **across gRPC** (opt-in via the
  `otel` Cargo feature): every RPC handler calls
  `link_span_to_metadata`, which reads `traceparent` from request
  metadata and sets it as the OTel parent context of the current
  tracing span. Integration-tested via a `tracing-opentelemetry`
  bridge.
- **OTLP exporter** (opt-in via the `otel` feature + runtime
  `CHOREO_OTLP_ENDPOINT`): when both are present the binary
  installs a batching OTLP/gRPC exporter and layers the
  `tracing-opentelemetry` bridge into the subscriber, so every
  instrumented span ships to the configured collector with real
  OTel trace/span IDs. Feature off → the binary has zero OTel
  dependency surface. Endpoint unset → the exporter is not wired
  (no silent background connections).

See `docs/experiments/` for anything beyond these bullet points.

## Legal

Copyright © 2026 Tirso García Ibáñez.

This repository is part of the Underpass AI project.
Licensed under the Apache License, Version 2.0, unless stated otherwise.

Redistributions and derivative works must preserve applicable copyright,
license, and NOTICE information.

Original author: [Tirso García Ibáñez](https://github.com/tgarciai) ·
[LinkedIn](https://www.linkedin.com/in/tirsogarcia/) ·
[Underpass AI](https://github.com/underpass-ai)
