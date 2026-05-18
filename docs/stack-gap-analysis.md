# Stack Gap Analysis

Snapshot date: 2026-05-18

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
  `RunCouncilDecision`, and contract CRUD.
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
- The compose E2E runner covers nine scenarios: seeded council,
  deliberation, missing-council delete, causal metadata over NATS,
  `Orchestrate -> RuntimeExecutor -> stub-runtime`, strict schema
  rejection, `ExternalContextBundle` round-trip, positive structured
  output through a stub OpenAI-compatible agent, and the same Report
  contract through the vLLM adapter shape.
- The stdio MCP adapter exposes all 16 gRPC RPCs as `choreo_*` tools
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

### 3. Consumer-smoke positive path is not the default path

`choreo-consumer-smoke` exercises the public gRPC + optional NATS
surface and proves the strict rejection path against a NoopAgent stack.
The positive structured-output path exists in compose scenario 8, but
the smoke harness does not yet ship a first-class mode that registers
the stub-LLM-backed structured JSON agent and flips
`report_payload_validates` to `Passed`.

This is a harness ergonomics gap, not a missing Choreographer feature.

### 4. Compose E2E operations doc is missing

The compose stack now includes both `stub-runtime` and `stub-llm`, but
there is no dedicated operations document for:

- the nine scenarios and what each proves
- the stub-LLM OpenAI-compatible surface
- the hard-coded Report payload
- `STUB_LLM_LISTEN`
- when to use `make e2e-compose` versus `make e2e-provider-vllm`

The backlog tracks this as `docs/operations/compose-e2e.md`.

### 5. Streaming remains phase-level

`StreamDeliberation` emits phase transitions and a final winner frame.
The proto shape reserves payload variants for proposal, critique, and
revision frames, but those per-turn frames are not emitted yet.

That is an honest product limitation for live agent UIs; synchronous
callers can use `Deliberate` or the buffered MCP wrapper today.

## Recommended Hardening Plan

1. Add `docs/operations/compose-e2e.md` so operators can interpret the
   repo-owned E2E stack without reading the runner source.
2. Add a consumer-smoke positive-path mode that registers an
   OpenAI-compatible structured JSON agent when a stub or provider
   endpoint is available.
3. Keep `make e2e-provider-vllm` as the explicit external-provider
   validation path and require downstream deployments to run it, or an
   equivalent provider smoke, before claiming provider readiness.
4. Only add a Choreographer-owned context adapter if a concrete product
   needs that ownership boundary. Otherwise keep caller-materialized
   context as the documented contract.
5. Treat per-proposal/per-critique/per-revision streaming as a separate
   additive feature, with tests that prove frame ordering and backpressure.

## Honest Current Position

The Choreographer is now a real gRPC + NATS + persistence + Runtime
executor + MCP application with structured-output validation and
repo-owned stack E2E over stubs.

It is not a product integration by itself. Downstream products still
own their context materialization, provider credentials, Runtime tool
catalog, output contracts, and production smoke tests.
