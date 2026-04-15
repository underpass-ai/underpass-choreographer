//! [`MessagingPort`] — asynchronous message bus used to publish and
//! consume domain events.
//!
//! The port speaks in domain-event terms; adapters map to/from NATS,
//! Kafka, or any other substrate without leaking transport details
//! into the core.

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};

use crate::error::DomainError;
use crate::events::{
    DeliberationCompletedEvent, PhaseChangedEvent, TaskCompletedEvent, TaskDispatchedEvent,
    TaskFailedEvent,
};

/// Marker trait shared by every published domain event so they can be
/// serialized by the adapter. The marker is automatically implemented
/// for all `Serialize + DeserializeOwned + Send + Sync` types — no
/// explicit `impl DomainEvent for …` is required.
pub trait DomainEvent: Serialize + DeserializeOwned + Send + Sync + 'static {}

impl DomainEvent for TaskDispatchedEvent {}
impl DomainEvent for TaskCompletedEvent {}
impl DomainEvent for TaskFailedEvent {}
impl DomainEvent for DeliberationCompletedEvent {}
impl DomainEvent for PhaseChangedEvent {}

/// Handler invoked by the messaging adapter for every message on a
/// subscribed subject. Handlers receive already-deserialized domain
/// events; transport errors never reach them.
#[async_trait]
pub trait SubscriptionHandler<E: DomainEvent>: Send + Sync {
    async fn handle(&self, event: E) -> Result<(), DomainError>;
}

/// Publish / subscribe surface. Intentionally narrow: specific
/// `publish_*` methods per event type enforce that publishing is a
/// first-class, audited action — publishing an arbitrary untyped
/// payload is not possible.
#[async_trait]
pub trait MessagingPort: Send + Sync {
    async fn publish_task_dispatched(&self, event: &TaskDispatchedEvent)
        -> Result<(), DomainError>;
    async fn publish_task_completed(&self, event: &TaskCompletedEvent) -> Result<(), DomainError>;
    async fn publish_task_failed(&self, event: &TaskFailedEvent) -> Result<(), DomainError>;
    async fn publish_deliberation_completed(
        &self,
        event: &DeliberationCompletedEvent,
    ) -> Result<(), DomainError>;
    async fn publish_phase_changed(&self, event: &PhaseChangedEvent) -> Result<(), DomainError>;
}
