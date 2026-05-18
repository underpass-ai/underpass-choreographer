# Claude Desktop configuration

Claude Desktop reads MCP servers from `claude_desktop_config.json`.
Path varies by OS:

| OS      | Path                                                                          |
|---------|-------------------------------------------------------------------------------|
| macOS   | `~/Library/Application Support/Claude/claude_desktop_config.json`             |
| Linux   | `~/.config/Claude/claude_desktop_config.json`                                 |
| Windows | `%APPDATA%\Claude\claude_desktop_config.json`                                 |

See the canonical UX reference at
[`docs/operations/mcp-stdio.md`](../mcp-stdio.md) for the tool list,
env-var reference, and TLS posture options.

## Installed binary

Install from crates.io:

```bash
cargo install choreo-mcp --locked
```

The dev fallback (in-tree source) lives at
`CHOREO_MCP_INSTALL_MODE=git bash scripts/mcp/install-choreo-mcp.sh`
in the repo.

```json
{
  "mcpServers": {
    "underpass-choreographer": {
      "command": "choreo-mcp",
      "env": {
        "CHOREO_MCP_GRPC_ENDPOINT": "https://choreographer.example.com"
      }
    }
  }
}
```

The binary must be on Claude's `PATH`. If `cargo install` placed it
under `~/.cargo/bin` and that directory is not in Claude's `PATH`,
use an absolute path:

```json
"command": "/home/<you>/.cargo/bin/choreo-mcp"
```

## Dev from a checkout

```json
{
  "mcpServers": {
    "underpass-choreographer": {
      "command": "cargo",
      "args": [
        "run", "-q",
        "--manifest-path", "/path/to/underpass-orchestrator/Cargo.toml",
        "-p", "choreo-mcp",
        "--locked"
      ],
      "env": {
        "CHOREO_MCP_GRPC_ENDPOINT": "https://choreographer.example.com"
      }
    }
  }
}
```

## Fixture mode (no choreographer running)

```json
{
  "mcpServers": {
    "underpass-choreographer": {
      "command": "choreo-mcp",
      "env": {
        "CHOREO_MCP_BACKEND": "fixture"
      }
    }
  }
}
```

Every `choreo_*` tool becomes callable and returns its deterministic
canned response.

## mTLS to a hardened deployment

```json
{
  "mcpServers": {
    "underpass-choreographer": {
      "command": "choreo-mcp",
      "env": {
        "CHOREO_MCP_GRPC_ENDPOINT": "https://choreographer.underpass.svc:50055",
        "CHOREO_MCP_GRPC_TLS_MODE": "mutual",
        "CHOREO_MCP_GRPC_TLS_CA_PATH": "/var/run/choreo-tls/ca.crt",
        "CHOREO_MCP_GRPC_TLS_CERT_PATH": "/var/run/choreo-tls/tls.crt",
        "CHOREO_MCP_GRPC_TLS_KEY_PATH": "/var/run/choreo-tls/tls.key",
        "CHOREO_MCP_GRPC_TLS_DOMAIN_NAME": "choreographer-grpc"
      }
    }
  }
}
```

## Verifying

After saving the config, restart Claude Desktop completely (the
config is read on launch). In a new conversation:

> List the choreographer's councils.

Claude should call `choreo_list_councils` and show the result.

If the tools do not appear:

- check Claude Desktop's "Developer" panel for stderr from the
  spawned `choreo-mcp` process — the adapter writes JSON tracing
  to stderr on launch (`backend`, `grpc_tls`);
- verify `choreo-mcp --version` runs from a terminal with the
  same `PATH` Claude inherits;
- rule out config-parse errors by validating the JSON file.
