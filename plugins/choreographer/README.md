# Choreographer Codex plugin

This bundle runs the Choreographer ceremony engine as a local MCP stdio
process. It does not require a Choreographer service, gRPC, NATS, or a
database.

The repository packaging script places the isolated embedded binary at
`bin/choreo-mcp`. Codex starts it through `scripts/run-embedded-mcp.sh`.

Current executable scope: `choreo_run_ceremony`.
