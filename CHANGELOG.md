# Changelog

All notable changes to the Underpass Choreographer are tracked here.

This repository has not cut a public `v*` tag yet. The workspace and
Helm chart currently carry version `0.1.0`; keep entries under
`Unreleased` until the release process in `docs/release.md` creates an
immutable tag and published artifacts.

The format follows the spirit of Keep a Changelog, with categories kept
short and factual. Do not add claims here unless the behavior is
implemented and covered by a committed gate, smoke test, or documented
operator command.

## Unreleased

### Added

- Embedded MCP backend and repo-local Codex plugin bundle. The isolated
  `choreo-mcp` build exposes only `choreo_run_ceremony`, completes the MCP
  stdio handshake and a real ceremony without gRPC/protobuf, and is covered by
  direct, process, dependency-boundary, and plugin-launcher smoke tests.
- `choreo-embedded`, an in-process distribution of the ceremony engine with
  local defaults, injectable domain ports, an async host-callback step adapter,
  incremental human-active operations, and no required gRPC, NATS or Postgres
  dependency. It uses the same domain and application use cases as the
  deployable binary and carries the same workspace release version.
- Observability — Prometheus metrics: the binary exposes the operational
  metric families at `GET /metrics` (HTTP port `8080`) through a
  `MetricsRecorderPort` (core) and a `PrometheusMetricsRecorder` adapter
  (explicit registry, no global recorder), alongside the original
  `Statistics`-backed counters. Covers deliberation quality (duration,
  winner-score distribution, terminal outcome), the LLM judge (latency,
  score, errors by kind, discrimination, tokens, scoring mode), the
  proposing providers (request latency, errors, in-flight gauge, tokens),
  the ceremony engine (outcomes, durations, per-step status, blocked
  transitions), NATS publish (latency + errors), and the Postgres pool.
  Wired through a `with_metrics` opt-in so only the composition root
  installs the live recorder. Covered by unit tests.
- Observability — distributed tracing: with the `otel` feature and an OTLP
  endpoint configured, a deliberation is exported as one trace whose span
  events carry the debate itself — proposals, peer critiques, validator
  verdicts, judge scores, and the winning rationale — over mutual TLS to
  the in-cluster collector.
- Ceremony "meeting record": the winning contribution of each ceremony step
  is returned on the `RunCeremony` response (`CeremonyStepExecution.output`),
  so the full prose outcome of a run is a first-class API artifact.
- LLM-as-judge scoring: an optional `JudgeAwareScoring` strategy fed by an
  `LlmJudgeValidator` that ranks deliberation proposals by intrinsic
  quality instead of validator pass-fraction. Opt-in via
  `CHOREO_JUDGE_ENABLED` (with `CHOREO_JUDGE_THRESHOLD`), reusing the vLLM
  endpoint/model; fail-fast wiring and a Helm chart guard refuse a
  judge-on-without-vLLM configuration. Covered by unit tests and a
  provider-backed E2E.
- Ceremony engine: `RunCeremony` executes YAML-defined ceremonies as
  finite-state machines (states, steps with pluggable handlers, guarded
  transitions, roles), with multi-agent panels, a run-time context brief
  injected into each agent's task, and a Mermaid sequence diagram in the
  response. Catalog ceremonies (daily standup, technical debate, sprint
  planning, speaker + Q&A) run end-to-end in CI, driven by the
  `choreo-run-ceremony` operator tool.
- Helm persistence for the judge + vLLM provider env in the
  `underpass-runtime` overlay, guarded by a CI marker and a chart `fail`
  assertion enforcing the judge↔vLLM coupling.
- Product usability and publication planning:
  `docs/product-usability-publication-plan.md` and
  `docs/product-publication-checklist.md`.
- Explicit documentation that Choreographer is agnostic and
  independently usable; KMP, PIR, Runtime, and other projects are study
  cases or optional integrations, not required dependencies.
- Local no-external-service quickstart:
  `CHOREO_NATS_ENABLED=false just run`.
- MCP fixture and live-gRPC quickstarts, plus examples for
  `CreateCouncil`, `RegisterAgent`, `RegisterContract`,
  `RunCouncilDecision`, and `Orchestrate`.
- Repo-owned compose E2E guide covering the compose scenarios, stubs,
  Report schema, and provider-shaped OpenAI/vLLM paths.
- E2E runner scenario selection through `CHOREO_E2E_SCENARIOS`, with
  groups for `compose`, `cluster-connectivity`, `runtime-stub`, and
  `structured-output`.
- Consumer smoke `positive-path`, including Report contract
  registration, Strict-mode `RunCouncilDecision`, provider-shaped
  OpenAI/vLLM agents, and optional NATS causality assertions.
- Helm install profiles for minimal standalone, embedded NATS,
  Postgres DSN from Secret, provider environment Secret wiring, and the
  Underpass Runtime executor profile.
- Kubernetes deployment guide covering minimal install, embedded NATS,
  gRPC TLS/mTLS, Postgres secret sourcing, provider environment
  secrets, Runtime executor TLS, and operator smokes.
- Support matrix covering Rust toolchain, image tags, chart versions,
  provider adapters, and Kubernetes posture.
- Upgrade, rollback, and operator deploy verification runbooks for
  pinned images, Secret references, OCI chart installs, and smoke
  checks.
- Security policy covering supported scope, private vulnerability
  reporting, coordinated disclosure, deployment hardening, and secret
  containment.

### Changed

- Kubernetes E2E jobs default to cluster-connectivity scenarios instead
  of running fixture-only stub scenarios against real deployments.
- `make e2e-compose` keeps the full compose group as the fixture-backed
  end-to-end path.
- Helm render checks now cover pinned-image enforcement, TLS secret
  validation, embedded NATS wiring, Runtime executor failure modes,
  Postgres Secret rendering, and provider env Secret rendering.

### Validation

- MCP catalog parity is checked against the gRPC proto surface.
- Compose E2E has been validated through all nine scenarios, including
  structured Report output and provider-shaped paths.
- Kubernetes smoke has been validated with the selected
  cluster-connectivity group.
- `choreo-consumer-smoke` has been validated for rejection-path and
  positive-path behavior against local Choreographer, NATS, and
  `choreo-stub-llm`.

### Security

- Provider credentials, Postgres DSNs, and TLS materials are documented
  as secret-managed inputs, not values-file or descriptor content.
- Chart gates assert hardened pod defaults and prevent accidental
  rendering of literal Postgres DSNs in the Secret-backed profile.

### Known Limits

- No public immutable `v*` tag, release image, OCI chart, or crates.io
  package has been cut yet; current published `sha-*` images are RC
  smoke artifacts, not stable release artifacts.
- `choreo-mcp` can only be published after `choreo-mcp-proto v0.1.0`
  is available in crates.io.
- Provider-backed positive smokes are validated with deterministic
  OpenAI-compatible stubs unless a real provider is explicitly wired by
  the operator.
- The Helm chart does not manage Ingress, provider egress allow-lists,
  or multi-replica/state coordination beyond the documented single
  replica posture.

## 0.1.0 - Pending

- Initial pre-release version present in `Cargo.toml` and
  `charts/choreographer/Chart.yaml`.
- No immutable `v0.1.0` tag is present in this checkout yet.
