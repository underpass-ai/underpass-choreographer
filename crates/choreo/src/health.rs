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
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;

/// Read-only handles the health endpoints need. Cloning a
/// `HealthState` is cheap — every inner handle is already an `Arc`
/// or a lightweight client.
#[derive(Clone, Default)]
pub struct HealthState {
    /// `Some` when NATS messaging was wired at composition time.
    /// `None` when the service is running with `NoopMessaging`; in
    /// that case readiness never fails on NATS.
    nats: Option<async_nats::Client>,
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
    pub fn new(nats: Option<async_nats::Client>, service_version: impl Into<String>) -> Self {
        Self {
            nats,
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
    use serde_json::Value;
    use tower::ServiceExt; // for `oneshot`

    async fn body_json(resp: Response) -> Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        serde_json::from_slice(&bytes).expect("valid json")
    }

    #[tokio::test]
    async fn healthz_returns_200_with_alive_status() {
        let app = router(HealthState::new(None, "0.1.0"));
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
        let app = router(HealthState::new(None, "0.1.0"));
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
        let app = router(HealthState::new(None, "9.9.9"));
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
        let app = router(HealthState::new(None, "0.1.0"));
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
        let app = router(HealthState::new(None, "0.1.0"));
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
        let state = HealthState::new(None, "0.1.0");
        let shown = format!("{state:?}");
        assert!(shown.contains("HealthState"));
        assert!(shown.contains("nats_wired"));
        assert!(!shown.to_lowercase().contains("secret"));
        assert!(!shown.to_lowercase().contains("token"));
        assert!(!shown.to_lowercase().contains("api_key"));
    }
}
