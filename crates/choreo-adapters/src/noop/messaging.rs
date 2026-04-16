//! No-op [`MessagingPort`].
//!
//! Every publish call returns `Ok(())` without any transport activity.
//! Intended for deployments that disable messaging (`nats_enabled=false`)
//! and for tests. Events are logged at `debug` level so they remain
//! observable without requiring a broker.

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::events::{
    DeliberationCompletedEvent, PhaseChangedEvent, TaskCompletedEvent, TaskDispatchedEvent,
    TaskFailedEvent,
};
use choreo_core::ports::MessagingPort;
use tracing::debug;

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMessaging;

impl NoopMessaging {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl MessagingPort for NoopMessaging {
    async fn publish_task_dispatched(
        &self,
        event: &TaskDispatchedEvent,
    ) -> Result<(), DomainError> {
        debug!(
            task_id = event.task_id().as_str(),
            specialty = event.specialty().as_str(),
            "noop messaging: task.dispatched"
        );
        Ok(())
    }

    async fn publish_task_completed(&self, event: &TaskCompletedEvent) -> Result<(), DomainError> {
        debug!(
            task_id = event.task_id().as_str(),
            specialty = event.specialty().as_str(),
            "noop messaging: task.completed"
        );
        Ok(())
    }

    async fn publish_task_failed(&self, event: &TaskFailedEvent) -> Result<(), DomainError> {
        debug!(
            task_id = event.task_id().as_str(),
            specialty = event.specialty().as_str(),
            kind = event.error_kind(),
            "noop messaging: task.failed"
        );
        Ok(())
    }

    async fn publish_deliberation_completed(
        &self,
        event: &DeliberationCompletedEvent,
    ) -> Result<(), DomainError> {
        debug!(
            task_id = event.task_id().as_str(),
            specialty = event.specialty().as_str(),
            winner = event.winner_proposal_id().as_str(),
            "noop messaging: deliberation.completed"
        );
        Ok(())
    }

    async fn publish_phase_changed(&self, event: &PhaseChangedEvent) -> Result<(), DomainError> {
        debug!(
            task_id = event.task_id().as_str(),
            from = event.from_phase(),
            to = event.to_phase(),
            "noop messaging: phase.changed"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::events::EventEnvelope;
    use choreo_core::value_objects::{
        AgentId, DurationMs, EventId, ProposalId, Score, Specialty, TaskId,
    };
    use time::macros::datetime;

    fn env() -> EventEnvelope {
        EventEnvelope::new(
            EventId::new("e").unwrap(),
            datetime!(2026-04-15 12:00:00 UTC),
            "test",
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn publish_task_dispatched_is_ok() {
        let ev = TaskDispatchedEvent::new(
            env(),
            TaskId::new("t").unwrap(),
            Specialty::new("s").unwrap(),
            None,
        );
        NoopMessaging::new()
            .publish_task_dispatched(&ev)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn publish_task_completed_is_ok() {
        let ev = TaskCompletedEvent::new(
            env(),
            TaskId::new("t").unwrap(),
            Specialty::new("s").unwrap(),
            Some(AgentId::new("a").unwrap()),
            DurationMs::from_millis(1),
        );
        NoopMessaging::new()
            .publish_task_completed(&ev)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn publish_task_failed_is_ok() {
        let ev = TaskFailedEvent::new(
            env(),
            TaskId::new("t").unwrap(),
            Specialty::new("s").unwrap(),
            "kind",
            "reason",
        )
        .unwrap();
        NoopMessaging::new().publish_task_failed(&ev).await.unwrap();
    }

    #[tokio::test]
    async fn publish_deliberation_completed_is_ok() {
        let ev = DeliberationCompletedEvent::new(
            env(),
            TaskId::new("t").unwrap(),
            Specialty::new("s").unwrap(),
            ProposalId::new("p").unwrap(),
            Score::new(0.5).unwrap(),
            1,
            DurationMs::from_millis(10),
        );
        NoopMessaging::new()
            .publish_deliberation_completed(&ev)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn publish_phase_changed_is_ok() {
        let ev = PhaseChangedEvent::new(env(), TaskId::new("t").unwrap(), "a", "b").unwrap();
        NoopMessaging::new()
            .publish_phase_changed(&ev)
            .await
            .unwrap();
    }
}
