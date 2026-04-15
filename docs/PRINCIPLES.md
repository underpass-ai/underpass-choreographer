# Engineering principles

This document is the standard we hold ourselves to. It applies to code,
docs, commit messages, release notes, and anything else in this repo.

## 1. Honesty over hype

- Do not describe a capability as present if it is not implemented and
  exercised in CI.
- Do not describe a number (latency, coverage, quality, accuracy) unless
  it came from a reproducible measurement in this repository.
- Use the words "planned", "prototype", "experimental", "not measured"
  out loud when they apply. Status claims in `README.md` are bounded
  by what CI can prove today.
- If a past claim is wrong, remove it. Do not soften it silently.

## 2. Everything demonstrable, everything measurable

- Behavioural claim → test that would fail if the claim broke.
- Performance claim → benchmark committed under `docs/experiments/`.
- Quality claim → gate in CI (coverage, lint, contract, security).
- Architectural claim → enforcement (lints, `buf breaking`, helm lint,
  test that rejects the forbidden shape).

If a claim has none of the above, it does not belong in the project.

## 3. Scientific method for iteration

When we change non-trivial behaviour we follow a named protocol:

1. **Hypothesis** — what do we expect and why, in one sentence that
   could be falsified.
2. **Design** — inputs, measured outputs, what would count as a
   refutation, environment, sample size.
3. **Method** — a `run.sh` that re-derives the results end-to-end.
4. **Results** — raw numbers with uncertainty. Charts are allowed;
   numbers are canonical.
5. **Conclusion** — did the data support the hypothesis? What did we
   *not* establish?
6. **Threats to validity** — what could be wrong with this experiment?
7. **Follow-ups** — concrete next experiments.

All of this lives under `docs/experiments/NNN-short-slug/`. Null results
are kept on purpose. Experiments are append-only: a new understanding
becomes a new numbered experiment, not an edit.

## 4. Use-case agnostic

The Choreographer coordinates specialist agent councils in response to
events. It does not know what the domain is. The following words must
not appear in the core, the protocol, the chart defaults, or the
AsyncAPI spec:

- software-engineering-specific: "story", "sprint", "backlog", "plan
  approval", "build phase", "test phase", "DEV/QA/ARCHITECT/DEVOPS/DATA"
- any other domain's vocabulary (clinical, logistics, finance, etc.)

If a use case needs domain vocabulary it attaches it via `attributes`
or `payload` fields (`google.protobuf.Struct`) — never by adding typed
fields to the contract.

## 5. Provider-agnostic

No LLM vendor, inference backend, or agent technology is privileged.
vLLM, Anthropic, OpenAI, local models, rule engines, and human-in-the-
loop are peer adapters behind the same `AgentPort` trait, each gated
behind its own Cargo feature. The core does not import any of them.

## 6. API-first

Two specifications are the source of truth:

- gRPC: `crates/choreo-proto/proto/underpass/choreo/v1/choreo.proto`
- AsyncAPI: `specs/asyncapi/choreographer.asyncapi.yaml`

Rust code is generated from (or aligned with) these specs. Any change
to public surface starts by editing the spec and passing the contract
gate (`scripts/ci/contract-gate.sh`). `buf breaking` and AsyncAPI
validation run before Rust compiles in CI.

## 7. DDD, SOLID, hexagonal, clean

- **No primitive obsession.** Domain APIs never pass raw `String`,
  `u32`, `f64` across boundaries — newtypes with invariants.
- **Aggregates protect invariants.** State transitions happen through
  aggregate methods, not by mutating fields.
- **Hexagonal.** `choreo-core` depends on nothing IO-shaped.
  `choreo-app` depends on `choreo-core`. Adapters depend on both.
  The binary is the composition root. Arrows never reverse.
- **SOLID.** Ports are narrow and segregated. Extension is by adding
  a new adapter / trait impl, not by editing the core.

## 8. Distribution is container-first, Kubernetes-first

The supported ways to run this service are:

- A container image (`Dockerfile`, docker or podman).
- The Helm chart (`charts/choreographer/`).

Local runs from `cargo run` are fine for development but are not a
product surface.

## 9. Quality gates

- Unit coverage: minimum **80 %** of lines, target band 80–90 %,
  enforced in CI.
- Integration tests: **testcontainers-backed**, real services per run.
- End-to-end: runner container via `docker compose` or as a Kubernetes
  `Job` against a kind cluster with the Helm chart installed.
- Clippy: `-D warnings`. Rustfmt: enforced. Contract gate: blocking.

## 10. What to do when tempted

If you are about to write a sentence that reads well but is not
something CI can prove — delete it. Add the experiment first, then
re-add the claim with a link to the experiment.
