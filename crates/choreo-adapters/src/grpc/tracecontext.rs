//! W3C Trace Context extraction from incoming gRPC metadata.
//!
//! Every RPC handler calls [`link_span_to_metadata`] at the top of
//! its body. When the caller includes a `traceparent` (and optional
//! `tracestate`) header, the current `tracing` span becomes a child
//! of that remote span — so in whichever OTel-aware collector the
//! deployment points at, the choreographer's work threads together
//! with the caller's trace.
//!
//! The helper is a no-op when the `otel` feature is disabled so
//! binaries built without OTel keep zero runtime cost. When the
//! feature is on but the caller did not send tracecontext, the
//! current span becomes a fresh root — that is honest default
//! behaviour for self-originated work.

#[cfg(feature = "otel")]
pub use enabled::link_span_to_metadata;

#[cfg(not(feature = "otel"))]
pub fn link_span_to_metadata<T>(_request: &tonic::Request<T>) {}

#[cfg(feature = "otel")]
mod enabled {
    use opentelemetry::propagation::Extractor;
    use tonic::metadata::MetadataMap;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    /// Extract W3C tracecontext from `request.metadata()` and set it
    /// as the parent of the currently-active `tracing` span.
    pub fn link_span_to_metadata<T>(request: &tonic::Request<T>) {
        let carrier = MetadataExtractor(request.metadata());
        let parent_ctx = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&carrier)
        });
        tracing::Span::current().set_parent(parent_ctx);
    }

    /// Adapter from tonic's `MetadataMap` to OTel's `Extractor`
    /// trait (a minimal TextMap carrier API: `get` + `keys`).
    struct MetadataExtractor<'a>(&'a MetadataMap);

    impl Extractor for MetadataExtractor<'_> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).and_then(|v| v.to_str().ok())
        }

        fn keys(&self) -> Vec<&str> {
            self.0
                .keys()
                .filter_map(|k| match k {
                    tonic::metadata::KeyRef::Ascii(name) => Some(name.as_str()),
                    tonic::metadata::KeyRef::Binary(_) => None,
                })
                .collect()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tonic::metadata::MetadataValue;

        #[test]
        fn extractor_finds_known_header() {
            let mut md = MetadataMap::new();
            md.insert(
                "traceparent",
                MetadataValue::from_static(
                    "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
                ),
            );
            let ext = MetadataExtractor(&md);
            assert!(ext.get("traceparent").is_some());
            assert!(ext.get("nope").is_none());
        }

        #[test]
        fn extractor_skips_binary_headers_from_keys() {
            let mut md = MetadataMap::new();
            md.insert("x-plain", MetadataValue::from_static("ok"));
            md.insert_bin(
                "x-bin-bin",
                tonic::metadata::MetadataValue::from_bytes(&[1, 2, 3]),
            );
            let ext = MetadataExtractor(&md);
            let keys = ext.keys();
            assert!(keys.contains(&"x-plain"));
            assert!(!keys.iter().any(|k| k.ends_with("-bin")));
        }

        // Deeper integration test (span ↔ OTel parent correlation)
        // lives in `crates/choreo-adapters/tests/grpc_tracecontext.rs`
        // because it needs a process-global `tracing-opentelemetry`
        // bridge and would race with unit tests' thread-locals.
    }
}
