//! NATS subscriber for inbound [`TriggerEvent`]s.
//!
//! Wire format: JSON on the configured trigger subject wildcard
//! (default `choreo.trigger.>`), with envelope fields at the top
//! level next to `kind`, `requested_specialties`, etc. The exact
//! shape matches AsyncAPI's allOf composition.

use std::sync::Arc;

use async_nats::Client;
use choreo_app::services::AutoDispatchService;
use choreo_core::error::DomainError;
use choreo_core::events::TriggerEvent;
use futures::StreamExt;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use super::config::NatsSubjects;

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
                handle_message(&dispatch, &message.payload).await;
            }
            info!("nats trigger subscriber stream ended");
        });

        Ok(handle)
    }
}

#[tracing::instrument(name = "nats.trigger.inbound", skip_all)]
async fn handle_message(dispatch: &AutoDispatchService, payload: &[u8]) {
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
