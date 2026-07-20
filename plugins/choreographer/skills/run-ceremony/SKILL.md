---
name: run-ceremony
description: Run a declarative Choreographer ceremony locally from YAML when the user asks to coordinate or execute a structured ceremony.
---

# Run a Choreographer ceremony

Use the `choreo_run_ceremony` tool exposed by the bundled Choreographer MCP
server.

1. Obtain or construct a valid ceremony YAML definition.
2. Keep the ceremony id stable when the user supplies one; otherwise let the
   engine generate it.
3. Put caller-provided data in the ceremony `context` object.
4. Call `choreo_run_ceremony` with `definition_yaml` and the optional context.
5. Report the final state, completion status, and step results. Surface the
   Mermaid sequence when it materially helps explain the execution.

Do not claim that a ceremony completed if the tool returned `isError: true` or
`completed: false`.

The current embedded plugin supports complete one-shot ceremonies. Do not use
it for a ceremony that must pause for a later human authorization until the
incremental MCP commands are available.
