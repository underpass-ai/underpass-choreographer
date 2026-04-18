//! Integration test: `link_span_to_metadata` in `grpc::tracecontext`
//! actually sets the OTel parent context on the current tracing
//! span when a W3C `traceparent` header rides on the request.
//!
//! Owns its own integration-test binary so the
//! `tracing-opentelemetry` bridge + global propagator install
//! don't race with unit tests' thread-local subscribers.

#![cfg(feature = "otel")]

use std::sync::OnceLock;

use opentelemetry::trace::{TraceContextExt as _, TracerProvider as _};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::TracerProvider;
use tonic::metadata::MetadataValue;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};

fn install_bridge() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
        let provider = TracerProvider::builder().build();
        opentelemetry::global::set_tracer_provider(provider.clone());
        let otel_layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("test"));
        tracing_subscriber::registry().with(otel_layer).init();
    });
}

#[tokio::test]
async fn link_span_to_metadata_adopts_incoming_traceparent_as_parent() {
    install_bridge();

    let mut request = tonic::Request::new(());
    request.metadata_mut().insert(
        "traceparent",
        MetadataValue::from_static("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"),
    );

    let span = tracing::info_span!("rpc.test.link");
    let _enter = span.enter();

    // Call through the public surface. `choreo_adapters::grpc` is
    // private; we reach in via the adapter module's path — this
    // test compiles under the crate's own test harness so access is
    // unrestricted.
    choreo_adapters::__test_only::link_span_to_metadata(&request);

    let span_ctx = tracing::Span::current()
        .context()
        .span()
        .span_context()
        .clone();
    assert_eq!(
        format!("{:032x}", span_ctx.trace_id()),
        "0af7651916cd43dd8448eb211c80319c",
        "current span's OTel context must carry the propagated trace id"
    );
}

#[tokio::test]
async fn link_span_to_metadata_without_header_gets_a_fresh_trace_id() {
    install_bridge();

    let request = tonic::Request::new(());
    let span = tracing::info_span!("rpc.test.noop");
    let _enter = span.enter();

    choreo_adapters::__test_only::link_span_to_metadata(&request);

    // With no incoming traceparent, the bridge still assigns a
    // valid self-generated trace id to the current tracing span.
    // Assert only that the trace id is NOT the well-known remote
    // value from the adoption test — the span is not falsely
    // associated with the propagated trace.
    let span_ctx = tracing::Span::current()
        .context()
        .span()
        .span_context()
        .clone();
    assert_ne!(
        format!("{:032x}", span_ctx.trace_id()),
        "0af7651916cd43dd8448eb211c80319c",
        "a missing traceparent must not pick up a remote trace id"
    );
}
