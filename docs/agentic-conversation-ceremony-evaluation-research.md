# Agentic Meeting Ceremony Evaluation Research

Status: research and design proposal. This document is not an implementation
claim.

Choreographer is agnostic and independent. KMP, Runtime, and other
systems mentioned here are studied as possible context, evidence, or
tool providers for evaluations; they are not required dependencies of
the Choreographer product.

Date: 2026-04-26

## Scope

We want to evaluate whether a designed agentic meeting ceremony is a good fit
for a given problem. The design must use Choreographer as a
domain-agnostic conversation orchestrator. Product vocabulary, domain facts, and
application-owned identifiers must stay at the edge through task attributes,
external context metadata, output contracts, kernel graph data, or runtime
session metadata.

Code reviewed locally for the original research snapshot:

- this Choreographer repository
- a local checkout of `underpass-runtime`
- a local checkout of `rehydration-kernel`

Detailed meeting blueprints live in
`docs/agentic-meeting-ceremony-blueprints.md`.

## Vision: Real-World-Like Agentic Meetings

The target abstraction is closer to a real meeting than to a free-form chat.
Agents should meet with an agenda, explicit roles, evidence requests, minutes,
decisions, dissent, assumptions, and action items. Claims made inside the
meeting should be treated as unverified until they are backed by evidence from
kernel context, runtime tool results, artifacts, or approved external inputs.

This gives us three complementary planes:

- Choreographer is the neutral meeting facilitator. It controls phases,
  participants, turn policy, critique/revision/scoring, and trace emission
  without knowing the product domain.
- Rehydration Kernel is the scenario memory plane. It can recreate past
  situations by rendering causally meaningful context from stored graph state.
  For future planning, the application can materialize hypothetical scenario
  graphs or branches and ask the kernel to render them with the same evidence
  discipline.
- Underpass Runtime, reached through the runtime client, is the governed tool
  and action plane. It lets the meeting validate claims, inspect systems, run
  checks, produce artifacts, and record policy denials or approvals as
  first-class evidence.

The evaluator should therefore judge not only the final answer, but the meeting
quality: whether the right agenda was selected, whether claims were validated,
whether past context was reconstructed faithfully, whether future scenarios were
planned with explicit assumptions, and whether resulting decisions are
traceable to evidence.

Important distinction: the kernel does not need to predict the future. Future
planning should be modeled as explicit hypothetical graph construction and
evaluation. The meeting owns assumptions and scenario branches; the kernel
renders and preserves them; runtime-client tools validate whatever can be
validated.

## Design Stance: Adapter-Agnostic, Sibling-Optimized

The medium-term goal is a generic Choreographer that is usable outside the
Underpass stack. The contracts must therefore stay adapter-agnostic: no kernel
types, runtime tool names, Kubernetes terms, or product vocabulary should leak
into the core meeting model.

At the same time, the first meeting designs should be optimized for our sibling
systems because that is where the strongest evidence loop exists today:

- Rehydration Kernel gives us past-scenario replay, causal context, evidence
  packs, relation explanations, content hashes, quality metrics, and
  hypothetical scenario branches supplied by the application.
- Underpass Runtime gives us governed tool discovery, recommendations,
  invocations, policy decisions, denials, approvals, logs, artifacts, quality
  metrics, and learning evidence.

The design rule is:

- Core contracts describe capabilities, not products: `context_provider`,
  `scenario_provider`, `tool_runtime`, `evidence_source`, `artifact_source`,
  `policy_evaluator`, and `trace_sink`.
- Underpass adapters bind those generic capabilities to the kernel and runtime
  client.
- Meeting types may require capability classes such as replay, scenario
  branching, governed invocation, artifact capture, or policy validation, but
  they must not require a concrete implementation.
- The first reference implementations should use kernel + runtime client as the
  default high-power backend.

This keeps the architecture honest: generic by contract, Underpass-native by
initial deployment.

## Local Evidence

### Choreographer

Choreographer is currently a domain-agnostic deliberation engine.

- `docs/PRINCIPLES.md` states that core/protocol/specs must avoid use-case
  vocabulary. Domain terms belong in `attributes` or external context.
- `crates/choreo-core/src/ports/agent.rs` exposes only generic agent actions:
  `generate`, `critique`, and `revise`.
- `crates/choreo-app/src/usecases/deliberate.rs` implements a generic ceremony:
  propose, peer critique, revise, validate, score, complete.
- `crates/choreo-app/src/usecases/orchestrate.rs` composes deliberation with an
  executor and carries generic execution options.
- `crates/choreo-core/src/entities/task.rs` now carries `TaskMetadata` for
  causal IDs and execution profile, while application-owned IDs remain in
  `Task.attributes` or `ExternalContextBundle.metadata`.
- `crates/choreo-core/src/entities/external_context.rs` provides bounded,
  typed-but-caller-owned context items, references, summaries, and metadata.
- `crates/choreo-adapters/src/agents/prompts.rs` keeps system prompts neutral
  and tests against domain-vocabulary leaks.

Implication: meeting ceremonies can be represented as orchestration
patterns, but not as product workflows embedded in the core.

### Rehydration Kernel

The kernel is the evidence memory and context rehydration plane.

- `crates/rehydration-domain/src/value_objects/relation_semantic_class.rs`
  defines semantic relation classes: structural, causal, motivational,
  procedural, evidential, and constraint.
- `crates/rehydration-domain/src/value_objects/relation_explanation.rs`
  captures why a relationship exists: rationale, motivation, method, evidence,
  confidence, sequence, and related decision/cause IDs.
- `crates/rehydration-domain/src/value_objects/rehydration_mode.rs` defines
  modes such as `ResumeFocused` and `ReasonPreserving`.
- `crates/rehydration-domain/src/value_objects/resolution_tier.rs` defines L0,
  L1, and L2 tiers, which map well to summary, causal spine, and evidence pack.
- `crates/rehydration-domain/src/value_objects/bundle_quality_metrics.rs`
  computes raw-equivalent tokens, compression ratio, causal density, noise
  ratio, and detail coverage.
- `crates/application/src/queries/get_context.rs` and related renderers return
  a rendered context with content hash, selected mode, tiers, truncation, and
  quality metrics.
- `crates/rehydration-testkit/src/llm_graph.rs` validates `GraphBatch`:
  unique nodes, reachable root, valid relation endpoints, and richer evidence
  requirements for non-structural relations.
- `crates/transport-grpc/src/agentic_reference/basic_context_agent.rs` shows an
  agent using kernel context to drive runtime actions through a generic
  runtime contract.

Important caveat: `context.bundle.generated` is documented in the kernel
AsyncAPI and test fixtures, but `docs/beta-status.md` says it is contract-only
today and not emitted by the kernel runtime yet. Evaluations should therefore
prefer explicit `GetContext` calls and captured render metadata until that event
is implemented.

### Underpass Runtime

The runtime is the governed action and execution-evidence plane.

- `internal/domain/session.go` models sessions with workspace, runtime
  reference, allowed paths, principal, and metadata.
- `internal/domain/capability.go` models tools with schemas, scope, side
  effects, risk, approval requirements, policy metadata, observability, and
  examples.
- `internal/domain/invocation.go` models invocation status, correlation ID,
  timing, output, logs, artifacts, and errors.
- `internal/app/service.go` implements session creation, tool discovery,
  invocation, authorization, artifact persistence, event publication, quality
  observation, and learning telemetry.
- `internal/app/recommender.go` implements `RecommendTools`, persists
  recommendation decisions, emits learning events, and tracks cross-agent
  insight.
- `internal/domain/quality_metrics.go` gives invocation quality signals such
  as status, duration, exit code, output size, error code, and latency bucket.
- `internal/adapters/policy/static_policy.go` enforces roles, risk, approvals,
  allowed paths, subject/topic/queue/key-prefix constraints, namespaces,
  registries, and profiles.
- `specs/underpass/runtime/learning/v1/learning.proto` carries learning
  evidence facts with correlation and causation IDs.
- `e2e/tests/10-llm-agent-loop` validates an LLM conversation loop over tool
  discovery and invocation.
- `e2e/tests/12-event-driven-agent` validates event-driven activation,
  governed tool use, and result publication.
- `e2e/tests/13-multi-agent-pipeline` validates a concrete multi-agent
  pipeline ceremony over a shared runtime session.

Implication: the runtime gives objective evidence about what the conversation
actually did, what was denied, what artifacts were produced, and whether tool
recommendations or policies shaped the trajectory.

## External Patterns

The web/research baseline suggests these reusable ceremony archetypes:

| Ceremony archetype | Shape | External evidence | Local fit |
| --- | --- | --- | --- |
| Solo refinement | draft -> self-feedback -> revise -> validate | Self-Refine: <https://arxiv.org/abs/2303.17651>; Reflexion: <https://arxiv.org/abs/2303.11366> | Single-agent Choreographer council or runtime loop with reflection memory from kernel |
| Peer-review council | parallel proposals -> peer critique -> revision -> score | AutoGen multi-agent conversations: <https://arxiv.org/abs/2308.08155> | Current Choreographer `DeliberateUseCase` |
| Adversarial debate | independent answers -> debate rounds -> judge | Multiagent debate: <https://arxiv.org/abs/2305.14325> | Add debate phase policy above generic agent ports |
| Role-play pair | role-constrained assistant/user agents cooperate | CAMEL role playing: <https://arxiv.org/abs/2303.17760> | Role descriptions in council/agent descriptors, still generic |
| Selector group chat | shared thread, next speaker selected by manager/model | AutoGen conversation patterns: <https://autogenhub.github.io/autogen/docs/tutorial/conversation-patterns/> | Requires turn-level trace and speaker-selection policy |
| Supervisor/router | central supervisor delegates to specialists | LangChain multi-agent docs: <https://docs.langchain.com/oss/python/langchain/multi-agent/index> | App-layer shell can route between Choreographer, kernel, and runtime |
| SOP pipeline | ordered roles with structured handoffs | MetaGPT SOPs: <https://arxiv.org/abs/2308.00352>; ChatDev chat chain: <https://arxiv.org/abs/2307.07924> | Runtime e2e `13-multi-agent-pipeline`, but domain-specific roles stay outside Choreographer |
| Social/memory simulation | agents remember, reflect, plan, and interact over time | Generative Agents: <https://arxiv.org/abs/2304.03442> | Kernel provides durable context; runtime provides action evidence |
| Governed execution board | agents must acquire evidence before conclusions/actions | AgentBench interactive evals: <https://arxiv.org/abs/2308.03688>; GAIA tool-use tasks: <https://arxiv.org/abs/2311.12983> | Kernel + runtime evidence gates before scoring |
| Agent-as-judge | evaluator inspects intermediate trajectory, not only output | Agent-as-a-Judge: <https://arxiv.org/abs/2410.10934>; MT-Bench LLM judge caveats: <https://arxiv.org/abs/2306.05685> | Evaluator should combine deterministic checks with bounded LLM judging |

## Agentic Meeting Types

Meeting types are product-agnostic templates. They should define the shape of
the meeting, not the vocabulary of the application that uses it. A consuming
application can bind domain data into `ProblemSpec`, `ExternalContextBundle`,
`Task.attributes`, runtime session metadata, or kernel graph nodes without
changing these contracts.

### Meeting Type Contract

Every meeting type should be describable with the same neutral contract:

- `meeting_type`: stable generic identifier.
- `purpose`: why this meeting exists.
- `agenda_template`: ordered generic agenda items.
- `role_slots`: facilitator, proposer, challenger, evidence reviewer,
  operator, recorder, judge, or observer.
- `context_policy`: required context capability, max detail tier, focus
  strategy, replay requirements, and scenario-branch requirements. Underpass
  binds this to the kernel.
- `tool_policy`: allowed tool capability families, recommendation use,
  approval mode, max risk, side-effect budget, and artifact requirements.
  Underpass binds this to the runtime client.
- `claim_policy`: which claims require evidence, which may remain assumptions,
  and what hard gates apply to unsupported claims.
- `decision_policy`: how decisions are accepted, rejected, deferred,
  escalated, or split into action items.
- `output_contracts`: required generic outputs such as `MeetingMinutes`,
  `ClaimLedger`, `DecisionRecord`, `ScenarioBranch`, `ActionItem`, and
  `EvaluationScorecard`.
- `evaluation_focus`: which scoring dimensions matter most for this meeting.

### Catalog

| Meeting type | Purpose | Kernel leverage | Runtime client leverage | Required outputs |
| --- | --- | --- | --- | --- |
| Intake meeting | Frame an ambiguous problem and decide the next ceremony | Retrieve compact context and prior related state with L0/L1 focus | Discover available evidence tools and low-risk read-only checks | `MeetingMinutes`, `ProblemFrame`, `EvidenceRequest`, `NextCeremonyDecision` |
| Evidence review meeting | Verify or reject important claims before a decision | Render evidence packs, relation explanations, provenance, and content hashes | Run validation tools, collect artifacts, record denials and failures | `ClaimLedger`, `EvidenceBundleRef`, `UnresolvedQuestion`, `ValidationReport` |
| Past replay meeting | Reconstruct what happened in a previous scenario | Use reason-preserving replay, causal spine, temporal context, and known evidence | Pull logs, metrics, files, prior outputs, or other read-only artifacts | `ReplayTimeline`, `CausalPath`, `ClaimLedger`, `KnownUnknowns` |
| Future scenario planning meeting | Plan possible future branches with explicit assumptions | Render hypothetical scenario graphs or branches supplied by the application | Run simulations, dry runs, policy checks, cost checks, or feasibility probes | `ScenarioBranch`, `AssumptionLedger`, `RiskRegister`, `DecisionOptions` |
| Decision council | Choose among competing options under evidence and constraints | Compare context bundles for each option and preserve rationale/evidence | Validate feasibility and side effects for candidate actions | `DecisionRecord`, `DissentRecord`, `TradeoffMatrix`, `ActionItem` |
| Design review meeting | Challenge a proposed plan before implementation or execution | Retrieve prior decisions, constraints, causal dependencies, and evidence | Run static checks, compatibility checks, test probes, or policy checks | `ReviewFinding`, `ClaimLedger`, `RevisionRequest`, `ApprovalDecision` |
| Red-team meeting | Stress-test assumptions, safety, and failure modes | Retrieve similar failures, constraints, weak signals, and distractors | Execute safe probes and policy simulations; record blocked actions | `RiskRegister`, `AttackPath`, `MitigationPlan`, `DeniedActionRecord` |
| Production incident resolution meeting | Investigate a production software incident and decide a safe corrective path | Replay recent context, causal spine, prior similar scenarios, and candidate remediation branches | Run governed diagnostics, read-only inspection, tests, policy checks, dry runs, and artifact capture | `IncidentAnalysis`, `ReplayTimeline`, `ClaimLedger`, `RemediationOption`, `DecisionRecord`, `RollbackPlanRef` |
| Execution readiness meeting | Decide go/no-go before a governed action | Render current state, blockers, dependencies, and required evidence | Verify approvals, environment state, tool availability, and dry-run artifacts | `ReadinessChecklist`, `GoNoGoDecision`, `ActionPlan`, `RollbackPlanRef` |
| Live coordination meeting | Coordinate multi-step work while evidence changes | Refresh focused context between phases and preserve causal continuity | Invoke governed tools, capture artifacts, update claim status after each step | `PhaseLog`, `ClaimLedger`, `ArtifactIndex`, `ActionItem` |
| Post-action learning meeting | Learn from an executed action and update future behavior | Rehydrate before/after context and preserve causal deltas | Analyze invocation quality, artifacts, recommendation decisions, and failures | `LearningRecord`, `OutcomeAssessment`, `UpdatedAssumption`, `FollowupAction` |
| Escalation/handoff meeting | Transfer context and decisions to another actor or system | Render resume-focused context with causal spine and unresolved evidence | Package artifacts, invocation refs, policy state, and pending approvals | `HandoffBrief`, `OpenDecision`, `EvidenceBundleRef`, `ResponsibilityMap` |

### Type Selection Heuristics

The evaluator should also judge whether the selected meeting type fits the
problem:

- Choose `Intake meeting` when the problem is under-specified or the right
  evidence path is unknown.
- Choose `Evidence review meeting` when the risk is mainly unsupported claims.
- Choose `Past replay meeting` when correctness depends on reconstructing a
  prior scenario.
- Choose `Future scenario planning meeting` when the task is about options,
  branches, assumptions, and consequences.
- Choose `Decision council` when there are multiple viable options and
  tradeoffs matter.
- Choose `Design review meeting` when there is already a proposal that needs
  critique before action.
- Choose `Red-team meeting` when safety, adversarial behavior, or hidden
  failure modes dominate.
- Choose `Production incident resolution meeting` when a production software
  incident needs evidence-backed diagnosis, remediation choice, and risk-aware
  action planning.
- Choose `Execution readiness meeting` when a governed action is imminent.
- Choose `Live coordination meeting` when the meeting must interleave reasoning
  and runtime actions.
- Choose `Post-action learning meeting` after action completion or failure.
- Choose `Escalation/handoff meeting` when responsibility moves across
  boundaries.

### Why Kernel And Runtime Client Matter

Without the kernel, these meetings degrade into short-context discussion. They
can still deliberate, but they cannot reliably recreate prior scenarios,
preserve causal paths, compare branches, or prove what evidence was available
at decision time.

Without the runtime client, these meetings degrade into unsupported reasoning.
They can make hypotheses, but they cannot validate claims through governed
tools, produce artifacts, observe denials, or prove that an action was feasible
and policy-compliant.

The intended product is the combination: a meeting ceremony that can replay
past context, construct future scenario branches, validate claims with tools,
and emit an auditable decision trail.

## Proposed Evaluation Model

### ProblemSpec

`ProblemSpec` describes the problem without assuming a particular ceremony:

- `problem_id`: stable identifier.
- `prompt`: the problem statement.
- `constraints`: budget, latency, safety, tool, and output constraints.
- `success_criteria`: objective assertions and qualitative rubrics.
- `required_evidence`: kernel nodes, relation classes, runtime artifacts, or
  external references that must support the final answer.
- `risk_budget`: maximum allowed side effects, approval requirements, and
  policy-denial expectations.
- `ground_truth`: optional expected answer, expected failure point, expected
  causal path, or known distractors.
- `domain_metadata`: application-owned data passed through generic metadata.

### CeremonySpec

`CeremonySpec` describes how agents should talk and act:

- `ceremony_id`: stable identifier.
- `roles`: generic participant descriptors, specialties, tool permissions, and
  context needs.
- `phases`: ordered or dynamic phases.
- `turn_policy`: parallel, round-robin, selector, supervisor, debate,
  pipeline, or bounded reflection.
- `evidence_policy`: when kernel context is mandatory, which rehydration mode
  to request, max tier, and required provenance.
- `runtime_policy`: which tool families are allowed, approval stance, risk
  ceiling, and artifact requirements.
- `output_contract`: schema and contract ID for final answer.
- `stop_criteria`: convergence, validator pass, token budget, time budget, max
  rounds, or judge confidence.

### PhaseSpec

Each phase should be inspectable:

- `phase_id`: stable generic phase name.
- `mode`: parallel, gated, pipeline, debate, reflection, or selector.
- `participants`: roles allowed in the phase.
- `inputs`: previous phase outputs, kernel context bundles, runtime artifacts,
  and external context.
- `allowed_actions`: generate, critique, revise, validate, score, query
  context, recommend tools, invoke tool.
- `validators`: deterministic contract validators and optional judge rubrics.
- `exit_conditions`: completion, revision required, escalation, or abort.

### RunTrace

The evaluator needs a full trace, not only the final answer:

- Choreographer events: event ID, correlation ID, causation ID, task ID,
  phase transitions, proposals, critiques, revisions, validations, scores, and
  final output.
- Kernel context: root/focus node IDs, rendered content hash, mode, tiers,
  token count, truncation, quality metrics, relation classes, and references.
- Runtime evidence: session ID, capability discovery, recommendations,
  invocations, policy denials, approvals, artifacts, logs, outputs, errors, and
  quality metrics.
- Conversation transcript: speaker, phase, input references, output references,
  evidence references, tool references, and timestamp.
- Claim ledger: every important claim, its status, supporting evidence,
  opposing evidence, assumptions, validation method, and owner.
- Meeting minutes: agenda, attendees/roles, decisions, dissent, unresolved
  questions, action items, deadlines, and escalation points.
- Scenario branches: reconstructed past scenario IDs, hypothetical future
  scenario IDs, assumptions, confidence, and validation status.
- Evaluation artifacts: deterministic check results, judge prompts, judge
  outputs, score components, failure taxonomy, and baselines compared.

Current gap: Choreographer has domain events and proposals, but it does not yet
persist an explicit turn-level transcript, claim ledger, meeting minutes, or
scenario branch references. That is the main instrumentation gap before serious
ceremony evaluation.

## Evaluation Dimensions

| Dimension | What it checks | Evidence source |
| --- | --- | --- |
| Problem fit | Ceremony matches problem uncertainty, decomposition, risk, and need for evidence | `ProblemSpec`, `CeremonySpec`, baseline comparisons |
| Meeting fidelity | Agenda, roles, minutes, dissent, decisions, and action items are explicit | Transcript, meeting minutes, phase trace |
| Evidence fidelity | Final output cites required evidence and avoids unsupported claims | Kernel rendered context, content hash, relation explanations, references |
| Claim validation | Important claims are verified, rejected, or marked as assumptions | Claim ledger, runtime tool results, kernel references |
| Past replay | Reconstructed context preserves causal path, evidence, and known history | Kernel context, relation explanations, ground truth |
| Future planning | Future scenarios include explicit assumptions, branches, risks, and validation steps | Scenario branches, runtime checks, evaluator rubrics |
| Process integrity | Phases, turn policy, critique/revision loops, and stop criteria were followed | Choreographer events and transcript |
| Runtime governance | Tool use respected policies, approvals, side-effect budget, and denials | Runtime invocations, policy decisions, artifacts |
| Output correctness | Final output satisfies contract, task constraints, and success criteria | Validators, ground truth, LLM judge when needed |
| Traceability | Claims map to proposal, context, invocation, artifact, or validation evidence | Correlation/causation IDs, references, artifacts |
| Efficiency | Token, latency, tool count, cost, context compression, and retries are acceptable | Choreographer timing, kernel metrics, runtime quality metrics |
| Robustness | Ceremony survives noise, missing context, alternate budgets, and ablations | Re-run matrix and perturbation tests |
| Safety | No fabricated evidence, unauthorized actions, hidden side effects, or ignored denials | Runtime policy, kernel provenance, evaluator checks |

## Scoring Proposal

Default score: 100 points.

| Component | Weight |
| --- | ---: |
| Evidence fidelity and claim validation | 20 |
| Output correctness and contract validity | 15 |
| Meeting fidelity and process integrity | 15 |
| Past replay and future scenario quality | 15 |
| Runtime governance | 15 |
| Traceability | 10 |
| Robustness and ablation performance | 5 |
| Efficiency | 5 |

Hard gates:

- Invalid final output contract caps score at 40.
- Missing required evidence caps score at 60.
- Hallucinated evidence caps score at 50.
- High-impact claim left unverified and unmarked as an assumption caps score at
  60.
- Future plan without explicit assumptions caps score at 70.
- Past replay that omits known causal evidence caps score at 70.
- Policy-denied action treated as success caps score at 30.
- Unauthorized high-risk side effect fails the run.
- Unavailable or unverifiable trace fails the evaluation, not necessarily the
  ceremony. The evaluator must distinguish product failure from observability
  failure.

## Evaluation Algorithm

1. Load or generate a `ProblemSpec`.
2. Materialize known evidence into the kernel using projection events or
   `GraphBatch` fixtures.
3. Create a runtime session with explicit principal, allowed paths, tool
   profile, and metadata.
4. Run the candidate `CeremonySpec` through an app-layer shell that calls
   Choreographer, kernel, and runtime through adapters.
5. Capture `RunTrace` with Choreographer events, kernel render metadata,
   runtime invocations, artifacts, transcript entries, and correlation IDs.
6. Extract the meeting minutes, claim ledger, decisions, unresolved questions,
   action items, and scenario branches.
7. Run deterministic validators first: schema, required evidence, phase order,
   allowed tools, policy compliance, artifact presence, claim evidence status,
   and scenario assumption completeness.
8. Run bounded LLM judging only for qualitative deltas: critique usefulness,
   meeting quality, reasoning quality, completeness, and problem-fit rationale.
   Judge prompts must include the evidence bundle and must not see unsupported
   hidden data.
9. Compare against baselines: direct single agent, Choreographer peer-review
   without kernel, peer-review with kernel, debate, and SOP pipeline.
10. Emit a scorecard, hard-gate status, failure taxonomy, and ceremony
   improvement recommendations.

## Architecture Implications

- Choreographer remains the ceremony orchestrator. It should not know kernel
  graph semantics, runtime tool catalogs, Kubernetes, or product nouns.
- A thin app-layer integration shell should map concrete provider evidence into
  `ExternalContextBundle`, `Task.attributes`, and generic output contracts.
- In the Underpass deployment, that shell should bind context/scenario
  capabilities to the kernel and tool/action capabilities to the runtime
  client.
- The kernel is our first context/evidence provider. It should not decide which
  ceremony is correct.
- The runtime client is our first governed action provider. It should not
  become hidden memory or unrestricted tool access.
- Evaluation should be a separate bounded component that consumes traces from
  all three systems.
- Kubernetes jobs in namespace `underpass-runtime` are the right place for
  end-to-end evaluator runs once the offline evaluator is stable.

## Recommended Implementation Slices

1. Freeze the initial meeting-type catalog as documentation: intake, evidence
   review, past replay, future scenario planning, decision council, design
   review, red-team, production incident resolution, execution readiness, live
   coordination, post-action learning, and handoff.
2. Add schema docs for `ProblemSpec`, `CeremonySpec`, `RunTrace`, and
   `EvaluationSpec`.
3. Add meeting-oriented schema docs for agenda, minutes, claim ledger,
   decisions, action items, and scenario branches.
4. Add turn-level trace/evidence-reference instrumentation to Choreographer
   using domain-agnostic names only.
5. Build an offline evaluator over fixtures from `rehydration-kernel` testkit
   and `underpass-runtime` e2e artifacts.
6. Add deterministic validators for contract validity, phase order, required
   evidence, claim validation, scenario assumptions, runtime policy compliance,
   and trace completeness.
7. Add a bounded LLM judge only after deterministic checks exist.
8. Add baseline ceremony runners: direct, peer-review, peer-review+kernel,
   debate, and SOP pipeline.
9. Promote the evaluator to a Kubernetes Job in namespace `underpass-runtime`
   after offline fixtures are stable.

## Open Design Questions

- Should `CeremonySpec` live as a versioned YAML/JSON schema first, or as Rust
  value objects in Choreographer with adapters later?
- Do we need a new Choreographer `ConversationTrace` aggregate, or should trace
  be an observer/adaptor concern?
- How strict should evidence citation be: every final claim, every decision,
  or only claims marked as high risk by the problem spec?
- Should runtime recommendation decisions count as evidence of process quality,
  or only as explanatory metadata?
- Should kernel `context.bundle.generated` be implemented before the evaluator
  leaves offline mode?

## Source Links

- AutoGen multi-agent conversation framework: <https://arxiv.org/abs/2308.08155>
- AutoGen conversation patterns docs: <https://autogenhub.github.io/autogen/docs/tutorial/conversation-patterns/>
- CAMEL role-playing communicative agents: <https://arxiv.org/abs/2303.17760>
- MetaGPT SOP-based multi-agent collaboration: <https://arxiv.org/abs/2308.00352>
- ChatDev chat-chain software development agents: <https://arxiv.org/abs/2307.07924>
- Multiagent debate: <https://arxiv.org/abs/2305.14325>
- Self-Refine: <https://arxiv.org/abs/2303.17651>
- Reflexion: <https://arxiv.org/abs/2303.11366>
- Generative Agents: <https://arxiv.org/abs/2304.03442>
- AgentBench: <https://arxiv.org/abs/2308.03688>
- GAIA: <https://arxiv.org/abs/2311.12983>
- LLM-as-a-judge with MT-Bench and Chatbot Arena: <https://arxiv.org/abs/2306.05685>
- Agent-as-a-Judge: <https://arxiv.org/abs/2410.10934>
- LangChain multi-agent docs: <https://docs.langchain.com/oss/python/langchain/multi-agent/index>
- CrewAI process docs: <https://docs.crewai.com/en/concepts/processes>
