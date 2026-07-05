# Stack Gap Analysis

Snapshot date: 2026-05-18 (ceremony engine + LLM-as-judge scorer added
2026-06-09)

This document records the remaining gaps for Choreographer as an
independent, domain-agnostic coordination product. The following repos
are referenced only as studied integrations and possible use cases:

- [underpass-runtime](https://github.com/underpass-ai/underpass-runtime)
- [rehydration-kernel](https://github.com/underpass-ai/rehydration-kernel)

The goal is not to market readiness. The goal is to state what is
wired, what is intentionally out of scope, and what must still be
proved by a downstream integration. Choreographer does not require
KMP, PIR, or any specific downstream product to be usable.

## Scope

This analysis is based on the local repository docs, chart, contracts,
CI scripts, and source code in this checkout. It does not assert the
current implementation state of sibling repositories.

## What Is Wired Today

- The Rust workspace keeps the intended dependency direction:
  `choreo-core` -> `choreo-app` -> `choreo-adapters` -> `choreo`.
- The gRPC service exposes and implements the full
  `underpass.choreo.v1` surface: the generic deliberation/orchestration
  RPCs, council and agent registry RPCs, trigger ingest, status/metrics,
  `RunCouncilDecision`, contract CRUD, and `RunCeremony` ceremony
  execution.
- Runtime execution is configurable: `CHOREO_EXECUTOR_KIND=noop|runtime`
  selects either `NoopExecutor` or the gRPC `RuntimeExecutor`.
- The context boundary is caller-materialized and agnostic.
  Choreographer accepts a typed `ExternalContextBundle` on tasks and
  triggers, regardless of whether the caller built it from KMP, RAG, a
  database, static fixtures, or another source.
- Structured council outputs are first-class through `OutputContract`,
  JSON-object validation, required fields, allowed string values, JSON
  Schema, bounded event-shape validation, and deterministic
  `NoValidProposal` failure.
- Provider-backed agent materialization is wired through
  `DispatchingAgentFactory`. `noop` is always accepted; `anthropic`,
  `openai`, and `vllm` require matching Cargo features and boot-time
  provider configuration.
- Broker semantics are declared honestly as plain core NATS pub/sub.
  The adapter does not claim JetStream durability, acknowledgement, or
  replay semantics.
- Server TLS/mTLS and Runtime client TLS are wired and covered by
  handshake-level integration tests.
- The compose E2E runner covers the core scenarios: seeded council,
  deliberation, missing-council delete, causal metadata over NATS,
  `Orchestrate -> RuntimeExecutor -> stub-runtime`, strict schema
  rejection, `ExternalContextBundle` round-trip, positive structured
  output through a stub OpenAI-compatible agent, and the same Report
  contract through the vLLM adapter shape — plus YAML ceremony execution
  (a council deliberation per step) and the catalog ceremonies (daily
  standup, technical debate, sprint planning, speaker + Q&A). The
  operator-facing map lives in
  [`operations/compose-e2e.md`](./operations/compose-e2e.md).
- Ceremony orchestration is wired: `RunCeremony` executes a YAML
  finite-state machine (states/steps/transitions/guards/roles) with
  pluggable handlers, multi-agent panels, context threading between
  steps, and a Mermaid diagram in the response.
- Winner scoring is a pluggable `ScoringPort`: uniform pass-fraction by
  default, or an opt-in LLM-as-judge (`CHOREO_JUDGE_ENABLED`) that ranks
  by intrinsic quality and fails fast without a vLLM endpoint/model.
- The stdio MCP adapter exposes all 17 gRPC RPCs as `choreo_*` tools
  and has fixture + live gRPC backends.

## Remaining Gaps

### 1. No Choreographer-owned Context Transport

The current production boundary is explicit and domain-neutral:
callers materialize context and pass an `ExternalContextBundle`.
That keeps the core clean, but it also means the repo does not prove a
direct transport integration to any specific context system.

If a downstream product requires Choreographer to fetch context itself,
that should be a new port, adapter, and E2E slice for that product.
Until then, the honest claim is caller-supplied context, not KMP client
ownership.

### 2. Real external provider validation is operator-run

The repo-owned E2E path uses `stub-llm` so CI can prove provider-shaped
adapter contracts without external credentials. A real vLLM endpoint is
covered by the manual `make e2e-provider-vllm` flow.

That is sufficient for repository CI, but a product deployment still
needs to run its own provider smoke with its model, credentials,
network policy, TLS posture, and latency budget.

### 3. Consumer-smoke positive path is opt-in

`choreo-consumer-smoke` exercises the public gRPC + optional NATS
surface and proves the strict rejection path against a NoopAgent stack.
The positive structured-output path now ships as
`--chain positive-path`: it registers an `openai` or `vllm` agent
against an OpenAI-compatible endpoint, runs `RunCouncilDecision` in
Strict mode, and flips `report_payload_validates` to `Passed`.

It intentionally remains opt-in because a real deployment must choose
its provider endpoint, model, credentials, network policy, TLS posture,
and latency budget.

### 4. Streaming remains phase-level

`StreamDeliberation` emits phase transitions and a final winner frame.
The proto shape reserves payload variants for proposal, critique, and
revision frames, but those per-turn frames are not emitted yet.

That is an honest product limitation for live agent UIs; synchronous
callers can use `Deliberate` or the buffered MCP wrapper today.

## Recommended Hardening Plan

1. Keep the consumer-smoke positive-path mode opt-in and require
   downstream deployments to supply their own OpenAI-compatible stub or
   provider endpoint.
2. Keep `make e2e-provider-vllm` as the explicit external-provider
   validation path and require downstream deployments to run it, or an
   equivalent provider smoke, before claiming provider readiness.
3. Only add a Choreographer-owned context adapter if a concrete product
   needs that ownership boundary. Otherwise keep caller-materialized
   context as the documented contract.
4. Treat per-proposal/per-critique/per-revision streaming as a separate
   additive feature, with tests that prove frame ordering and backpressure.

## Honest Current Position

The Choreographer is now a real gRPC + NATS + persistence + Runtime
executor + MCP application with structured-output validation, a
declarative ceremony engine, an optional LLM judge, deliberation-native
observability (Prometheus metrics + OTel traces of the debate), and
repo-owned stack E2E over stubs.

It is not a product integration by itself. Downstream products still
own their context materialization, provider credentials, Runtime tool
catalog, output contracts, and production smoke tests.
