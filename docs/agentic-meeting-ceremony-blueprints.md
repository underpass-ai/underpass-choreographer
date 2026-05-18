# Agentic Meeting Ceremony Blueprints

Status: initial blueprint catalog. These are product-agnostic meeting designs
optimized for the Underpass sibling capabilities: rehydration kernel as the
context/scenario provider and runtime client as the governed tool runtime.

Choreographer itself remains agnostic and independent. The sibling
systems named here are example providers for study and evaluation, not
required dependencies of the Choreographer product.

Date: 2026-04-26

Related research: `docs/agentic-conversation-ceremony-evaluation-research.md`.

## Design Goal

A meeting should produce a better result than a single agent by forcing the
system to:

- reconstruct relevant past context before deciding;
- state assumptions before planning future scenarios;
- validate important claims with evidence;
- use governed tools instead of unsupported reasoning;
- preserve dissent and unresolved questions;
- emit auditable decisions and action items.

Contracts stay generic. A meeting may require `context.replay`,
`context.evidence_pack`, `scenario.branching`, `tool.discovery`,
`tool.invocation`, `artifact.capture`, or `policy.validation`, but it must not
require concrete kernel or runtime types. Underpass adapters bind those
capabilities to the kernel and runtime client.

## Common Output Objects

These names are generic and can be represented as JSON/YAML contracts later.

- `MeetingMinutes`: agenda, participants, phase log, decisions, dissent,
  unresolved questions, action items, and timestamps.
- `ClaimLedger`: claim ID, speaker, status, evidence refs, opposing evidence,
  assumptions, validation method, confidence, and owner.
- `EvidenceBundleRef`: provider, query/focus, content hash, rendered mode,
  tier, token count, references, and quality metrics.
- `DecisionRecord`: options considered, selected option, rationale, evidence
  refs, dissent, constraints, risks, and follow-up actions.
- `ScenarioBranch`: branch ID, parent scenario, assumptions, expected effects,
  risks, validation status, and evidence refs.
- `ActionItem`: owner, action, preconditions, tool/action refs, due state,
  approval state, and rollback or escalation hint.
- `EvaluationScorecard`: hard gates, weighted scores, missing evidence,
  process violations, and improvement recommendations.

## Shared Meeting Phases

Meeting blueprints compose these generic phases:

- `prepare`: load problem, constraints, prior context, and output contracts.
- `frame`: agree what question the meeting must answer.
- `rehydrate`: request past/context evidence from a context provider.
- `branch`: construct future or alternative scenario branches.
- `propose`: produce candidate answers, options, or plans.
- `challenge`: critique claims, assumptions, omissions, and risks.
- `validate`: call governed tools through a tool runtime and attach artifacts.
- `revise`: update proposals and claim status based on evidence.
- `decide`: accept, reject, defer, escalate, or split decisions.
- `record`: emit minutes, ledgers, decisions, artifacts, and next actions.
- `evaluate`: score meeting quality and identify missing evidence.

## Meeting Blueprints

### 1. Intake Meeting

Purpose: turn an ambiguous request into a framed problem and select the next
meeting type.

Best when: the input is under-specified, evidence paths are unclear, or the
system must avoid jumping into execution.

Quality hypothesis: result quality improves when the system first separates
the problem, missing evidence, constraints, risk level, and suitable ceremony.

Roles:

- `facilitator`: keeps the agenda bounded.
- `problem-framer`: rewrites the problem in neutral terms.
- `evidence-scout`: identifies missing context and evidence needs.
- `operator`: discovers available tool capabilities.
- `recorder`: emits the frame and next meeting recommendation.

Phases:

1. `prepare`: load prompt, constraints, and any provided external context.
2. `rehydrate`: request compact related context using L0/L1 focus.
3. `validate`: discover low-risk read-only tool families through the tool
   runtime.
4. `frame`: produce `ProblemFrame`, open questions, and risk budget.
5. `decide`: choose the next meeting type and required evidence.
6. `record`: emit minutes and evidence requests.

Context policy:

- Requires `context.summary` and optional `context.causal_spine`.
- Prefer compact mode; do not request full evidence packs unless risk is high.

Tool policy:

- Allows `tool.discovery` and low-risk read-only probes only.
- No side-effecting actions.

Required outputs:

- `MeetingMinutes`
- `ProblemFrame`
- `EvidenceRequest`
- `NextCeremonyDecision`

Hard gates:

- Fails if it recommends an execution meeting while high-impact claims are
  still unframed.
- Caps score at 70 if no evidence path is proposed.

### 2. Evidence Review Meeting

Purpose: verify, reject, or mark as assumption the claims that matter for a
decision.

Best when: the main risk is hallucination, weak evidence, or hidden
unsupported assumptions.

Quality hypothesis: result quality improves when every high-impact claim has
evidence status before decisions are made.

Roles:

- `claim-owner`: states candidate claims.
- `evidence-reviewer`: maps claims to evidence.
- `challenger`: searches for contradictions and missing context.
- `operator`: validates claims with governed tools.
- `judge`: decides claim status.
- `recorder`: maintains the claim ledger.

Phases:

1. `prepare`: load claims, criteria, and risk budget.
2. `rehydrate`: request evidence packs and relation explanations.
3. `challenge`: identify unsupported, ambiguous, or conflicting claims.
4. `validate`: run governed checks and attach artifacts.
5. `revise`: update claim wording and confidence.
6. `decide`: mark each claim as verified, rejected, assumption, or unresolved.
7. `record`: emit claim ledger and validation report.

Context policy:

- Requires `context.evidence_pack`, provenance, content hash, and relation
  explanations.
- Prefer reason-preserving mode for high-risk claims.

Tool policy:

- Allows read-only validation and bounded diagnostic tools.
- Requires artifacts for successful validation.
- Policy denials must be recorded as evidence, not hidden.

Required outputs:

- `ClaimLedger`
- `EvidenceBundleRef`
- `ValidationReport`
- `UnresolvedQuestion`

Hard gates:

- Fails if a high-impact claim is accepted without evidence.
- Caps score at 50 if evidence refs cannot be traced to a provider output.

### 3. Past Replay Meeting

Purpose: reconstruct a past scenario with causal continuity and known evidence.

Best when: correctness depends on understanding what happened before, why it
happened, and what evidence was available at the time.

Quality hypothesis: result quality improves when the meeting starts from a
replayed causal context rather than a fresh short-context summary.

Roles:

- `timeline-builder`: reconstructs sequence.
- `causal-analyst`: identifies causal and constraint relations.
- `evidence-reviewer`: checks provenance and missing facts.
- `operator`: retrieves read-only artifacts.
- `challenger`: tests alternate explanations.
- `recorder`: emits replay timeline and known unknowns.

Phases:

1. `prepare`: define replay target, boundaries, and expected ground truth if
   available.
2. `rehydrate`: request reason-preserving replay and causal spine.
3. `validate`: retrieve logs, metrics, files, or prior artifacts via governed
   read-only tools.
4. `challenge`: compare alternate causal explanations.
5. `revise`: update causal path and confidence.
6. `decide`: classify known facts, likely causes, and unknowns.
7. `record`: emit replay timeline and causal path.

Context policy:

- Requires `context.replay`, `context.causal_spine`, relation explanations,
  and evidence references.
- If available, includes temporal deltas or before/after bundles.

Tool policy:

- Allows read-only artifact retrieval and diagnostics.
- No mutation or remediation actions.

Required outputs:

- `ReplayTimeline`
- `CausalPath`
- `ClaimLedger`
- `KnownUnknowns`

Hard gates:

- Caps score at 70 if known causal evidence is omitted.
- Fails if the meeting fabricates unavailable past evidence.

### 4. Future Scenario Planning Meeting

Purpose: compare possible future branches with explicit assumptions, risks, and
validation steps.

Best when: the problem is about planning, tradeoffs, sequencing, or possible
future outcomes.

Quality hypothesis: result quality improves when future plans are represented
as scenario branches with assumptions and validation status, not as a single
unchecked prediction.

Roles:

- `scenario-designer`: creates branches.
- `assumption-owner`: states assumptions explicitly.
- `risk-reviewer`: identifies failure modes and constraints.
- `operator`: runs simulations, dry runs, or feasibility checks.
- `challenger`: attacks optimistic branches.
- `recorder`: emits branches and decision options.

Phases:

1. `prepare`: load objective, constraints, and acceptable risk.
2. `rehydrate`: retrieve current state and relevant historical analogs.
3. `branch`: construct candidate scenario branches.
4. `challenge`: identify assumptions and risks per branch.
5. `validate`: run feasible dry runs, policy checks, simulations, or probes.
6. `revise`: update scenario confidence and branch status.
7. `decide`: recommend branch, defer, or request more evidence.
8. `record`: emit scenario branches and decision options.

Context policy:

- Requires current context and similar historical scenarios if available.
- Scenario branches are application-supplied hypothetical graph updates or
  generic branch records, then rendered by the context provider.

Tool policy:

- Allows safe dry runs, simulations, policy checks, cost checks, and read-only
  feasibility probes.
- Side-effecting actions require explicit approval and should normally be
  deferred to an execution readiness meeting.

Required outputs:

- `ScenarioBranch`
- `AssumptionLedger`
- `RiskRegister`
- `DecisionOptions`

Hard gates:

- Caps score at 70 if future branches lack assumptions.
- Caps score at 60 if a recommendation ignores branch-specific validation
  failures.

### 5. Decision Council

Purpose: choose among competing options under evidence, constraints, and risk.

Best when: there are multiple viable options and a decision must be auditable.

Quality hypothesis: result quality improves when options are compared against
evidence, constraints, dissent, and runtime feasibility before selection.

Roles:

- `option-owner`: presents one or more options.
- `evidence-reviewer`: maps options to evidence.
- `tradeoff-analyst`: compares benefits, costs, and constraints.
- `challenger`: preserves dissent and risks.
- `operator`: validates feasibility and side effects.
- `judge`: selects, rejects, defers, or escalates.
- `recorder`: emits decision record and actions.

Phases:

1. `prepare`: load options, constraints, and success criteria.
2. `rehydrate`: fetch context for each option.
3. `propose`: produce structured option records.
4. `challenge`: identify missing evidence, tradeoffs, and dissent.
5. `validate`: check feasibility and policy for candidate actions.
6. `revise`: update options and confidence.
7. `decide`: select, reject, defer, split, or escalate.
8. `record`: emit decision and action items.

Context policy:

- Requires comparable evidence for each option.
- Must preserve dissent-relevant evidence, not only evidence supporting the
  winning option.

Tool policy:

- Allows feasibility checks, side-effect estimation, policy validation, and
  read-only artifact retrieval.
- Mutating actions belong in follow-up execution ceremonies.

Required outputs:

- `DecisionRecord`
- `DissentRecord`
- `TradeoffMatrix`
- `ActionItem`

Hard gates:

- Caps score at 60 if selected option lacks comparative evidence.
- Caps score at 70 if dissent is omitted rather than resolved or recorded.

### 6. Design Review Meeting

Purpose: challenge a proposed plan before implementation or execution.

Best when: a draft design or plan already exists and the next step could create
cost, risk, or irreversible work.

Quality hypothesis: result quality improves when proposed work is reviewed
against prior context, constraints, tests, policy, and alternative designs.

Roles:

- `proposal-owner`: explains the design.
- `reviewer`: checks correctness and completeness.
- `constraint-reviewer`: checks boundaries and non-goals.
- `operator`: runs static checks or compatibility probes.
- `challenger`: proposes counterexamples.
- `recorder`: emits findings and revision requests.

Phases:

1. `prepare`: load proposal, success criteria, and constraints.
2. `rehydrate`: retrieve related decisions, dependencies, and prior failures.
3. `challenge`: identify gaps, contradictions, and risky assumptions.
4. `validate`: run static checks, tests, compatibility probes, or policy
   checks where possible.
5. `revise`: convert findings into revision requests.
6. `decide`: approve, approve with changes, reject, or defer.
7. `record`: emit review findings and approval decision.

Context policy:

- Requires prior decisions, constraints, dependency context, and relevant
  evidence.

Tool policy:

- Allows static, read-only, test, and policy-checking tools.
- Any mutation must be explicitly approved and recorded.

Required outputs:

- `ReviewFinding`
- `ClaimLedger`
- `RevisionRequest`
- `ApprovalDecision`

Hard gates:

- Caps score at 60 if approval is granted despite unresolved high-risk
  findings.
- Fails if a tool denial is ignored in the approval rationale.

### 7. Red-Team Meeting

Purpose: stress-test assumptions, safety, policy, and failure modes.

Best when: risk, adversarial behavior, hidden coupling, or safety envelope is
more important than speed.

Quality hypothesis: result quality improves when the meeting deliberately
searches for failure paths and records mitigations before action.

Roles:

- `defender`: states intended plan and constraints.
- `red-teamer`: searches for attack/failure paths.
- `policy-reviewer`: checks risk and approval boundaries.
- `operator`: runs safe probes and policy simulations.
- `judge`: classifies severity and mitigation status.
- `recorder`: emits risk register and mitigation plan.

Phases:

1. `prepare`: load plan, constraints, risk budget, and safety requirements.
2. `rehydrate`: retrieve similar failures, constraints, and weak signals.
3. `challenge`: generate failure paths and adversarial cases.
4. `validate`: execute safe probes or policy simulations.
5. `revise`: attach mitigation and residual risk.
6. `decide`: block, allow with mitigations, or escalate.
7. `record`: emit risks, mitigations, and denied actions.

Context policy:

- Requires related failure context, constraints, and risk-relevant evidence.

Tool policy:

- Allows only safe probes, read-only diagnostics, and policy simulations.
- Denied actions are positive evidence that controls work.

Required outputs:

- `RiskRegister`
- `AttackPath`
- `MitigationPlan`
- `DeniedActionRecord`

Hard gates:

- Fails if high-risk actions are executed without approval.
- Caps score at 50 if severe risks are not mapped to mitigations or explicit
  acceptance.

### 8. Production Incident Resolution Meeting

Purpose: investigate a production incident in a software application and decide
the safest corrective path.

Best when: a live or recent production problem needs diagnosis, evidence-backed
root cause analysis, remediation options, and a go/no-go decision before
applying a fix.

Quality hypothesis: result quality improves when the meeting combines past
replay, live evidence collection, competing hypotheses, risk review, and
runtime feasibility checks before selecting a remediation.

Roles:

- `incident-facilitator`: keeps scope, severity, and stop criteria explicit.
- `timeline-builder`: reconstructs what changed and when.
- `hypothesis-owner`: proposes possible causes and fixes.
- `evidence-reviewer`: maps symptoms and hypotheses to evidence.
- `operator`: runs governed diagnostic, read-only, test, or dry-run tools.
- `risk-reviewer`: checks blast radius, rollback, and safety constraints.
- `judge`: selects remediation, defers, escalates, or requests more evidence.
- `recorder`: emits incident analysis, decision, and action plan.

Phases:

1. `prepare`: load incident statement, severity, affected surface, constraints,
   and current risk budget.
2. `rehydrate`: replay relevant past context, recent changes, causal spine,
   prior similar incidents, and current state.
3. `propose`: generate competing root-cause hypotheses and remediation options.
4. `challenge`: test each hypothesis against available evidence and search for
   contradictions or missing signals.
5. `validate`: run governed diagnostics, log/metric/file reads, tests, policy
   checks, dry runs, or impact checks through the tool runtime.
6. `revise`: update claim ledger, root-cause confidence, remediation options,
   and rollback assumptions.
7. `decide`: choose fix, mitigation, rollback, defer, or escalate.
8. `record`: emit incident report, evidence refs, decision, action plan,
   readiness requirements, and follow-up learning tasks.

Context policy:

- Requires `context.replay`, `context.causal_spine`, current focused context,
  prior similar scenario lookup, and evidence refs.
- Should preserve causation across symptom, change, hypothesis, validation,
  decision, and remediation records.
- Future branches may model candidate remediation paths and rollback paths as
  explicit `ScenarioBranch` records.

Tool policy:

- Allows read-only diagnostics, log/metric/file inspection, test probes,
  policy checks, dry runs, recommendation evidence, and artifact capture.
- Side-effecting remediation requires explicit approval and should normally be
  handed to `Execution Readiness Meeting` unless the incident policy allows
  immediate controlled action.
- Runtime denials, failed probes, and missing permissions are evidence and must
  remain in the record.

Required outputs:

- `IncidentAnalysis`
- `ReplayTimeline`
- `ClaimLedger`
- `RootCauseHypothesis`
- `RemediationOption`
- `DecisionRecord`
- `RiskRegister`
- `ActionItem`
- `RollbackPlanRef`

Hard gates:

- Fails if a remediation is selected without evidence for the accepted root
  cause or mitigation rationale.
- Fails if a high-risk production action is executed without required approval.
- Caps score at 60 if rollback or blast-radius assumptions are missing.
- Caps score at 70 if similar past incidents or recent changes are not checked
  when available.

### 9. Execution Readiness Meeting

Purpose: decide go/no-go before a governed action.

Best when: a planned action is imminent and must pass evidence, policy, and
rollback checks.

Quality hypothesis: result quality improves when execution is separated from
readiness and all preconditions are verified before action.

Roles:

- `action-owner`: states intended action.
- `readiness-reviewer`: checks preconditions.
- `policy-reviewer`: checks approvals and risk.
- `operator`: runs dry runs and environment checks.
- `rollback-reviewer`: verifies rollback or escape path.
- `judge`: issues go/no-go.
- `recorder`: emits checklist and decision.

Phases:

1. `prepare`: load intended action, constraints, and approval requirements.
2. `rehydrate`: retrieve current state, blockers, and dependencies.
3. `validate`: verify approvals, tool availability, environment state, and dry
   run artifacts.
4. `challenge`: identify missing rollback, unknowns, or policy gaps.
5. `revise`: update action plan and preconditions.
6. `decide`: go, no-go, defer, or escalate.
7. `record`: emit checklist, decision, action plan, and rollback refs.

Context policy:

- Requires current state, dependencies, blockers, and relevant evidence.

Tool policy:

- Allows dry runs, policy checks, environment checks, and readiness probes.
- Mutating execution should happen only after go decision and approval.

Required outputs:

- `ReadinessChecklist`
- `GoNoGoDecision`
- `ActionPlan`
- `RollbackPlanRef`

Hard gates:

- Fails if go is issued without required approval evidence.
- Caps score at 60 if rollback/precondition evidence is missing.

### 10. Live Coordination Meeting

Purpose: coordinate multi-step work while evidence changes during the meeting.

Best when: the meeting must interleave reasoning, context refresh, tool
invocation, and decision updates.

Quality hypothesis: result quality improves when each step updates the claim
ledger and context before the next action.

Roles:

- `facilitator`: controls phase and stop criteria.
- `operator`: invokes governed tools.
- `evidence-reviewer`: updates evidence and claim status.
- `challenger`: checks whether new evidence invalidates prior decisions.
- `recorder`: emits phase log and artifact index.

Phases:

1. `prepare`: load objective, risk budget, and stop criteria.
2. `rehydrate`: load current focused context.
3. `propose`: choose next step.
4. `validate`: invoke governed tool or retrieve artifact.
5. `revise`: update claim ledger, context needs, and action plan.
6. Repeat `rehydrate` through `revise` until stop criteria.
7. `decide`: finalize, defer, escalate, or hand off.
8. `record`: emit phase log, artifacts, and open actions.

Context policy:

- Requires focused refresh between material steps.
- Must preserve causation across context updates.

Tool policy:

- Allows governed invocation inside risk budget.
- Requires artifact capture for each material action.

Required outputs:

- `PhaseLog`
- `ClaimLedger`
- `ArtifactIndex`
- `ActionItem`

Hard gates:

- Caps score at 50 if actions are taken without updating the claim ledger.
- Fails if a denied action is retried through another path without approval.

### 11. Post-Action Learning Meeting

Purpose: learn from an executed action and improve future decisions.

Best when: an action completed, failed, was denied, or produced unexpected
evidence.

Quality hypothesis: result quality improves when outcomes are compared against
pre-action assumptions and runtime evidence is converted into future guidance.

Roles:

- `outcome-reviewer`: compares expected and actual outcome.
- `evidence-reviewer`: links artifacts and metrics.
- `assumption-reviewer`: updates assumptions.
- `operator`: retrieves invocation quality and artifacts.
- `recorder`: emits learning record and follow-ups.

Phases:

1. `prepare`: load action plan, expected outcome, and prior assumptions.
2. `rehydrate`: retrieve before/after context and causal deltas.
3. `validate`: retrieve invocation quality, logs, artifacts, decisions, and
   denials.
4. `challenge`: identify incorrect assumptions and missed signals.
5. `revise`: update assumptions and future guidance.
6. `decide`: accept learning, request follow-up, or escalate.
7. `record`: emit learning record and follow-up actions.

Context policy:

- Requires before/after context and causal deltas where available.

Tool policy:

- Allows artifact retrieval, quality metrics, telemetry, and read-only
  analysis tools.

Required outputs:

- `LearningRecord`
- `OutcomeAssessment`
- `UpdatedAssumption`
- `FollowupAction`

Hard gates:

- Caps score at 60 if actual runtime evidence is not consulted.
- Caps score at 70 if failed assumptions are not recorded.

### 12. Escalation And Handoff Meeting

Purpose: transfer context, evidence, decisions, and responsibility across a
boundary.

Best when: another actor, system, or meeting must continue the work.

Quality hypothesis: result quality improves when handoff includes causal
context, decisions, evidence refs, artifacts, unresolved questions, and
responsibility mapping instead of a lossy summary.

Roles:

- `handoff-owner`: states transfer goal.
- `context-curator`: selects resume-focused context.
- `evidence-reviewer`: verifies included evidence.
- `operator`: packages artifacts and pending approvals.
- `receiver-advocate`: asks what the next actor needs.
- `recorder`: emits handoff brief and responsibility map.

Phases:

1. `prepare`: identify receiver, boundary, and continuation goal.
2. `rehydrate`: request resume-focused context and unresolved evidence.
3. `validate`: package artifacts, invocation refs, and policy state.
4. `challenge`: test whether receiver can continue without hidden context.
5. `revise`: fill missing links and responsibilities.
6. `decide`: hand off, defer, or escalate.
7. `record`: emit handoff brief and open decisions.

Context policy:

- Requires resume-focused context, causal spine, unresolved questions, and
  evidence refs.

Tool policy:

- Allows artifact packaging, read-only state checks, and approval-state lookup.

Required outputs:

- `HandoffBrief`
- `OpenDecision`
- `EvidenceBundleRef`
- `ResponsibilityMap`

Hard gates:

- Caps score at 50 if handoff lacks evidence refs for open decisions.
- Caps score at 60 if ownership of next actions is ambiguous.

## First Reference Set

The first implementation should not attempt all meeting types at once. The
highest leverage sequence is:

1. `Evidence Review Meeting`: establishes claim validation and evidence refs.
2. `Past Replay Meeting`: exercises kernel replay and causal context.
3. `Production Incident Resolution Meeting`: combines replay, diagnostics,
   hypotheses, remediation options, runtime evidence, and risk review.
4. `Future Scenario Planning Meeting`: exercises scenario branching and
   assumptions.
5. `Decision Council`: combines evidence, replay, scenario branches, and
   runtime-client feasibility checks into an auditable decision.
6. `Execution Readiness Meeting`: gates real tool/action execution.

This sequence maximizes use of both siblings while keeping Choreographer
contracts generic.
