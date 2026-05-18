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
- **Underpass Choreographer** — this repo. Coordination plane:
  domain-agnostic deliberation, council orchestration, executor
  hand-off, and MCP exposure of every RPC.
- **Underpass Runtime** — execution + governed-tools plane. Lives in
  the sibling repo `underpass-runtime`; the choreographer talks to
  it through `RuntimeExecutor` (Epic 1).

## Operations — how to run, install, and configure

| Doc | Purpose |
|---|---|
| [`dev-loop.md`](./dev-loop.md) | Local iteration loop; every command mirrors a CI gate. |
| [`release.md`](./release.md) | Versioning + cut-a-release checklist. |
| [`operations/mcp-stdio.md`](./operations/mcp-stdio.md) | **MCP entry point.** Installable stdio adapter exposing every gRPC RPC as an MCP tool. |
| [`operations/mcp/codex.md`](./operations/mcp/codex.md) | Codex CLI specifics: `codex mcp add`, dev-from-checkout, mTLS, fixture. |
| [`operations/mcp/claude-desktop.md`](./operations/mcp/claude-desktop.md) | `claude_desktop_config.json` snippets, per-OS paths, troubleshooting. |

## Discipline — how this project decides what to ship

| Doc | Purpose |
|---|---|
| [`PRINCIPLES.md`](./PRINCIPLES.md) | Honest documentation, demonstrable claims, scientific iteration. |

## Status, gaps, and roadmap

| Doc | Purpose |
|---|---|
| [`backlog.md`](./backlog.md) | Epic-by-epic readiness backlog + session log + gating rules. PIR framing was dropped 2026-05-12 — this is a generic stack-readiness backlog. |
| [`stack-gap-analysis.md`](./stack-gap-analysis.md) | Current honest snapshot of what is wired, what remains product-owned, and what still needs downstream proof. |
| [`product-usability-publication-plan.md`](./product-usability-publication-plan.md) | Spanish plan for making the Choreographer usable and publishable as a product surface. |
| [`product-publication-checklist.md`](./product-publication-checklist.md) | Living checklist for tracking the usability and publication plan. |

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
| [`pir-choreographer-integration-design.md`](./pir-choreographer-integration-design.md) | Legacy PIR case-study design. PIR is owned by a separate project; this file is retained only as a possible use-case study and is not load-bearing for this repo's backlog. |

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
