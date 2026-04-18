//! Telemetry pipeline setup.
//!
//! Two build-time variants, selected by the `otel` Cargo feature:
//!
//! - **Without `otel`** (default): a plain JSON-format subscriber
//!   goes to stdout. Spans are observable in-process only.
//!
//! - **With `otel`** + `CHOREO_OTLP_ENDPOINT` set: the same JSON
//!   subscriber is layered together with a `tracing-opentelemetry`
//!   bridge that exports spans over OTLP (gRPC transport) to the
//!   configured collector. Spans acquire real OTel trace/span IDs
//!   and cross-service correlation becomes available.
//!
//! - **With `otel`** but `CHOREO_OTLP_ENDPOINT` unset: the binary
//!   behaves exactly as without the feature — JSON only. No silent
//!   background exporter, no wasted connections.
//!
//! The returned [`TelemetryGuard`] owns the tracer provider's
//! lifetime so `main` can drop it at shutdown and flush any
//! buffered spans.

use anyhow::Result;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// RAII handle returned by [`init_tracing`]. Drop on shutdown so the
/// OTel exporter flushes buffered spans before the process exits.
#[must_use]
pub struct TelemetryGuard {
    #[cfg(feature = "otel")]
    provider: Option<opentelemetry_sdk::trace::TracerProvider>,
}

impl TelemetryGuard {
    #[cfg(not(feature = "otel"))]
    fn noop() -> Self {
        Self {}
    }

    #[cfg(feature = "otel")]
    fn noop() -> Self {
        Self { provider: None }
    }
}

impl std::fmt::Debug for TelemetryGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelemetryGuard").finish()
    }
}

#[cfg(feature = "otel")]
impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            // `shutdown` drains in-flight spans and closes the
            // exporter. Any error here is purely at-shutdown and
            // gets logged via the global tracer hook.
            let _ = provider.shutdown();
        }
    }
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

#[cfg(not(feature = "otel"))]
pub fn init_tracing() -> Result<TelemetryGuard> {
    tracing_subscriber::registry()
        .with(env_filter())
        .with(fmt::layer().json())
        .init();
    Ok(TelemetryGuard::noop())
}

#[cfg(feature = "otel")]
pub fn init_tracing() -> Result<TelemetryGuard> {
    use opentelemetry::global;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig as _;
    use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::TracerProvider, Resource};
    use opentelemetry_semantic_conventions as sc;

    // Every binary build registers the W3C propagator so any
    // interceptor (gRPC, NATS) can extract/inject `traceparent`
    // even when no exporter is configured. Propagation is cheap.
    global::set_text_map_propagator(TraceContextPropagator::new());

    let filter = env_filter();
    let fmt_layer = fmt::layer().json();

    let Some(endpoint) = std::env::var("CHOREO_OTLP_ENDPOINT")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
        return Ok(TelemetryGuard::noop());
    };

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .build()?;

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(Resource::new(vec![
            opentelemetry::KeyValue::new(sc::resource::SERVICE_NAME, "underpass-choreographer"),
            opentelemetry::KeyValue::new(sc::resource::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
        ]))
        .build();

    global::set_tracer_provider(provider.clone());
    let otel_layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("choreographer"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    tracing::info!(endpoint, "otlp exporter wired");
    Ok(TelemetryGuard {
        provider: Some(provider),
    })
}
