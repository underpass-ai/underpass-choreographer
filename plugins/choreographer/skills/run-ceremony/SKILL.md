---
name: run-ceremony
description: Run a declarative Choreographer ceremony locally from YAML when the user asks to coordinate or execute a structured ceremony.
---

# Run a Choreographer ceremony

Use the tools exposed by the bundled Choreographer MCP server. Choose the
one-shot path only when the ceremony can run to completion without a later
human decision.

## One-shot ceremonies

1. Obtain or construct a valid ceremony YAML definition.
2. Keep the ceremony id stable when the user supplies one; otherwise let the
   engine generate it.
3. Put caller-provided data in the ceremony `context` object.
4. Call `choreo_run_ceremony` with `definition_yaml` and the optional context.
5. Report the final state, completion status, and step results. Surface the
   Mermaid sequence when it materially helps explain the execution.

Do not claim that a ceremony completed if the tool returned `isError: true` or
`completed: false`.

## Incremental ceremonies with human authorization

1. Call `choreo_start_ceremony` with the YAML and initial context. Keep its
   `ceremony_id` for every later call.
2. While `next_step_id` is present, call `choreo_run_ceremony_step` for that
   exact step. Re-read the returned instance after every action.
3. When `waiting_for_human` contains guard names, pause the ceremony and ask
   the user to authorize or reject the concrete decision. Explain what
   transition the approval would enable.
4. Never infer approval from silence, prior instructions, an agent result, or
   the fact that approval seems operationally sensible. Call
   `choreo_approve_ceremony_guard` only after explicit human authorization in
   the current conversation.
5. Call `choreo_apply_ceremony_transition` only when the returned transition
   reports `enabled: true`.
6. Repeat from step 2 until `completed: true`. Use
   `choreo_get_ceremony_instance` whenever state must be refreshed without a
   mutation.

If the user is uncertain or defers approval, call
`choreo_defer_ceremony_guard`. Preserve their own words in `statement`, state
why the decision remains unclear in `reason`, and record concrete
`reconsider_when` conditions. Leave the persistent ceremony paused and report
its `ceremony_id`, current state, and blocking guard. A deferral never satisfies
the guard. Do not convert a refusal into a tool error or silently choose another
transition.

## Dynamic participant interventions

When a participant asks the meeting for an opinion or asks a role to inspect
something, keep the ceremony instance active and use
`choreo_request_ceremony_intervention`:

1. Preserve the participant's own request in `message`. Use `opinion`,
   `investigation`, or `action` for `kind`.
2. Omit `target_role_ids` when addressing the whole table. Supply explicit
   role ids when the participant named a specialist.
3. When the participant selects an option proposed in an earlier intervention,
   include `provenance` with the source intervention, the role whose response
   proposed it, and the selected role. Preserve the participant's specific
   wording; a selection is still not authorization for a consequential action.
4. For a read-only evidence request, call
   `choreo_collect_ceremony_evidence` when the embedded host configured the
   requested `source_id`. Preserve the participant's exact request in `query`
   and put safe structured selectors, such as the service and time window, in
   `details`. The returned non-empty evidence pack is recorded as that role's
   response.
5. Otherwise obtain the actual opinion, evidence, or action result with the
   host's available capabilities, then record each targeted role's contribution
   with `choreo_respond_to_ceremony_intervention`. Never turn an absent source
   or empty result into evidence; report the block and leave the intervention
   open.
6. Leave the intervention open until the requesting participant explicitly
   says they are satisfied or asks to close it. Only then call
   `choreo_close_ceremony_intervention` as that requesting role.

An `action` intervention is not human authorization and never bypasses a
ceremony guard or host permission. Resolve ambiguous operational requests to
the safe read-only meaning: inspect logs, query a database without writes, and
peek at queue metadata without consuming messages. Ask for explicit approval
before any consequential mutation.
