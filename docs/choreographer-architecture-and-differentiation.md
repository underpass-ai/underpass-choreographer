# The Choreographer: Orchestrating Agents Toward a Good Decision, Not Just an Answer

Most agent systems optimize for getting *an* answer out of a model. The harder problem is getting a *good* decision out of several agents that disagree — and being able to say, mechanically and reproducibly, *why* one answer beat the others. The Choreographer is a coordination plane for multi-agent deliberation built around that distinction. It runs councils of specialist agents through a structured process — propose, critique, revise, validate, score, pick a winner — and it runs longer declarative *ceremonies* as explicit state machines. Everything that touches the outside world (model providers, persistence, messaging, execution, scoring policy) sits behind a narrow port, so the deliberation logic itself never learns the name of a vendor or a use case. This article walks the architecture and then states, generically and concretely, where it diverges from the prevailing patterns in agent orchestration.

## What it is

The Choreographer is the coordination plane of a three-plane platform: a memory/context plane produces LLM-ready context bundles, the Choreographer composes councils and runs deliberations and validates outputs, and a runtime plane executes the winning proposal under governance. The planes are separate repositories with no hard dependencies between them. The Choreographer "is agnostic and independently usable. It does not depend on KMP, PIR, or any downstream product" — it accepts a caller-supplied `ExternalContextBundle` from any source, and runtime execution is optional via an adapter (`README.md`, lines 18–24). It embeds no product vocabulary: no stories, plans, incidents, or claims are hardcoded.

Two core capabilities sit on top of the same domain. **Councils** run a single bounded deliberation among peer agents. **Ceremonies** run a longer, role-structured, multi-step process defined declaratively in YAML. Both reuse the same agents, validators, and scoring; a ceremony step can itself drive a full council deliberation.

## The hexagonal core

Before either capability, the shape that makes them composable: a strict dependency gradient. The architecture is hexagonal and enforced by crate boundaries, not by convention. `choreo-core` depends on nothing IO-shaped; `choreo-app` depends on `choreo-core`; adapters depend on both; the binary is the composition root. Arrows never reverse — and this is checkable: `choreo-core/Cargo.toml` carries no `reqwest`, `async-nats`, or `tonic`, while `choreo-adapters/Cargo.toml` imports all of them behind features.

**Ports live in the domain.** Ports are narrow, segregated traits in the core; adapters implement them. Crucially, every port returns `DomainError`, so adapter-shaped failures — I/O, wire parsing, vendor errors — are caught at the boundary and never leak upward as a type the domain has to reason about. `DomainError` names the *invariant violated*, not the primitive that violated it: `EmptyField`, `OutOfRange`, `InvalidTransition`, `InvariantViolated`, `NotFound`, `NoValidProposal` (`crates/choreo-core/src/error.rs`).

**The domain is typed and fail-fast.** Value objects enforce invariants at construction and are immutable thereafter. `Score` is the canonical example: it rejects NaN and infinities and constrains itself to a closed `[0.0, 1.0]`, which is what makes the final ranking a *total order* rather than a best-effort sort.

```rust
pub struct Score(f64);
impl Score {
    pub fn new(value: f64) -> Result<Self, DomainError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(DomainError::OutOfRange {
                field: "score", value, min: 0.0, max: 1.0,
            });
        }
        Ok(Self(value))
    }
}
```
*(`crates/choreo-core/src/value_objects/score.rs`, lines 22–32)*

Aggregates protect their state through behavior, not field mutation. A `Council` refuses to remove its last agent (`InvariantViolated`: "council must retain at least one agent"); a `Proposal.revise()` swaps content and bumps a revision counter while preserving identity; a `Task` keeps its per-run configuration — rubric, rounds, agent cap, deadline, output contract — inside a nested `TaskConstraints` value object rather than spilling primitives across the entity. The discipline is one class per file: entities, value objects, and ports each get their own directory and their own file.

## Councils: the deliberation pipeline

A deliberation is the central aggregate root, and it is a strict five-phase finite-state machine with one-way transitions only:

```text
Proposing -> Revising -> Validating -> Scoring -> Completed
```

Each transition has a precondition. You cannot leave `Proposing` with zero proposals; you cannot score before every proposal has an outcome. Methods reject operations that do not match the current phase (`crates/choreo-core/src/entities/deliberation.rs`, lines 30–66). The use case that drives this FSM, `DeliberateUseCase`, depends only on core traits and is documented as "fully domain-agnostic and provider-agnostic."

- **Proposing.** One proposal is seeded per agent via `AgentPort::generate`. Proposal IDs are returned *in agent order*, deliberately, to preserve the agent→proposal pairing the next phase relies on.
- **Revising.** Peer review runs `N` configurable rounds. The pairing is a deterministic circular rotation: agent *i* critiques the proposal of agent *(i+1) mod N*, then revises it in place.

```rust
let now = self.clock.now();
for round in 0..rounds {
    for (i, agent) in agents.iter().enumerate() {
        let peer_idx = (i + 1) % agents.len();
        let peer_proposal_id = ordered_proposal_ids[peer_idx].clone();
        let peer_content = deliberation.proposals()
            .get(&peer_proposal_id)
            .map(|p| p.content().to_owned())?; // peer content; error path elided
        let critique = agent.critique(&peer_content, constraints).await?;
        let revision = agent.revise(&peer_content, &critique).await?;
        deliberation.revise_proposal(&peer_proposal_id, revision.content, now)?;
    }
}
```
*(`crates/choreo-app/src/usecases/deliberate.rs`, lines 333–351)*

- **Validating.** Each proposal runs through every configured `ValidatorPort` in sequence, producing a list of `ValidatorReport`s (pass/fail plus opaque per-validator detail). Validators are domain-agnostic: lint, policy, fact-check, clinical-safety — the report carries any `kind` string, with no enum of use-case-specific check types baked into the core.
- **Scoring.** The reports for a proposal are aggregated into a single `Score` by the pluggable `ScoringPort`.
- **Completed.** `complete()` sorts proposals by score descending, breaking ties by proposal ID, and seals the ranking immutably: `ranked.sort_by(|a, b| b.2.score().cmp(&a.2.score()).then_with(|| a.0.cmp(&b.0)))` (`deliberation.rs`).

If the task declares an **output contract**, the ranking is reprioritized so valid proposals (those that passed all reports) come before invalid ones, and a missing valid proposal is a fail-safe `DomainError` rather than a silently-bad winner. A call-scoped `DeliberationObserverPort` (distinct from the persistent messaging port) lets one caller stream phase transitions live without affecting other callers or persisting anything; the default is a `NullObserver`.

## Ceremonies: declarative state machines

Where a council is a single deliberation, a ceremony is a longer, role-structured process expressed as pure data. A `CeremonyDefinition` is a finite-state machine — states (with initial/terminal flags), transitions (from/to/trigger), per-state steps with pluggable handlers, guards on transitions, and roles that own actions — and it validates its whole structure on construction: exactly one initial state, a well-formed transition graph, valid step/guard/role references. It is pure domain: it knows nothing about YAML, transport, or a handler registry.

```yaml
states:
  - id: OPENING
    initial: true
  - id: DIVERGING
transitions:
  - from: OPENING
    to: DIVERGING
    trigger: context_shared
    guards:
      - open_room_completed
steps:
  - id: open_room
    state: OPENING
    handler: facilitation_prompt
    config:
      participants: [facilitator]
      prompt: "Open the meeting, restate the brief, and invite perspectives."
guards:
  open_room_completed:
    type: automated
    check: "step_status:open_room:COMPLETED"
roles:
  - id: FACILITATOR
    allowed_actions: [open_room, context_shared]
```
*(excerpt from `tests/e2e/ceremonies/editorial-planning-meeting.yaml`)*

`RunCeremonyUseCase` executes the machine: for the current state it runs the configured steps, appends their contributions to a transcript, then applies the first transition whose guards are all satisfied — looping until a terminal state. Guards are declarative: `Always`, `AllStepsCompleted`, a `StepStatus` check, or `HumanApproval` resolved against context flags. Step handlers are pluggable via `CeremonyStepHandlerPort`; the shipped `DeliberatingCeremonyStepHandler` turns a step into a council deliberation. The step request threads prior context: prior transcript turns are rendered as prose into the agent's brief *by default* (a step can opt out via `see_prior: false`), so a downstream agent reasons about what was already said. Idempotency and retry — `StepLease`, `IdempotencyKey`, `retry_policy` — are owned by the ceremony instance in the domain, not bolted onto an adapter, which is what makes safe failover and re-execution a first-class property rather than an afterthought.

## Scoring and the LLM judge

Here is the subtle failure the architecture is built to avoid. Default **uniform scoring** sets a proposal's score to the fraction of validator reports that passed. When two proposals both pass every structural validator, they tie at 1.0, and the ranking falls back to breaking the tie *by proposal ID* — which is to say, arbitrarily. Boilerplate checks ("is it non-empty," "does it match the schema") cannot tell a brilliant proposal from a merely-compliant one.

The fix is a pluggable scoring policy plus an LLM-as-judge validator. `LlmJudgeValidator` implements `ValidatorPort` and rates a proposal on intrinsic quality — specificity, internal consistency, completeness, actionability — penalizing vagueness, unfilled placeholders, and self-contradiction. It writes its 0.0–1.0 verdict into the report's details under a single contract key, `judge.score`. `JudgeAwareScoring` reads that key and lets the verdict *be* the score:

```rust
impl ScoringPort for JudgeAwareScoring {
    async fn score(&self, reports: &[ValidatorReport]) -> Result<Score, DomainError> {
        if let Some(verdict) = reports
            .iter()
            .find_map(|report| report.details().get(JUDGE_SCORE_DETAIL_KEY))
            .and_then(Value::as_f64)
        {
            return Score::new(verdict.clamp(0.0, 1.0));
        }
        if reports.is_empty() { return Ok(Score::MIN); }
        let passed = reports.iter().filter(|report| report.passed()).count();
        Score::new(passed as f64 / reports.len() as f64)
    }
}
```
*(`crates/choreo-adapters/src/scoring.rs`; the scoring-mode metric instrumentation is elided here for readability)*

Three properties matter. It is a **safe default**: with no judge report present it falls back to exactly the uniform pass-fraction policy. It is **fail-fast**: the judge is opt-in via `CHOREO_JUDGE_ENABLED`, and if enabled but missing its endpoint, model, or threshold, `judge_from_env()` returns a `DomainError` at composition time — wiring fails to start, it does not silently degrade at runtime. And it is **decoupled**: the judge is a validator that feeds the scorer through a structured detail contract, so pass/fail validation and ranking policy stay separate concerns. Because the verdict is clamped into a bounded `Score`, the final `sort_by(score desc, then id)` remains a deterministic total order; the judge simply replaces an arbitrary tie-break with an intrinsic-quality signal.

## Integration boundaries

The agents are the first of four outer ports, and none of them is privileged. `AgentPort` is a three-method trait — `generate`, `critique`, `revise` — and the doc is explicit: the Choreographer does not know or care whether an agent is backed by a local inference server, a cloud API, a deterministic rule engine, or a human in the loop; every implementation lives behind the trait and no provider is privileged (`crates/choreo-core/src/ports/agent.rs`, lines 3–6). Each provider is a peer adapter behind its own Cargo feature; adding one is purely additive — a new feature, a new module, a new `impl AgentPort`, zero core changes. The `DispatchingAgentFactory` selects providers from environment config at boot and **fails loud**: an unsupported agent kind returns `DomainError::InvariantViolated("agent factory: unsupported agent kind")`, never a silent ignore. Credentials never persist in registry descriptors; they are env-sourced at boot and wrapped in opaque types whose `Debug` redacts the secret, so an accidental `dbg!()` cannot leak an API key.

The other three boundaries are equally narrow and equally optional. `ExecutorPort` is opaque — the Choreographer "does not know what execution means beyond 'adapter runs it and reports an outcome'" — with a gRPC-to-runtime adapter and a deterministic no-op adapter. `MessagingPort` is event-typed, not arbitrary JSON: one method per event (`publish_task_dispatched`, `publish_deliberation_completed`, `publish_phase_changed`, …), with a core NATS pub/sub adapter and a no-op default. Persistence is all-or-nothing by design — every registry and repository is either Postgres-backed or in-memory; splitting the source of truth across replicas is deliberately unsupported to avoid consistency hazards.

## A meeting, observed: planning a drone

The machinery above is abstract by design; here is one concrete run. The Choreographer is handed an ambiguous engineering brief — design a sub-$4,500, 25 kg-MTOW drone that flies a 45-minute mission to spot diseased trees across mixed canopies — and an `engineering_planning` ceremony with four roles, each a council of three vLLM-backed agents: a mission owner, a systems architect, a safety reviewer, and a program lead. The ceremony threads each step's winning contribution into the next step's brief, so the meeting compounds rather than restarts.

It does not read like one model answering four times. The **mission owner** refuses to dodge the trade-off it was handed — *"we prioritize sensing fidelity over sub-meter precision … we will not exceed the $4,500 budget chasing perfect coordinates"* — and names the one assumption that would sink the project (treating the downstream disease classifier as a black box), pinning it to a falsifiable contract: 4-band multispectral imagery at ≤10 cm/pixel, 70% overlap. The **systems architect** commits to a named "Sensing-First" architecture, records three *deliberate rejections* (no companion compute, no RTK/SLAM, no onboard autonomy) each with a reason, and states its bet out loud: *"I am betting on relative consistency [over] absolute GPS … sacrificing onboard intelligence to maximize flight time."* The **safety reviewer** does not rubber-stamp it — it isolates a specific kill-mechanism ("the canopy-wind feedback loop"), **overrules** the architect's Li-ion battery choice (voltage sag under gust-load brown-outs the flight controller), and mandates a concrete replacement: a high-discharge LiPo plus an isolated BEC and a capacitor bank on the power rail. The **program lead** then *decides* — *"I side with the safety reviewer on power delivery and maintain 50 m altitude to guarantee the GSD contract"* — reconciling the endurance-versus-stability conflict instead of averaging it away.

That winning prose for each step is not buried inside the engine. The chosen contribution is returned on the `RunCeremony` response (`CeremonyStepExecution.output`), so the meeting record — the *acta* — is a first-class API artifact, not a log line. And because each step's winner is selected by the judge-aware scorer, the earlier guarantee holds end-to-end: this is the highest intrinsic-quality proposal that survived peer critique, not the first one drafted or an arbitrary tie-break.

The meeting is also **fully replayable as a distributed trace**. Built with the `otel` feature and given an OTLP endpoint, the whole deliberation becomes one trace whose span events carry the debate itself — *proposal drafted*, *peer critique and revision*, *validator verdict*, *proposal scored* (with the judge's 0.0–1.0 number), *deliberation completed* (with the winning score). Export is over **mutual TLS** — the Choreographer presents a client certificate to the collector, the Underpass standard for every in-cluster hop — landing in Tempo and viewable in Grafana. The result is uncommon for an agent system: you can open a past meeting and watch, span by span, *which* proposals were made, *how* they were critiqued, *what* the judge scored each one, and *why* a particular contribution won — the reasoning itself, addressable by trace ID, not a summary reconstructed after the fact.

And in aggregate the same meeting is a set of **metrics**, scraped from `/metrics` and recorded through a `MetricsRecorderPort` whose contract is deliberately synchronous and infallible — instrumentation can never block or fail a deliberation. They measure the things a deliberation orchestrator should be judged on, which generic RED dashboards never capture: the **winner-score distribution** (is the *quality* of decisions drifting?), the **`NoValidProposal` rate** (every proposal generated but none satisfied the contract), per-provider **token cost** and **in-flight depth** (the leading indicator of vLLM serial saturation), and — the sharpest of them — **judge discrimination**: the rate at which the judge's verdict actually re-ranks the winner versus merely confirming the first proposal. A judge that never re-ranks is expensive dead weight; the metric, computed inside the judge-aware scorer so the use case stays judge-agnostic, is the one number that answers "is the expensive LLM judge earning its tokens?" — a question no generic agent dashboard thinks to ask.

## What makes it different

Stated generically, against the prevailing state of the art, and tied to a concrete mechanism in each case:

- **Where graph- and code-first orchestration encodes the process as imperative code**, the Choreographer encodes deliberations as a typed five-phase aggregate FSM and ceremonies as declarative YAML parsed into a validated `CeremonyDefinition`. The process is data and invariants, not control flow you have to read to understand.
- **Where a single-agent tool-calling loop produces one answer with no internal disagreement**, the council runs *peer* critique and revision in a deterministic rotation (*i* critiques *(i+1) mod N*) across configurable rounds, so the winner survives contact with adversarial review.
- **Where a single-agent loop returns the first completion and stops, and multi-agent systems break ties among equally-valid outputs by proposal ID (arbitrary)**, scoring here is an explicit total order over a bounded `Score`, and the pluggable judge replaces the arbitrary ID tie-break with an intrinsic-quality verdict.
- **Where vendor-coupled SDKs assume one provider's API shape**, every vendor is a feature-gated peer behind `AgentPort`, secrets are redacted opaque types, and an unsupported kind fails loudly rather than silently no-op-ing.
- **Where anemic glue code spreads invariants across handlers**, value objects enforce bounds at construction, aggregates guard their own transitions, and all ports return `DomainError` so adapter failures never leak upward as foreign types.
- **Where context is threaded through a shared mutable channel that concurrent runs can race on**, context arrives as a caller-supplied, per-call `ExternalContextBundle` through a port, and a ceremony's transcript is owned by its own instance — prior turns flow into downstream briefs by default, with no global state for parallel ceremonies to contend over.
- **Where most agent systems emit a final answer and, at best, an unstructured log**, a deliberation is one OpenTelemetry trace whose span events carry the debate — every proposal, critique, validator verdict, and judge score, plus the winning rationale — exported over mutual TLS, so a past meeting is replayable span-by-span by trace ID; and in aggregate it is a set of deliberation-specific Prometheus metrics (winner-score distribution, `NoValidProposal` rate, per-provider token cost and saturation, and judge re-rank discrimination) that ask whether the *decisions* are good and whether the judge earns its tokens — questions a generic RED dashboard never poses.

## How it runs

The composition root reads `ServiceConfig` from the environment and wires every adapter: scoring policy (uniform vs. judge-aware), persistence (all-Postgres or all-in-memory), messaging (NATS or no-op), and executor (runtime gRPC or no-op). Deployment is Kubernetes-native via a Helm chart with checked-in profiles — minimal (no-op, in-memory), embedded-NATS, Postgres-secret, and a runtime profile that wires mTLS to the runtime plane, a vLLM endpoint serving a gemma model, and the judge enabled at a 0.5 threshold. Built with the `otel` feature and given an OTLP endpoint, the same runtime profile exports every deliberation as a distributed trace over mutual TLS to the cluster collector; absent the endpoint it stays JSON-to-stdout with no background exporter.

Evidence is contract-first and gated. The per-PR gate enforces protobuf breaking-change detection plus AsyncAPI validation *before any Rust compiles*, then `clippy -D warnings`, format checks, bench-compile, and unit + integration tests with provider features locked; the helm-lint gate refuses to render without a pinned image and asserts hardened manifests (NetworkPolicy, PodDisruptionBudget, read-only root filesystem, secret wiring). A compose-stack end-to-end smoke — the runner driven through the full gRPC + NATS stack — gates tag-time publication before any image or chart ships. Operator-run paths go further against a real cluster: a Kubernetes-Job runner, and a provider-backed e2e that spins up real agents against a gemma-via-vLLM endpoint and runs a full deliberation — proposals, peer revisions, validation, and judge scoring — against a live model, not a mock.

The judge and judge-aware scoring are the newest layer. Everything beneath them — the hexagonal core, the council deliberation FSM, the declarative ceremony engine, and the provider-agnostic agent adapters — predates them, is on the main branch, and is covered by the contract-gated CI above. The judge is opt-in and fails fast on misconfiguration; with it enabled, the full propose → critique → revise → validate → judge-score pipeline is exercised end-to-end against a real vLLM-served gemma model in the Kubernetes runtime profile.
