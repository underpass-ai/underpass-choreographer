# Ceremony authoring runbook — building multi-agent meetings that actually work

*Status: every mechanic below verified end-to-end on 2026-07-03 against a live
installation (chart + `RunCeremony`), including a 7-participant, 31-LLM-call
ceremony with two 2-agent councils running three adversarial peer-review
rounds each.*

The architecture docs explain *how the engine deliberates*
([choreographer-architecture-and-differentiation.md](../choreographer-architecture-and-differentiation.md));
this runbook is for the operator **writing a ceremony YAML**: the schema keys
that matter, the mechanics they trigger, the silent no-ops, how to size
timeouts, and how to verify what actually happened.

## 1. The step config keys that drive everything

```yaml
steps:
  - id: technical_review
    state: TECHNICAL_REVIEW
    handler: appeal_engineers        # = the agent SPECIALTY (registry key)
    config:
      agent_kind: vllm               # noop | vllm (feature/agent_kinds-gated)
      provider.model: "role:coding"  # exact served-model name OR role alias
      provider.max_tokens: 640
      provider.timeout_secs: 240     # per LLM CALL, not per step
      see_prior: true                # prior steps' outputs enter the prompt
      rounds: 3                      # adversarial peer-review rounds
      num_agents: 2                  # council size
      prompt: "..."
      output_contract:               # optional deterministic policy gate (#118)
        contract_id: my-contract     # unknown keys inside this block are rejected
        format: json_object
        required_fields: [claims, decision]
        allowed_values:
          decision: [accept, reject, request_changes, request_more_evidence]
        json_schema: { ... }         # optional embedded JSON Schema
        evidence:                    # optional grounding rule (#119): every claim
          claims_field: claims       # object must cite refs that exist in the
          refs_field: evidence_refs  # allowed set...
          allowed_refs_from_context: evidence_pack   # ...resolved per-run from
                                     # the RunCeremony context (or a static
                                     # allowed_refs list)
```

With an `output_contract` present, proposals that fail the gate are rejected
deterministically (`NoValidProposal{contract_id}` fails the deliberation);
with an `evidence` block, the `claims-evidence-grounded` validator enforces
that no unsupported claim reaches the step's winning contribution.

Facts that are easy to get wrong:

- **Steps in the same state run in YAML declaration order.** The step `id`
  is an identity and lookup key; it does not determine execution priority.
  Put dependent steps later in the list and use `see_prior: true` when they
  must receive earlier contributions.
- **`handler` is the specialty.** Agents and councils persist in the registry
  **by specialty id** for the lifetime of the pod (in-memory persistence).
  Re-running a ceremony with the same handler but a different `agent_kind`
  reuses the OLD agents — restart the pod or pick a fresh handler name.
- **`provider.*` lives in the step config**, not at ceremony level. Each step
  can point at a different model, size and timeout.
- **`see_prior: true` is your reply mechanism across steps** — later steps
  receive earlier outputs as context. Replies *within* a step are `rounds`.

## 2. `rounds` — real replies, with a silent no-op

`rounds: N` runs the engine's adversarial peer review: **for each round, each
agent critiques its neighbour's proposal ((i+1) mod N, deterministic circular
rotation) and the neighbour's proposal is replaced by the revision.**

- **`rounds` with `num_agents: 1` is a silent no-op** (`deliberate.rs`:
  `if rounds == 0 || agents.len() < 2 { return }`). No error, no warning —
  the step just runs a single draft. If you want back-and-forth, you need a
  council of at least 2.
- LLM call count per step: `num_agents` drafts + `rounds × num_agents × 2`
  (each review = one critique call + one revise call). A 2-agent, 3-round
  council = **14 sequential calls**. Budget your step timeout accordingly:
  `step timeout ≥ calls × p95-per-call-latency`, and remember
  `provider.timeout_secs` bounds each call, not the step.
- Every review emits a `"peer critique and revision"` log/span event with
  `round` and `reviewer` — so "who spoke how many times" is a Loki/Tempo
  query, not archaeology.

## 3. Role aliases mix models *per call*

If the provider behind `CHOREO_VLLM_ENDPOINT` is a routing gateway that
resolves aliases (e.g. `role:coding` → round-robin over the models carrying
that role), then **every individual call — draft, critique, revision —
resolves independently**. In a 3-round council the model that critiques a
proposal is routinely not the model that drafted it. This is a feature (true
model heterogeneity inside one council) and a caveat (the choreographer's
logs record the agent id, not which physical model served each call — if you
need that attribution, log the resolved model in your gateway).

Exact served-model names (`provider.model: "phi4-mini-aws"`) pin a step to
one model; aliases (`role:general`) opt into the mix.

## 4. What multi-round councils actually do (verified, and humbling)

From the verified 31-call run — two councils asked *"why did your previous
investigation fail?"* with 3 peer-review rounds each:

- **Rounds amplify fluency, not grounding.** Without a validator that checks
  claims against the case evidence, each critique round acted as "elaborate
  further": by round 3 both councils had drifted into impressive,
  well-structured, largely fabricated process machinery, far from the
  question.
- **Narrow roles beat big models.** A 3B model given a terse
  "mark each claim GROUNDED or SPECULATION" task produced the most
  evidence-faithful output of the ceremony, outperforming 24-30B models
  deliberating freely.
- Practical guidance: pair every free-deliberating council with (a) a
  grounding step whose only job is separating evidence from speculation, and
  (b) a judge prompt that must cite which claims it relied on. Keep council
  prompts adversarial about *grounding* ("attack claims not supported by the
  stated facts"), not just adversarial in tone.

## 5. Sizing and invocation checklist

1. **Lint before running**: `CeremonyDefinitionYaml::parse_path` (a five-line
   test in `crates/choreo-adapters/tests/`) catches schema errors in
   milliseconds instead of after a cluster round-trip.
2. **Count your calls**: sum over steps of
   `num_agents + rounds × num_agents × 2`. Multiply by expected per-call
   latency; set `timeouts.step_default` above the slowest step, and your
   gRPC client's timeout above the whole sum (`RunCeremony` blocks until the
   terminal state).
3. **Thinking models**: budget `provider.max_tokens` for reasoning + answer
   (a thinking model can spend its entire budget inside `<think>` and return
   an empty answer).
4. **Guards**: `step_status:<step_id>:COMPLETED` chained through the FSM
   transitions is the standard linear-meeting pattern.
5. **Verify what happened, not what you asked for**: count
   `proposal drafted` and `peer critique and revision` events per agent
   (Loki), and read the deliberation spans (Tempo) — see the
   [observability runbook](./observability-runbook.md). If `rounds` was
   silently no-opped (§2), this is where you notice.

## 6. Dynamic participant interventions

An embedded incremental ceremony can accept new agenda items after it starts;
the YAML remains the stable frame while the running `CeremonyInstance` owns
the ordered conversation. Declare capabilities on roles rather than inventing
placeholder steps for every possible question:

```yaml
roles:
  - id: ENGINEER
    allowed_actions:
      - request_intervention
  - id: OBSERVER
    allowed_actions:
      - respond_to_intervention
  - id: DATABASE_SPECIALIST
    allowed_actions:
      - respond_to_intervention
  - id: QUEUE_SPECIALIST
    allowed_actions:
      - respond_to_intervention
```

The host opens an `opinion`, `investigation`, or `action` with
`choreo_request_ceremony_intervention`. With no `target_role_ids`, every role
with response capability may answer; a non-empty list scopes the request.
Each targeted role can answer once, and only the requesting role can close the
intervention. Relevant requests and accumulated responses are passed into
later deliberating step handlers as live participant language, so the meeting
can react without rewriting its definition.

Treat `action` as coordination, not authority. “Look at the queue” should be
implemented as read-only observation or peek, never consuming messages; any
external mutation still requires the host's permissions and the ceremony's
explicit human guards.
