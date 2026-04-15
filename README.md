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

### Quality gates

- Unit coverage: **minimum 80 % of lines**, target band 80–90 %, enforced
  by `scripts/ci/rust-coverage.sh`.
- Integration tests: **testcontainers-backed**, real services per run
  (no mocks at the integration boundary).
- End-to-end tests: a runner container drives scenarios either via
  `docker compose` (fast feedback) or as a Kubernetes `Job` against a
  kind cluster with the Helm chart installed (contract-true path).

## Status

Scaffold only. Porting underway from `swe-ai-fleet/services/orchestrator`,
stripping all SWE identity as described in `docs/experiments/` and the
PR template.
