//! NATS [`MessagingPort`] implementation.
//!
//! Publishes every outbound domain event as a JSON message on its
//! canonical subject. The payload is whatever `serde_json` produces
//! for the event type; thanks to `#[serde(flatten)]` on the envelope
//! field the shape matches the AsyncAPI contract (envelope at the
//! root next to event-specific fields).

use async_nats::Client;
use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::events::{
    DeliberationCompletedEvent, PhaseChangedEvent, TaskCompletedEvent, TaskDispatchedEvent,
    TaskFailedEvent,
};
use choreo_core::ports::MessagingPort;
use serde::Serialize;
use tracing::debug;

use super::config::NatsSubjects;

/// Publishes domain events to NATS.
///
/// Constructed from an already-connected [`Client`]; the composition
/// root owns connection lifecycle so the adapter stays focused.
#[derive(Clone)]
pub struct NatsMessaging {
    client: Client,
    subjects: NatsSubjects,
}

impl std::fmt::Debug for NatsMessaging {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NatsMessaging")
            .field("subjects", &self.subjects)
            .finish()
    }
}

impl NatsMessaging {
    #[must_use]
    pub fn new(client: Client, subjects: NatsSubjects) -> Self {
        Self { client, subjects }
    }

    async fn publish_event<E: Serialize>(
        &self,
        subject: &str,
        event: &E,
    ) -> Result<(), DomainError> {
        let payload = serde_json::to_vec(event).map_err(|err| {
            debug!(error = %err, "nats payload encoding failed");
            DomainError::InvariantViolated {
                reason: "nats: failed to serialize outbound event",
            }
        })?;
        self.client
            .publish(subject.to_owned(), payload.into())
            .await
            .map_err(|err| {
                debug!(error = %err, subject, "nats publish failed");
                DomainError::InvariantViolated {
                    reason: "nats: publish failed",
                }
            })?;
        debug!(subject, "nats event published");
        Ok(())
    }
}

#[async_trait]
impl MessagingPort for NatsMessaging {
    async fn publish_task_dispatched(
        &self,
        event: &TaskDispatchedEvent,
    ) -> Result<(), DomainError> {
        self.publish_event(&self.subjects.task_dispatched, event)
            .await
    }

    async fn publish_task_completed(&self, event: &TaskCompletedEvent) -> Result<(), DomainError> {
        self.publish_event(&self.subjects.task_completed, event)
            .await
    }

    async fn publish_task_failed(&self, event: &TaskFailedEvent) -> Result<(), DomainError> {
        self.publish_event(&self.subjects.task_failed, event).await
    }

    async fn publish_deliberation_completed(
        &self,
        event: &DeliberationCompletedEvent,
    ) -> Result<(), DomainError> {
        self.publish_event(&self.subjects.deliberation_completed, event)
            .await
    }

    async fn publish_phase_changed(&self, event: &PhaseChangedEvent) -> Result<(), DomainError> {
        self.publish_event(&self.subjects.phase_changed, event)
            .await
    }
}
