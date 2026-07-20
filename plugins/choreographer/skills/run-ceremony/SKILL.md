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

If the user rejects or defers approval, leave the persistent ceremony paused
and report its `ceremony_id`, current state, and blocking guard. Do not convert
a refusal into a tool error or silently choose another transition.
