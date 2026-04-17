//! HTTP health endpoints for Kubernetes probes and human operators.
//!
//! Two routes, both returning JSON:
//!
//! - `GET /healthz` — liveness. Returns `200 OK` as long as the
//!   async runtime is responsive. Never checks external
//!   dependencies: killing the pod when NATS is down does not fix
//!   anything.
//! - `GET /readyz` — readiness. Checks that every external
//!   dependency the composition root wired is presently reachable
//!   (NATS via `connection_state()` when enabled). Returns `200 OK`
//!   when every check passes, `503 Service Unavailable` with a JSON
//!   body listing the failing components otherwise.
//!
//! The module is transport-only: it builds an [`axum::Router`]
//! given a [`HealthState`]. Composition and lifecycle belong to
//! [`crate::runtime`].

use std::sync::Arc;

use async_nats::connection::State as NatsConnectionState;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use choreo_core::ports::StatisticsPort;
use serde::Serialize;

/// Read-only handles the health endpoints need. Cloning a
/// `HealthState` is cheap — every inner handle is already an `Arc`
/// or a lightweight client.
#[derive(Clone)]
pub struct HealthState {
    /// `Some` when NATS messaging was wired at composition time.
    /// `None` when the service is running with `NoopMessaging`; in
    /// that case readiness never fails on NATS.
    nats: Option<async_nats::Client>,
    statistics: Arc<dyn StatisticsPort>,
    service_version: Arc<str>,
}

impl std::fmt::Debug for HealthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthState")
            .field("nats_wired", &self.nats.is_some())
            .field("service_version", &self.service_version)
            .finish()
    }
}

impl HealthState {
    #[must_use]
    pub fn new(
        nats: Option<async_nats::Client>,
        statistics: Arc<dyn StatisticsPort>,
        service_version: impl Into<String>,
    ) -> Self {
        Self {
            nats,
            statistics,
            service_version: Arc::from(service_version.into().into_boxed_str()),
        }
    }
}

/// Build the health router. Mount it alongside the gRPC server on
/// the HTTP port.
pub fn router(state: HealthState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct LivenessBody {
    status: &'static str,
    service: &'static str,
    version: String,
}

async fn healthz(State(state): State<HealthState>) -> impl IntoResponse {
    // Liveness must never consult external deps.
    Json(LivenessBody {
        status: "alive",
        service: "underpass-choreographer",
        version: state.service_version.as_ref().to_owned(),
    })
}

#[derive(Serialize)]
struct ReadinessBody {
    status: &'static str,
    service: &'static str,
    version: String,
    checks: Vec<CheckResult>,
}

#[derive(Serialize)]
struct CheckResult {
    name: &'static str,
    healthy: bool,
    detail: &'static str,
}

async fn readyz(State(state): State<HealthState>) -> Response {
    let mut checks = Vec::new();

    match &state.nats {
        Some(client) => checks.push(check_nats(client)),
        None => checks.push(CheckResult {
            name: "nats",
            healthy: true,
            detail: "not wired (noop messaging)",
        }),
    }

    let ready = checks.iter().all(|c| c.healthy);
    let body = ReadinessBody {
        status: if ready { "ready" } else { "not-ready" },
        service: "underpass-choreographer",
        version: state.service_version.as_ref().to_owned(),
        checks,
    };
    let code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(body)).into_response()
}

/// Prometheus text format exposition.
///
/// Hand-rolled (no client library) to avoid another dependency for
/// five counters and a gauge. The format is specified at
/// <https://prometheus.io/docs/instrumenting/exposition_formats/>.
/// One metric family per `# HELP` / `# TYPE` pair; samples follow.
async fn metrics(State(state): State<HealthState>) -> Response {
    use std::fmt::Write as _;

    let snap = match state.statistics.snapshot().await {
        Ok(s) => s,
        Err(err) => {
            tracing::error!(error = %err, "metrics snapshot failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "statistics snapshot failed\n",
            )
                .into_response();
        }
    };

    let ready_gauge = match &state.nats {
        Some(client) if client.connection_state() != NatsConnectionState::Connected => 0,
        _ => 1,
    };

    let total_ops = snap
        .total_deliberations()
        .saturating_add(snap.total_orchestrations());
    let total_deliberations = snap.total_deliberations();
    let total_orchestrations = snap.total_orchestrations();
    let total_duration_ms = snap.total_duration().get();

    // `write!` into String is infallible; unwrap is safe here.
    let mut body = String::new();
    body.push_str("# HELP choreo_deliberations_total Count of deliberations completed.\n");
    body.push_str("# TYPE choreo_deliberations_total counter\n");
    writeln!(body, "choreo_deliberations_total {total_deliberations}").unwrap();

    body.push_str("# HELP choreo_orchestrations_total Count of orchestrations completed.\n");
    body.push_str("# TYPE choreo_orchestrations_total counter\n");
    writeln!(body, "choreo_orchestrations_total {total_orchestrations}").unwrap();

    body.push_str(
        "# HELP choreo_deliberations_specialty_total Count of deliberations per specialty.\n",
    );
    body.push_str("# TYPE choreo_deliberations_specialty_total counter\n");
    for (specialty, count) in snap.per_specialty() {
        let label = escape_label_value(specialty.as_str());
        writeln!(
            body,
            "choreo_deliberations_specialty_total{{specialty=\"{label}\"}} {count}"
        )
        .unwrap();
    }

    body.push_str(
        "# HELP choreo_operation_duration_milliseconds Summed duration across deliberations and orchestrations.\n",
    );
    body.push_str("# TYPE choreo_operation_duration_milliseconds summary\n");
    writeln!(
        body,
        "choreo_operation_duration_milliseconds_sum {total_duration_ms}"
    )
    .unwrap();
    writeln!(
        body,
        "choreo_operation_duration_milliseconds_count {total_ops}"
    )
    .unwrap();

    body.push_str("# HELP choreo_service_ready 1 when every wired dependency is reachable.\n");
    body.push_str("# TYPE choreo_service_ready gauge\n");
    writeln!(body, "choreo_service_ready {ready_gauge}").unwrap();

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    )
        .into_response()
}

/// Escape a Prometheus label value per the exposition format:
/// backslash, double-quote, and newline must be escaped.
fn escape_label_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

fn check_nats(client: &async_nats::Client) -> CheckResult {
    match client.connection_state() {
        NatsConnectionState::Connected => CheckResult {
            name: "nats",
            healthy: true,
            detail: "connected",
        },
        NatsConnectionState::Pending => CheckResult {
            name: "nats",
            healthy: false,
            detail: "connection pending",
        },
        NatsConnectionState::Disconnected => CheckResult {
            name: "nats",
            healthy: false,
            detail: "disconnected",
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use choreo_adapters::memory::InMemoryStatistics;
    use choreo_core::value_objects::{DurationMs, Specialty};
    use serde_json::Value;
    use tower::ServiceExt; // for `oneshot`

    fn stats() -> Arc<dyn StatisticsPort> {
        Arc::new(InMemoryStatistics::new())
    }

    async fn body_json(resp: Response) -> Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        serde_json::from_slice(&bytes).expect("valid json")
    }

    async fn body_text(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    #[tokio::test]
    async fn healthz_returns_200_with_alive_status() {
        let app = router(HealthState::new(None, stats(), "0.1.0"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["status"], "alive");
        assert_eq!(body["service"], "underpass-choreographer");
        assert_eq!(body["version"], "0.1.0");
    }

    #[tokio::test]
    async fn readyz_without_nats_returns_200_and_reports_not_wired() {
        let app = router(HealthState::new(None, stats(), "0.1.0"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["status"], "ready");
        let checks = body["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0]["name"], "nats");
        assert_eq!(checks[0]["healthy"], true);
        assert_eq!(checks[0]["detail"], "not wired (noop messaging)");
    }

    #[tokio::test]
    async fn healthz_ignores_nats_state() {
        // Even with a NATS client present, /healthz must not depend
        // on NATS reachability — that is readiness's job. We cannot
        // easily build a real `async_nats::Client` without a server
        // here, so this test proves the route path independence by
        // hitting /healthz with `None` and asserting response shape.
        // The structural invariant is enforced by the implementation
        // of `healthz` never touching `state.nats`.
        let app = router(HealthState::new(None, stats(), "9.9.9"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["version"], "9.9.9");
    }

    #[tokio::test]
    async fn unknown_route_is_404() {
        let app = router(HealthState::new(None, stats(), "0.1.0"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn post_to_healthz_is_method_not_allowed() {
        let app = router(HealthState::new(None, stats(), "0.1.0"));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[test]
    fn health_state_debug_does_not_expose_secrets() {
        // Paranoia: make sure nothing sensitive sneaks into the
        // Debug output. Right now there are no secrets here, but
        // this test locks the invariant as the state grows.
        let state = HealthState::new(None, stats(), "0.1.0");
        let shown = format!("{state:?}");
        assert!(shown.contains("HealthState"));
        assert!(shown.contains("nats_wired"));
        assert!(!shown.to_lowercase().contains("secret"));
        assert!(!shown.to_lowercase().contains("token"));
        assert!(!shown.to_lowercase().contains("api_key"));
    }

    #[tokio::test]
    async fn metrics_emits_prometheus_text_with_zero_counters_on_fresh_boot() {
        let app = router(HealthState::new(None, stats(), "0.1.0"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("text/plain; version=0.0.4")
        );
        let text = body_text(resp).await;
        assert!(text.contains("# TYPE choreo_deliberations_total counter"));
        assert!(text.contains("choreo_deliberations_total 0"));
        assert!(text.contains("choreo_orchestrations_total 0"));
        assert!(text.contains("choreo_operation_duration_milliseconds_sum 0"));
        assert!(text.contains("choreo_operation_duration_milliseconds_count 0"));
        assert!(text.contains("choreo_service_ready 1"));
    }

    #[tokio::test]
    async fn metrics_reflects_recorded_operations_and_specialty_labels() {
        let stats_adapter = Arc::new(InMemoryStatistics::new());
        stats_adapter
            .record_deliberation(
                &Specialty::new("triage").unwrap(),
                DurationMs::from_millis(150),
            )
            .await
            .unwrap();
        stats_adapter
            .record_deliberation(
                &Specialty::new("reviewer").unwrap(),
                DurationMs::from_millis(200),
            )
            .await
            .unwrap();
        stats_adapter
            .record_orchestration(DurationMs::from_millis(400))
            .await
            .unwrap();

        let state = HealthState::new(None, stats_adapter.clone(), "0.1.0");
        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let text = body_text(resp).await;
        assert!(text.contains("choreo_deliberations_total 2"));
        assert!(text.contains("choreo_orchestrations_total 1"));
        assert!(
            text.contains("choreo_deliberations_specialty_total{specialty=\"triage\"} 1"),
            "missing triage label:\n{text}"
        );
        assert!(
            text.contains("choreo_deliberations_specialty_total{specialty=\"reviewer\"} 1"),
            "missing reviewer label:\n{text}"
        );
        assert!(text.contains("choreo_operation_duration_milliseconds_sum 750"));
        assert!(text.contains("choreo_operation_duration_milliseconds_count 3"));
    }

    #[test]
    fn label_value_escaping_handles_backslash_quote_and_newline() {
        assert_eq!(escape_label_value("plain"), "plain");
        assert_eq!(escape_label_value("a\\b"), "a\\\\b");
        assert_eq!(escape_label_value("a\"b"), "a\\\"b");
        assert_eq!(escape_label_value("a\nb"), "a\\nb");
    }
}
