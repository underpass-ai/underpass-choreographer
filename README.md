# Underpass Choreographer

Event-driven coordination plane for specialist agents. Domain-agnostic port of
the `swe-ai-fleet` orchestrator service to Rust.

Role in the Underpass platform:

- **Rehydration Kernel** — memory plane (context graph)
- **Choreographer** — event-driven coordination (this repo)
- **Underpass Runtime** — execution plane (governed tools)

The choreographer reacts to domain events, composes councils of agents,
runs deliberations, and publishes outcome events. It does **not** embed any
domain vocabulary (no stories, plans, roles hardcoded) — all that is injected
via configuration and proto messages.

## Workspace

| Crate | Purpose |
|---|---|
| `choreo-core` | Domain types, ports, events. No IO. |
| `choreo-app` | Use cases / application services. |
| `choreo-adapters` | NATS, gRPC clients, config, external integrations. |
| `choreo-proto` | Tonic-generated gRPC code (`underpass.choreo.v1`). |
| `choreo` | Binary: wires adapters, runs gRPC + NATS. |

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
  are **manual-dispatch only** — the per-PR gates
  (`clippy`, `test`, `contract`, `integration-nats`,
  `integration-postgres`, `container-image`, `helm-chart`) already
  cover the compile-and-unit surface; E2E is reserved for pre-
  release validation. Trigger via `gh workflow run e2e.yml`.

## Status

**What runs today** (enforced by CI, every claim is backed by a test or
gate in this repository):

- `choreo` binary starts, reads config from `CHOREO_*` env vars, and
  serves the full `underpass.choreo.v1` gRPC contract.
- Implemented RPCs: every RPC in the `underpass.choreo.v1` contract
  is backed by a use case — `Deliberate`, `StreamDeliberation`,
  `Orchestrate`, `CreateCouncil`, `ListCouncils`, `DeleteCouncil`,
  `GetDeliberationResult`, `ProcessTriggerEvent`, `GetStatus`,
  `GetMetrics`, `RegisterAgent`, `UnregisterAgent`. No RPC returns
  `UNIMPLEMENTED`. Caveats: (a) `RegisterAgent` currently materializes
  agents with `kind == "noop"`; provider-backed kinds land through
  richer `AgentFactoryPort` wirings in their respective feature
  slices. (b) `StreamDeliberation` emits phase transitions + a final
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

**What is *not* wired yet**:

- The wired `AgentFactoryPort` today only recognises `kind == "noop"`.
  Provider-specific factories (vLLM, Anthropic, OpenAI, …) exist as
  standalone adapters behind their Cargo features but are not yet
  composed into the binary's factory dispatch — that lands in a later
  slice.
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
