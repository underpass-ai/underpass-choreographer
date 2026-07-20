//! `choreo-mcp` — stdio MCP adapter for the Underpass Choreographer.
//!
//! Reads one JSON-RPC line at a time from stdin, dispatches to the
//! inner [`ChoreoMcpServer`], writes the response to stdout. Stdout
//! is reserved for JSON-RPC responses; logs go to stderr as JSON.
//!
//! See `docs/operations/mcp-stdio.md` for end-user setup.

use std::io::{self, BufRead, Write};

use choreo_mcp::{
    ChoreoMcpServer, GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV,
    GRPC_TLS_DOMAIN_NAME_ENV, GRPC_TLS_KEY_PATH_ENV, GRPC_TLS_MODE_ENV, MCP_BACKEND_ENV,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let server = match ChoreoMcpServer::try_from_env() {
        Ok(server) => server,
        Err(message) => {
            eprintln!("choreo-mcp: {message}");
            eprintln!("choreo-mcp: select a compiled backend with {MCP_BACKEND_ENV}");
            std::process::exit(2);
        }
    };

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    if server.backend_name() == "grpc" {
        eprintln!(
            "choreo-mcp: using live gRPC backend from {GRPC_ENDPOINT_ENV} with {GRPC_TLS_MODE_ENV}={}",
            server.grpc_tls_mode_name()
        );
        if server.grpc_tls_mode_name() != "disabled" {
            eprintln!(
                "choreo-mcp: TLS envs: {GRPC_TLS_CA_PATH_ENV}, {GRPC_TLS_CERT_PATH_ENV}, {GRPC_TLS_KEY_PATH_ENV}, {GRPC_TLS_DOMAIN_NAME_ENV}"
            );
        }
    } else if server.backend_name() == "embedded" {
        eprintln!("choreo-mcp: using embedded in-process ceremony backend");
    } else {
        eprintln!("choreo-mcp: using explicit fixture backend");
    }

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = server.handle_json_line(&line).await {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }

    Ok(())
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("choreo_mcp=info"));
    tracing_subscriber::fmt()
        .json()
        .with_writer(io::stderr)
        .with_env_filter(filter)
        .init();
}
