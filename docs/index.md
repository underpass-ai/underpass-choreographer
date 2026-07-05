# Documentation Index

Navigation hub for `underpass-choreographer` docs. Each entry links to
the canonical file and gives a one-line orientation.

Choreographer is agnostic and independently usable. In Underpass
platform research it is often discussed alongside these planes, but it
does not require KMP, PIR, or any downstream product to run:

- **[Underpass KMP](../README.md#the-underpass-platform)** — Kernel
  Memory Plane / Kernel Memory Protocol. Memory + context plane.
  Lives in the sibling repo `rehydration-kernel`; one possible producer
  of caller-supplied `ExternalContextBundle`s.
- **Underpass Choreographer** — this repo. Event-driven coordination
  plane for councils of specialist agents: structured deliberations,
  declarative YAML ceremonies, output contracts, optional LLM judge,
  executor hand-off — over gRPC and MCP.
- **Underpass Runtime** — execution + governed-tools plane. Lives in
  the sibling repo `underpass-runtime`; the choreographer talks to
  it through `RuntimeExecutor` (Epic 1).

## Architecture — how it works and how it differs

| Doc | Purpose |
|---|---|
| [`choreographer-architecture-and-differentiation.md`](./choreographer-architecture-and-differentiation.md) | Code-grounded walkthrough of the hexagonal core, council deliberation pipeline, the declarative ceremony engine, and the LLM-as-judge scorer — and where the design diverges from common agent-orchestration patterns. |
| [`embedded-choreographer.md`](./embedded-choreographer.md) | Two-distribution architecture and the implemented in-process ceremony API, injectable ports, local defaults and current limits. |
| [`choreographer-observability-design.md`](./choreographer-observability-design.md) | The observability design and the shipped metric catalogue served at `/metrics`: deliberation/judge/provider/ceremony Prometheus families, the differentiating signals (judge discrimination, winner-score distribution, vLLM serial saturation, token cost), and the alert/SLO + dashboard design. |

## Operations — how to run, install, and configure

| Doc | Purpose |
|---|---|
| [`dev-loop.md`](./dev-loop.md) | Local iteration loop, including `CHOREO_NATS_ENABLED=false just run` for no-external-service startup. |
| [`release.md`](./release.md) | Versioning + cut-a-release checklist. |
| [`operations/compose-e2e.md`](./operations/compose-e2e.md) | Repo-owned compose E2E: stack shape, scenarios (incl. YAML ceremony execution), stubs, Report schema, and provider-shaped paths. |
| [`operations/deploy-kubernetes.md`](./operations/deploy-kubernetes.md) | Helm install guide, including minimal standalone install, embedded NATS, TLS/mTLS, Postgres secret, provider env secrets, Runtime executor, and the Underpass Runtime profile. |
| [`operations/mcp-stdio.md`](./operations/mcp-stdio.md) | **MCP entry point.** Installable stdio adapter exposing every gRPC RPC as an MCP tool, including fixture-mode quickstart. |
| [`operations/codex-plugin.md`](./operations/codex-plugin.md) | Cumulative acceptance ladder, bundle layout, and local installation boundary for the Codex plugin. |
| [`operations/mcp/codex.md`](./operations/mcp/codex.md) | Codex CLI specifics: `codex mcp add`, dev-from-checkout, mTLS, fixture. |
| [`operations/mcp/claude-desktop.md`](./operations/mcp/claude-desktop.md) | `claude_desktop_config.json` snippets, per-OS paths, troubleshooting. |
| [`operations/support-matrix.md`](./operations/support-matrix.md) | Supported Rust toolchain and release-support rules. |
| [`operations/ceremony-authoring-runbook.md`](./operations/ceremony-authoring-runbook.md) | Authoring ceremonies: schema, rounds, sizing, output contracts, verification. |
| [`operations/observability-runbook.md`](./operations/observability-runbook.md) | Wiring traces, metrics and logs in a deployment. |
| [`operations/consumer-smoke.md`](./operations/consumer-smoke.md) | Standalone NATS consumer smoke check (incl. positive-path chain). |

## Discipline — how this project decides what to ship

| Doc | Purpose |
|---|---|
| [`PRINCIPLES.md`](./PRINCIPLES.md) | Honest documentation, demonstrable claims, scientific iteration. |
| [`../CHANGELOG.md`](../CHANGELOG.md) | Unreleased changes and release-note discipline before the first public tag. |
| [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | Contribution workflow, required gates, contract rules, and PR expectations. |
| [`../SECURITY.md`](../SECURITY.md) | Supported security scope, private vulnerability reporting, and deployment hardening baseline. |

## Status, gaps, and roadmap

| Doc | Purpose |
|---|---|
| [`backlog.md`](./backlog.md) | Epic-by-epic readiness backlog + session log + gating rules. PIR framing was dropped 2026-05-12 — this is a generic stack-readiness backlog. |
| [`stack-gap-analysis.md`](./stack-gap-analysis.md) | Current honest snapshot of what is wired, what remains product-owned, and what still needs downstream proof. |
| [`_archive/`](./_archive/) | Executed one-shot plans and legacy designs (usability plan, publication checklist, PIR case study). Historical only. |

## Experiments — append-only lab notebook

| Doc | Purpose |
|---|---|
| [`experiments/`](./experiments/) | Hypothesis → design → measurement → result per dated subfolder. Null results kept. |

## Research / Design — direction, not implementation claims

| Doc | Purpose |
|---|---|
| [`agentic-conversation-ceremony-evaluation-research.md`](./agentic-conversation-ceremony-evaluation-research.md) | Research on evaluating agentic meeting ceremonies using Choreographer with possible context/runtime providers such as KMP and Runtime. Status explicitly disclaimed as research. |
| [`agentic-meeting-ceremony-blueprints.md`](./agentic-meeting-ceremony-blueprints.md) | Catalog of product-agnostic meeting designs (intake, evidence review, past replay, future scenario, decision council, …). |

## Historical / out-of-scope

| Doc | Purpose |
|---|---|
| [`_archive/pir-choreographer-integration-design.md`](./_archive/pir-choreographer-integration-design.md) | Legacy PIR case-study design (archived 2026-07-05). PIR is owned by a separate project; retained only as a use-case study. |

## Where API examples live

| Path | Purpose |
|---|---|
| [`../api/examples/output-contracts/`](../api/examples/output-contracts/) | Canonical JSON Schemas for `OutputContract.json_schema` — currently a generic Report shape. |

## Sibling repos (for cross-reference)

- [`rehydration-kernel`](https://github.com/underpass-ai/rehydration-kernel)
  — Underpass KMP. MCP adapter pattern this repo's `crates/choreo-mcp`
  copies (`crates/rehydration-mcp/`).
- [`underpass-runtime`](https://github.com/underpass-ai/underpass-runtime)
  — execution plane. Proto vendored at
  `crates/choreo-proto/proto/underpass/runtime/v1/runtime.proto`;
  client adapter at `crates/choreo-adapters/src/runtime.rs`
  (Epic 1).
