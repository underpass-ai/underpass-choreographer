//! NATS subscriber for inbound [`TriggerEvent`]s.
//!
//! Wire format: JSON on the configured trigger subject wildcard
//! (default `choreo.trigger.>`), with envelope fields at the top
//! level next to `kind`, `requested_specialties`, etc. The exact
//! shape matches AsyncAPI's allOf composition.

use std::sync::Arc;

use async_nats::{header::HeaderMap, Client};
use choreo_app::services::AutoDispatchService;
use choreo_core::error::DomainError;
use choreo_core::events::TriggerEvent;
use choreo_core::value_objects::TraceContext;
use futures::StreamExt;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use super::config::NatsSubjects;
use super::messaging::TRACEPARENT_HEADER;

/// Spawns a background task that consumes trigger events from NATS
/// and forwards them to the application's `AutoDispatchService`.
#[derive(Clone)]
pub struct NatsTriggerSubscriber {
    client: Client,
    subjects: NatsSubjects,
    dispatch: Arc<AutoDispatchService>,
}

impl std::fmt::Debug for NatsTriggerSubscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NatsTriggerSubscriber")
            .field("subjects", &self.subjects)
            .finish()
    }
}

impl NatsTriggerSubscriber {
    #[must_use]
    pub fn new(client: Client, subjects: NatsSubjects, dispatch: Arc<AutoDispatchService>) -> Self {
        Self {
            client,
            subjects,
            dispatch,
        }
    }

    /// Start the subscription loop in a Tokio task.
    ///
    /// Returns the task handle so the composition root can await it
    /// on shutdown. The task runs until the subscription stream ends
    /// (e.g. on connection close).
    pub async fn spawn(self) -> Result<JoinHandle<()>, DomainError> {
        let mut subscription = self
            .client
            .subscribe(self.subjects.trigger.clone())
            .await
            .map_err(|err| {
                error!(
                    error = %err,
                    subject = self.subjects.trigger.as_str(),
                    "nats subscribe failed"
                );
                DomainError::InvariantViolated {
                    reason: "nats: subscribe failed",
                }
            })?;

        info!(
            subject = self.subjects.trigger.as_str(),
            "nats trigger subscriber started"
        );

        let dispatch = self.dispatch.clone();

        let handle = tokio::spawn(async move {
            while let Some(message) = subscription.next().await {
                let trace = extract_traceparent(message.headers.as_ref());
                handle_message(&dispatch, &message.payload, trace).await;
            }
            info!("nats trigger subscriber stream ended");
        });

        Ok(handle)
    }
}

/// Best-effort W3C Trace Context extraction from NATS headers.
///
/// Invalid headers are logged and discarded — we never reject a
/// message on malformed tracecontext alone; downstream processing
/// continues with `None`.
fn extract_traceparent(headers: Option<&HeaderMap>) -> Option<TraceContext> {
    let value = headers?.get(TRACEPARENT_HEADER)?.as_str();
    match TraceContext::parse(value) {
        Ok(ctx) => Some(ctx),
        Err(err) => {
            warn!(error = %err, header = value, "nats trigger: invalid traceparent header");
            None
        }
    }
}

/// `trace_id` and `span_id` land on the span as fields when the
/// inbound message carried a valid `traceparent`. Downstream log
/// scrapers and OTel-aware collectors can correlate on those names.
#[tracing::instrument(
    name = "nats.trigger.inbound",
    skip_all,
    fields(
        trace_id = trace.as_ref().map_or("", TraceContext::trace_id),
        span_id = trace.as_ref().map_or("", TraceContext::span_id),
    )
)]
async fn handle_message(
    dispatch: &AutoDispatchService,
    payload: &[u8],
    trace: Option<TraceContext>,
) {
    let _ = trace; // value is recorded as span field by the attribute above.
    let trigger: TriggerEvent = match serde_json::from_slice(payload) {
        Ok(t) => t,
        Err(err) => {
            warn!(error = %err, "nats trigger: malformed payload, dropping");
            return;
        }
    };
    match dispatch.dispatch(&trigger).await {
        Ok(outcome) => {
            info!(
                event_id = trigger.envelope().event_id().as_str(),
                successes = outcome.successes.len(),
                failures = outcome.failures.len(),
                "nats trigger dispatched"
            );
        }
        Err(err) => {
            error!(
                event_id = trigger.envelope().event_id().as_str(),
                error = %err,
                "nats trigger auto-dispatch failed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_map_with_traceparent(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(TRACEPARENT_HEADER, value);
        headers
    }

    #[test]
    fn extract_traceparent_returns_none_when_headers_absent() {
        assert!(extract_traceparent(None).is_none());
    }

    #[test]
    fn extract_traceparent_returns_none_when_header_missing() {
        let headers = HeaderMap::new();
        assert!(extract_traceparent(Some(&headers)).is_none());
    }

    #[test]
    fn extract_traceparent_parses_valid_header() {
        let headers =
            header_map_with_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01");
        let ctx = extract_traceparent(Some(&headers)).expect("valid traceparent");
        assert_eq!(ctx.trace_id(), "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(ctx.span_id(), "b7ad6b7169203331");
    }

    #[test]
    fn extract_traceparent_drops_invalid_header_instead_of_failing() {
        // Honest: malformed upstream tracecontext must never drop
        // the domain message. The header is ignored and the handler
        // keeps running with trace = None.
        let headers = header_map_with_traceparent("not-a-traceparent");
        assert!(extract_traceparent(Some(&headers)).is_none());
    }
}
