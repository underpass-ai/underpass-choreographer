# PIR ↔ Choreographer Integration Design

> **Archived 2026-07-05.** Legacy case-study design for PIR, an external
> paused product; never implemented here and explicitly out of scope. Its
> report-materialization approach was superseded by the Epic 10 decision
> (Report = output contract via JSON Schema, no dedicated entity).

Snapshot date: 2026-04-25

Historical status: this is a legacy PIR product-integration design.
PIR is owned outside this repository, and this file is retained for
context only. It is a case-study document, not a product dependency or
roadmap requirement. Current Choreographer readiness lives in
[`backlog.md`](./backlog.md) and [`stack-gap-analysis.md`](./stack-gap-analysis.md).

This document defines how `underpass-choreographer` could participate in
`underpass-payments-incident-response` ("PIR") for complex incidents that
should not escalate directly to a human.

The central idea is:

1. keep PIR as the bounded domain shell for payments incidents
2. add a pre-human expert deliberation stage powered by Choreographer
3. allow that deliberation to either:
   - propose a new bounded remedy event for reevaluation, or
   - conclude that the incident must go to a human
4. when escalation is unavoidable, generate a structured human handoff report
   from the already-materialized kernel graph before the final human escalation

This is not a proposal to replace PIR wholesale with Choreographer.
It is a proposal to insert Choreographer into PIR as a domain-governed
deliberation engine for a narrow part of the incident lifecycle.

Companion documents:

- [`stack-gap-analysis.md`](./stack-gap-analysis.md)
- [`backlog.md`](./backlog.md)

## Executive summary

Today, PIR already implements a domain-specific incident-response architecture:

- bounded event contracts
- deterministic specialist routing
- kernel-first context materialization
- governed runtime execution
- typed outcome events
- traceable graph writes into the rehydration kernel

Choreographer, by contrast, is currently a domain-agnostic deliberation service:

- it accepts opaque trigger events
- it fans out to one or more councils
- it runs proposal / critique / revision rounds
- it publishes deliberation and orchestration lifecycle events

This difference matters.

PIR is already the domain boundary.
Choreographer is not.

Therefore the correct integration shape is:

- PIR remains the event boundary, routing layer, kernel integration layer,
  runtime governance boundary, and terminal incident lifecycle owner.
- Choreographer becomes a reusable expert-deliberation subsystem inside PIR
  for the subset of situations where a single bounded specialist is no longer
  enough, but direct human escalation is still premature.

The proposed new capability is a two-step pre-human flow:

1. `complex-incident-reevaluation`
   - expert council deliberates on the incident bundle
   - may emit a new bounded remedy event
   - may request more evidence
   - may conclude escalation is required
2. `human-handoff-report`
   - expert council synthesizes the graph, prior findings, prior decisions,
     failed remediations, and evidence into a high-quality operator report
   - PIR then emits the final escalation-to-human event

This document argues that this is desirable, but also that it must not begin
until Choreographer is complete enough to behave as a real peer of Runtime and
Kernel rather than as an isolated deliberation prototype.

## Scope

This design is based on:

- this local `underpass-choreographer` checkout
- the current `docs/stack-gap-analysis.md` in this repo
- the sibling local repository `../underpass-payments-incident-response`
- the current PIR architecture and implementation documents

This document does not modify PIR or Choreographer code.
It defines architecture, boundaries, contracts, prerequisites, and migration
order.

## Problem statement

The current bounded PIR model converges quickly and safely for incidents that
fit one specialist or one bounded pipeline. That is a feature, not a bug.

However, there is a class of incidents that are still too complex for direct
handoff from one bounded specialist to a human:

- multiple prior remediation attempts have already failed
- multiple findings exist across different specialist axes
  (for example rollout + saturation + payment integrity)
- the incident state is rich in graph evidence but still ambiguous
- there may still be a safe bounded remediation path, but it is no longer
  obvious to one specialist in isolation

In those cases, escalating directly to a human is often premature.
The system should first attempt a bounded multi-expert reevaluation over the
materialized incident graph.

If the expert reevaluation still cannot produce a safe next move, the system
should not escalate with only raw evidence and scattered prior outputs.
It should first synthesize the graph into a structured operator report.

The desired behaviour is therefore:

- do not jump directly from `*.escalated` to human
- insert an expert deliberation step first
- rehydrate the incident graph and prior decisions from the kernel
- let a bounded expert council decide whether:
  - a new bounded remedy path exists
  - more evidence is required
  - only human escalation remains
- if human escalation remains, generate a high-signal report before handing off

## Why PIR must remain the shell

PIR is intentionally domain-specific.
Its own architecture states that it is:

- not a general incident agent
- not a tool executor
- not a memory store
- not a routing fabric for unrelated event classes

The important implication is that PIR owns the bounded domain contract for
payments incidents.

That contract includes:

- event families
- specialist bindings
- decision spaces
- tool profiles
- governance profiles
- success profiles
- incident graph semantics
- outcome event families

Choreographer does not currently own any of these.
Its README explicitly positions it as a domain-agnostic coordination plane:

- it reacts to domain events
- composes councils
- runs deliberations
- publishes outcome events
- does not embed domain vocabulary

That is exactly why Choreographer should not replace PIR's domain shell.
If it did, one of two bad things would happen:

1. PIR's domain semantics would be duplicated inside Choreographer, destroying
   Choreographer's intended neutrality.
2. Choreographer would be asked to reason from too-open inputs, violating
   PIR's "narrow before reasoning" rule.

The architectural boundary should therefore remain:

- PIR owns bounded payments incident semantics
- Choreographer owns expert deliberation mechanics

## Current-state comparison

### What PIR already has

PIR already implements most of the domain-specific incident loop:

- event boundary
- deterministic specialist routing
- kernel-first context materialization
- runtime-governed execution for some specialists
- bounded outcome events
- observability across the event chain

Current code and docs show:

- a seven-stage incident flow
- durable JetStream-based eventing
- real kernel and runtime adapters
- multiple bounded specialist pipelines
- explicit typed incident events and specialist contracts
- graph writes into the kernel as part of specialist work

For complex incidents, PIR already knows:

- what an incident is
- what a finding is
- what a decision is
- what specialist produced which output
- what bounded event families and subjects exist
- how to preserve incident ids, run ids, correlation ids, and causation chains

### What Choreographer already has

Choreographer already provides:

- councils grouped by specialty
- peer deliberation with proposal / critique / revision rounds
- provider-neutral agent interface
- event-driven trigger ingestion
- deliberation persistence
- orchestration lifecycle events

This is good raw machinery for expert conversations.

### What Choreographer lacked at this snapshot

At the 2026-04-25 snapshot, this repo's stack-gap analysis said
Choreographer was not yet a real stack-integrated peer of Runtime and
Kernel.

Blockers at that time included:

- `NoopExecutor` was still wired in the binary
- `NoopAgentFactory` was still wired in the binary
- the real runtime gRPC executor adapter was not composed
- no kernel integration or context rehydration path existed
- event correlation was partial
- TLS server wiring was missing despite chart surface
- JetStream semantics were not truly implemented even though the docs mentioned it

At that time, Choreographer could host deliberation, but could not yet
truthfully own a production-critical PIR integration point that depends on:

- real kernel-fed context
- real runtime-mediated execution follow-up
- correct causal propagation
- durable transport behaviour
- secure stack-level operation

## Design goal

Introduce a new expert-deliberation layer in PIR for complex incidents,
implemented through Choreographer, without weakening PIR's bounded domain model.

The system should support this sequence:

1. a bounded PIR specialist escalates
2. PIR does not hand off directly to human
3. PIR requests complex reevaluation
4. PIR rehydrates the incident bundle from kernel
5. PIR calls Choreographer with:
   - the bounded incident context
   - the reevaluation contract
   - the allowed decision space
6. Choreographer runs a bounded expert council
7. PIR interprets the structured result
8. if a new remedy is proposed:
   - PIR publishes a new bounded remedy event
   - PIR continues the incident lifecycle
9. if human escalation is required:
   - PIR requests a handoff-report deliberation through Choreographer
   - PIR stores or publishes that report
   - PIR emits the final escalation-to-human event

## Non-goals

This design explicitly does not propose:

- replacing PIR's event catalog with Choreographer triggers
- replacing PIR's specialist catalog with Choreographer councils
- moving kernel graph semantics into Choreographer
- moving runtime governance or execution policy into Choreographer
- letting Choreographer emit arbitrary domain events on its own authority
- turning complex incident handling into an open-ended multi-agent loop

## Core architectural rule

The new expert deliberation layer must preserve PIR's foundational rule:

> narrow before reasoning

That means the Choreographer integration must not be "give the agents the whole
incident and ask what to do".

It must instead be:

- give the council a bounded incident bundle
- give it a bounded role
- give it a closed decision space
- validate the output against a typed contract
- let PIR decide how that output maps back into bounded domain events

If this rule is violated, the integration becomes a generic incident agent
layer and loses PIR's main safety property.

## Proposed target architecture

### Boundary split

#### PIR keeps ownership of

- ingress event translation
- deterministic routing into bounded specialist families
- kernel seeding and graph contribution semantics
- kernel context retrieval
- runtime tool governance
- bounded domain event publication
- final authority over incident lifecycle transitions
- final authority over escalation-to-human

#### Choreographer owns

- expert council composition
- agent-to-agent deliberation mechanics
- candidate proposal generation
- critique / revision process
- scoring and winner selection
- optional execution handoff once a runtime executor exists

### New integration pattern

The best first integration is request/response over gRPC from PIR to
Choreographer, not event-bus coupling.

Why:

- PIR already owns the triggering bounded event
- PIR already owns context materialization from kernel
- PIR already owns the resulting bounded event families
- gRPC makes the boundary explicit and typed
- transport and correlation are simpler for the first production slice

That produces a clear sequence:

1. PIR specialist emits or receives a bounded escalation outcome
2. PIR loads the incident bundle from kernel
3. PIR maps that bundle into a Choreographer request
4. Choreographer returns a structured decision artifact
5. PIR validates and interprets it
6. PIR publishes the next bounded domain event

Event-bus integration between PIR and Choreographer can exist later, but the
first production path should be direct and explicit.

## New logical capabilities

Two new logical capabilities are proposed inside PIR:

1. `complex-incident-reevaluation`
2. `human-handoff-report`

These are not generic features.
They are bounded specialist families owned by PIR, using Choreographer as the
deliberation engine.

### 1. Complex incident reevaluation

Purpose:

- consume complex bounded escalation situations before final human escalation
- inspect the incident graph, prior decisions, and prior failures
- decide if there is still a safe bounded remedy path

Input shape:

- incident identifiers
- bounded summary of the incident graph from kernel
- upstream specialist outcomes
- prior decision nodes
- prior failed remediations
- explicit closed decision space

Closed decision space:

- `emit_new_remedy_event`
- `request_more_evidence`
- `escalate_to_human`
- `not_enough_evidence`

Critically, `emit_new_remedy_event` must not allow arbitrary event emission.
It must require a bounded event type from an allowlist.

Allowed remedy event families should be explicit, for example:

- `payments.incident.runtime-rollout.requested`
- `payments.incident.resource-saturation.planning.requested`
- `payments.incident.payment-integrity.review.requested`
- a new bounded domain event introduced through PIR's event catalog

Output requirements:

- selected decision from the closed set
- confidence
- rationale
- evidence references
- explicit proposed bounded event type when decision is `emit_new_remedy_event`
- explicit payload shape for that proposed bounded event
- explicit falsifiers / reasons when decision is `escalate_to_human`

### 2. Human handoff report

Purpose:

- synthesize a human-quality incident report from the kernel graph
- consolidate prior findings, decisions, failed remediations, and evidence
- ensure the human receives a structured, auditable package rather than only
  raw event fragments

Input shape:

- incident identifiers
- incident graph bundle from kernel
- prior findings
- prior decisions
- prior escalations
- failed runtime invocations
- service / workload / deploy evidence

Closed decision space:

- `report_completed`
- `report_failed`

This council is not deciding remediation.
It is deciding whether it successfully produced the handoff artifact.

Output requirements:

- executive summary
- timeline of major incident events
- hypotheses considered
- remediations attempted and their results
- open risks
- recommended next operator actions
- supporting evidence references

This output should become either:

- a graph node written through PIR into kernel, or
- a bounded domain artifact referenced by the final escalation event, or
- both

## Proposed PIR event model extensions

These names are representative.
Final naming must be negotiated in PIR's event catalog.

### Reevaluation request / result

New inbound family to Choreographer via PIR-owned orchestration:

- `payments.incident.complex-reevaluation.requested`

Possible outcomes:

- `payments.incident.complex-reevaluation.remedy-proposed`
- `payments.incident.complex-reevaluation.more-evidence-requested`
- `payments.incident.complex-reevaluation.escalate-to-human`
- `payments.incident.complex-reevaluation.failed`

### Handoff report request / result

- `payments.incident.handoff-report.requested`
- `payments.incident.handoff-report.completed`
- `payments.incident.handoff-report.failed`

### Terminal human escalation

The current terminal outcome can remain:

- `payments.incident.escalated.to-human`

But after this design, it should be emitted only after:

- complex reevaluation concludes human escalation is required, and
- the handoff report completes successfully or deterministically fails

## Detailed end-to-end flow

### A. Current direct-human path

Current rough path:

1. bounded specialist fails or escalates
2. PIR publishes `*.escalated`
3. `human-escalation` consumes that event
4. human handoff occurs

### B. Proposed future path

1. bounded specialist fails or escalates
2. PIR evaluates whether the escalation is eligible for complex reevaluation
3. PIR loads the incident bundle from kernel
4. PIR creates a `complex-incident-reevaluation` request
5. PIR calls Choreographer
6. Choreographer runs the reevaluation council
7. Choreographer returns a structured result
8. PIR validates the result contract
9. PIR branches:

- if `emit_new_remedy_event`:
  - PIR publishes the proposed bounded remedy event
  - incident remains in the machine loop

- if `request_more_evidence`:
  - PIR publishes a bounded evidence-request event or routes into a known
    investigation specialist

- if `escalate_to_human`:
  - PIR loads full incident bundle from kernel again if needed
  - PIR requests `human-handoff-report`
  - Choreographer runs the report council
  - PIR stores the report artifact / graph node
  - PIR emits `payments.incident.escalated.to-human`

- if `failed`:
  - PIR emits the appropriate deterministic terminal failure event

## Council design inside Choreographer

The councils used for this integration must not be open-ended generic swarms.
They should be small, role-bounded, and contract-driven.

### Reevaluation council

Representative roles:

- `incident-synthesis-expert`
- `runtime-remediation-expert`
- `risk-and-governance-expert`

Role intent:

- synthesis expert:
  - summarize what the graph says now
- runtime remediation expert:
  - determine whether a bounded safe next event exists
- risk/governance expert:
  - reject proposals that violate the domain's safety envelope

This should usually be a 3-agent council, not a large swarm.

### Handoff report council

Representative roles:

- `timeline-reconstruction-expert`
- `technical-diagnosis-expert`
- `operator-handoff-writer`

Role intent:

- timeline expert:
  - reconstruct key sequence of events from graph and outcomes
- diagnosis expert:
  - summarize technical findings and failed hypotheses
- handoff writer:
  - produce the final operator-facing report artifact

This may be 2 or 3 agents depending on cost and latency.

## Why this must be contract-driven

PIR specialists today rely on bounded decision spaces and explicit output
contracts.

Choreographer's native model today is freer:

- proposals are free-form text
- critique and revision are free-form text
- validators are generic
- trigger payload is opaque

That is acceptable for generic coordination, but it is not enough for this PIR
integration.

For this use case, Choreographer must support specialist-grade structured
output, meaning:

- every council must emit a typed JSON result
- that result must be validated against a closed schema
- invalid outputs must deterministically fail rather than leak back into PIR

Examples:

### Reevaluation output contract

```json
{
  "decision": "emit_new_remedy_event",
  "confidence": "medium",
  "reason": "The prior rollout mitigation failed because the wrong workload was targeted; the incident graph now points to saturation in the primary deployment.",
  "proposed_event_type": "payments.incident.resource-saturation.planning.requested",
  "proposed_payload": {
    "incident_id": "inc-123",
    "incident_run_id": "run-123",
    "specialist_id": "saturation-planner",
    "reason": "reevaluation-followup"
  },
  "evidence_refs": [
    "finding:inc-123:regression",
    "decision:inc-123:runtime-rollout",
    "workload:deployment/payments-api@underpass-runtime"
  ],
  "falsifiers": [
    "No direct payment-integrity signal exists in the current bundle."
  ]
}
```

### Handoff report output contract

```json
{
  "decision": "report_completed",
  "report": {
    "executive_summary": "...",
    "incident_timeline": [
      "...",
      "..."
    ],
    "findings": [
      "...",
      "..."
    ],
    "actions_attempted": [
      "...",
      "..."
    ],
    "open_risks": [
      "...",
      "..."
    ],
    "recommended_human_actions": [
      "...",
      "..."
    ]
  },
  "evidence_refs": [
    "incident:inc-123",
    "finding:inc-123:saturation",
    "decision:inc-123:payment-integrity"
  ]
}
```

## What "Choreographer complete" means for this project

The user requirement is correct:

> To start building this, Choreographer must first be complete.

In this context, "complete" does not mean feature-perfect forever.
It means complete enough to be a trustworthy stack peer for PIR.

The minimum required definition of done is below.

### 1. Runtime integration must be real

Choreographer cannot participate in PIR's complex incident flow while still
running with `NoopExecutor`.

Required:

- real runtime gRPC executor adapter
- mTLS-capable client wiring
- session metadata support for specialist execution
- integration test against a real or stubbed runtime surface

Without this, Choreographer cannot be trusted as part of a remediation path.

### 2. Kernel context integration must be real

The reevaluation and handoff councils must reason over the actual incident
bundle, not over a free-form payload assembled by hand.

Required:

- explicit kernel integration boundary in Choreographer or a PIR-side
  materialization contract that Choreographer consumes
- reproducible shape for incident bundle input
- tests proving the same incident graph can drive a deterministic council call

### 3. Structured output contracts must be first-class

Free-form winner proposals are not enough for PIR.

Required:

- typed structured council outputs
- schema validation for council winners
- deterministic error classification on invalid outputs
- contract tests

### 4. Event causality must be complete

PIR relies on:

- `incident_id`
- `incident_run_id`
- `correlation_id`
- `causation_id`
- specialist identity

Choreographer currently only models a subset of that cleanly.

Required:

- propagation of correlation through the full event lifecycle
- support for upstream trigger / source event reference
- ability to carry PIR incident ids and run ids either natively or via a typed
  metadata surface

### 5. Provider wiring must be real

The councils in this design are not `noop` councils.

Required:

- provider-backed agent factories actually wired in the binary
- ability to register or resolve real expert agents
- production-honest configuration path

### 6. Transport semantics must be honest and durable

If Choreographer is to sit in a critical incident path, transport claims must
match code.

Required:

- either plain NATS documented honestly, or real JetStream semantics
- preferably durable consumption if any bus-driven integration is used
- release-grade tests covering the chosen mode

### 7. TLS posture must be real

If deployed next to Runtime and Kernel in the stack, Choreographer cannot expose
a fake TLS chart surface.

Required:

- server TLS/mTLS implemented for the declared deployment mode
- chart values backed by real code paths

### 8. Stack-level E2E must exist

Before PIR depends on Choreographer in production, there must be a reproducible
stack proof:

```text
PIR bounded escalation
  -> kernel bundle
    -> choreographer deliberation
      -> structured decision
        -> PIR publishes bounded follow-up
```

And another one:

```text
PIR bounded escalation
  -> kernel bundle
    -> choreographer handoff report
      -> final escalation-to-human
```

Without these tests, the integration is architectural prose only.

## Concrete Choreographer changes required

This section maps the minimum changes into Choreographer terms.

### A. New integration surface

Add a specialist-grade RPC or request mode, distinct from the generic trigger
fan-out flow.

Preferred shape:

- `RunCouncilDecision`
- request includes:
  - council id / specialty
  - incident metadata
  - structured bundle
  - output schema id
  - deliberation constraints
- response includes:
  - structured winner
  - validation outcome
  - candidate summaries
  - traces / metadata

This is better than overloading the current generic `TriggerEvent`.

### B. Task metadata model

Current `Task` only includes:

- `specialty`
- `description`
- `constraints`
- `attributes`

That is too weak for PIR integration.

Choreographer must be able to carry, at minimum:

- external incident id
- external incident run id
- source event id
- correlation id
- causation id
- council contract id
- expected output schema id

This can be added either:

- directly to the task/event model, or
- via a typed metadata map with first-class helpers and validation

### C. Contract-aware validators

The current validator model is generic and content-oriented.

For PIR, new validators are required:

- JSON schema validator
- allowed-decision validator
- required-field validator
- event-proposal validator

These should run before a council result is accepted as the winner.

### D. Kernel-fed prompt assembly

Choreographer should not invent prompt assembly ad hoc for PIR.

It needs a repeatable mechanism for:

- bounded context sections
- evidence references
- prior decisions
- prior failed actions
- explicit decision space
- output contract instructions

Whether that prompt assembly happens inside PIR or inside Choreographer is an
implementation choice, but one side must own it explicitly.

### E. Real executor path

Once reevaluation starts emitting remedy events, Choreographer does not
necessarily need to execute them itself, because PIR still owns domain events.

But if any future council uses `Orchestrate`, the runtime executor path must be
real and stack-tested first.

## Concrete PIR changes required

### A. New bounded specialist families

PIR needs explicit new specialist families in its catalog:

- `complex-incident-reevaluation`
- `human-handoff-report`

These should have:

- consumed event types
- consumed subjects
- decision spaces
- governance notes
- output contracts

### B. New event families

PIR needs new bounded event families for:

- reevaluation request
- reevaluation outcome
- handoff-report request
- handoff-report outcome

### C. New orchestrator adapter

PIR needs a Choreographer client adapter, likely gRPC.

It should:

- call Choreographer
- pass bounded incident context
- validate response shape
- classify transport vs deterministic errors

### D. New routing logic

PIR needs deterministic rules for when to enter reevaluation rather than
handing off directly to `human-escalation`.

Examples of triggers:

- more than one specialist family has already contributed findings
- more than one remediation attempt has failed
- the last specialist returned `escalated` with a compatible reason class
- the incident class is explicitly marked as reevaluation-eligible

This must be deterministic and catalog-driven.

## Recommended first slice

The smallest credible first slice is not the full general framework.
It is one concrete path:

- upstream event: `payments.incident.runtime-rollout.escalated`
- new PIR bounded step: `complex-incident-reevaluation`
- council output:
  - either a new `runtime-rollout.requested` with a refined target
  - or `escalate_to_human`
- if `escalate_to_human`, run `human-handoff-report`

Why this slice:

- rollout already has a real runtime path in PIR
- rollout already has investigation + operator semantics
- the graph shape already exists
- the failure mode is easy to understand
- it exercises the exact desired pre-human branch

## Migration plan

### Phase 0. Choreographer completion

Do not start PIR integration until Choreographer closes the current stack gaps.

Mandatory:

1. real runtime executor
2. real kernel context path
3. structured output validation
4. provider-backed agent factory wiring
5. causal metadata propagation
6. honest transport semantics
7. TLS backed by code
8. stack E2E

### Phase 1. PIR-side architecture prep

1. add new PIR event families and specialist catalog entries
2. define reevaluation output contract
3. define handoff-report output contract
4. define deterministic eligibility rules for reevaluation

### Phase 2. Direct gRPC integration

1. add PIR Choreographer client
2. add one council in Choreographer for reevaluation
3. wire one concrete path:
   `runtime-rollout.escalated -> complex-incident-reevaluation`
4. validate bounded follow-up event emission

### Phase 3. Human report integration

1. add handoff-report council
2. write report artifact into kernel or bounded storage
3. change final human escalation path to require report synthesis first

### Phase 4. Generalization

1. add reevaluation support to saturation and payment-integrity paths
2. add more councils only where the domain justifies them
3. keep event families and decision spaces bounded

## Risks

### 1. Turning reevaluation into an unbounded agent loop

This is the biggest risk.

Mitigation:

- closed decision space
- bounded budget
- small councils
- deterministic PIR ownership of follow-up events

### 2. Duplicating domain logic between PIR and Choreographer

If specialist semantics leak into Choreographer prompts, validators, and event
mapping, the same domain contract will exist twice.

Mitigation:

- PIR owns domain event catalog
- PIR owns specialist catalog
- Choreographer receives bounded contracts, not implicit domain knowledge

### 3. Weak traceability across the boundary

If incident ids and causation metadata are not preserved across the PIR →
Choreographer → PIR round trip, the reevaluation layer becomes opaque.

Mitigation:

- make causal metadata first-class before integration starts

### 4. Premature adoption before Choreographer is ready

If PIR depends on a still-incomplete Choreographer, the resulting system will
be less safe than the current direct-human path.

Mitigation:

- treat Choreographer completion as a hard prerequisite, not a parallel nice-to-have

## Decision

Recommended decision:

- do this
- but only after Choreographer reaches stack-complete status
- and do it as a bounded PIR integration, not as PIR replacement

In one sentence:

> Choreographer should become PIR's expert deliberation engine for complex
> pre-human incident reevaluation and human handoff synthesis, while PIR
> remains the domain shell that owns event contracts, kernel semantics,
> runtime governance, and incident lifecycle authority.

## Immediate next steps

1. Finish Choreographer stack-completion work:
   - runtime executor
   - kernel context integration
   - structured outputs
   - causal metadata
   - provider wiring
   - honest transport/TLS
   - stack E2E
2. In PIR, draft the new bounded event families:
   - `complex-incident-reevaluation.*`
   - `handoff-report.*`
3. Choose one first migration path:
   - `runtime-rollout.escalated`
4. Define the reevaluation result schema and handoff-report schema
5. Add a PIR-side gRPC client to Choreographer only after the completion gates above are green
