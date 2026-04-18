use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Keep the guard alive through the process lifetime. Dropping it
    // on shutdown flushes the OTLP exporter (under the `otel`
    // feature) so no in-flight spans are lost.
    let _telemetry = choreo::init_tracing()?;

    tracing::info!(
        service = "underpass-choreographer",
        version = env!("CARGO_PKG_VERSION"),
        "starting"
    );

    let app = choreo::compose().await?;
    choreo::serve(app).await
}
