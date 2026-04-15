use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    tracing::info!(
        service = "underpass-choreographer",
        version = env!("CARGO_PKG_VERSION"),
        "starting"
    );

    // TODO: load config, wire adapters, start gRPC server + event consumers.
    Ok(())
}
