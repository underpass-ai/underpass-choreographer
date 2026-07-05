# Codex CLI configuration

Codex CLI reads MCP servers from its TOML config (usually
`~/.codex/config.toml` or `~/.config/codex/config.toml`). The
`choreo-mcp` adapter is added once; every Codex session can then call
the 17 `choreo_*` tools.

See the canonical UX reference at
[`docs/operations/mcp-stdio.md`](../mcp-stdio.md) for the tool list,
env-var reference, and TLS posture options.

## Quick add (installed binary)

Install from crates.io:

```bash
cargo install choreo-mcp --locked
```

The dev fallback (in-tree source) lives at
`CHOREO_MCP_INSTALL_MODE=git bash scripts/mcp/install-choreo-mcp.sh`
in the repo.

```bash
codex mcp add underpass-choreographer \
  --env CHOREO_MCP_GRPC_ENDPOINT=https://choreographer.example.com \
  -- choreo-mcp
```

The command writes:

```toml
[mcp_servers.underpass-choreographer]
command = "choreo-mcp"

[mcp_servers.underpass-choreographer.env]
CHOREO_MCP_GRPC_ENDPOINT = "https://choreographer.example.com"
```

## Dev from a checkout

When you want to run against the in-tree build (no install step), use
an absolute manifest path so the config works from any working
directory:

```bash
codex mcp add underpass-choreographer \
  --env CHOREO_MCP_GRPC_ENDPOINT=https://choreographer.example.com \
  -- cargo run -q --manifest-path /path/to/underpass-orchestrator/Cargo.toml -p choreo-mcp --locked
```

Which writes:

```toml
[mcp_servers.underpass-choreographer]
command = "cargo"
args = ["run", "-q", "--manifest-path", "/path/to/underpass-orchestrator/Cargo.toml", "-p", "choreo-mcp", "--locked"]

[mcp_servers.underpass-choreographer.env]
CHOREO_MCP_GRPC_ENDPOINT = "https://choreographer.example.com"
```

## Fixture mode (no choreographer running)

Useful for verifying that Codex picks the tools up at all:

```toml
[mcp_servers.underpass-choreographer]
command = "choreo-mcp"

[mcp_servers.underpass-choreographer.env]
CHOREO_MCP_BACKEND = "fixture"
```

The 17 `choreo_*` tools become callable; every call returns the
deterministic canned response (no network).

## mTLS to a hardened deployment

When the choreographer is behind mTLS (chart's
`tls.mode=mutual`), point Codex at the local cert bundle:

```toml
[mcp_servers.underpass-choreographer]
command = "choreo-mcp"

[mcp_servers.underpass-choreographer.env]
CHOREO_MCP_GRPC_ENDPOINT = "https://choreographer.underpass.svc:50055"
CHOREO_MCP_GRPC_TLS_MODE = "mutual"
CHOREO_MCP_GRPC_TLS_CA_PATH = "/var/run/choreo-tls/ca.crt"
CHOREO_MCP_GRPC_TLS_CERT_PATH = "/var/run/choreo-tls/tls.crt"
CHOREO_MCP_GRPC_TLS_KEY_PATH = "/var/run/choreo-tls/tls.key"
CHOREO_MCP_GRPC_TLS_DOMAIN_NAME = "choreographer-grpc"
```

The same `_TLS_*` envs trigger auto-detection — setting them is
enough; `_TLS_MODE` is a manual override when you want it explicit
for self-documentation.

## Verifying

After updating the config, restart Codex and ask the agent:

> List the choreographer's councils.

Codex should call `choreo_list_councils` and return the live result
(or the fixture's canned list, depending on backend).
