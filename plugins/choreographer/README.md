# Choreographer Codex plugin

This bundle runs the Choreographer ceremony engine as a local MCP stdio
process. It does not require a Choreographer service, gRPC, NATS, or a
database.

The repository packaging script places the isolated embedded binary at
`bin/choreo-mcp`. Codex starts it through `scripts/run-embedded-mcp.sh`.

Executable scope:

- `choreo_run_ceremony` for one-shot terminal execution;
- `choreo_start_ceremony`, `choreo_run_ceremony_step`,
  `choreo_approve_ceremony_guard`, `choreo_defer_ceremony_guard`,
  `choreo_apply_ceremony_transition`, and
  `choreo_get_ceremony_instance` for persistent, human-authorized flows;
- `choreo_list_ceremony_instances` to rediscover resumable meetings known to
  the active backend;
- `choreo_request_ceremony_intervention`,
  `choreo_respond_to_ceremony_intervention`,
  `choreo_collect_ceremony_evidence`, and
  `choreo_close_ceremony_intervention` for participant-created live agenda
  items controlled by the requesting role.

The bundled zero-infrastructure process keeps its repositories in memory.
`choreo_list_ceremony_instances` can recover host-side conversation loss while
that process remains alive. Surviving a process restart requires a host to wire
durable instance, definition, and context repositories.
