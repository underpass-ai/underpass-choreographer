//! [`OrchestrateUseCase`] — deliberate and then hand the winning
//! proposal to the configured [`ExecutorPort`].
//!
//! Emits a `TaskDispatchedEvent` when dispatching to the executor and
//! either a `TaskCompletedEvent` or `TaskFailedEvent` at the end, so
//! downstream systems can observe the full lifecycle.

use std::sync::Arc;

use choreo_core::entities::{Deliberation, Proposal, Task};
use choreo_core::error::DomainError;
use choreo_core::events::{
    EventEnvelope, TaskCompletedEvent, TaskDispatchedEvent, TaskFailedEvent,
};
use choreo_core::ports::{ClockPort, ExecutionOutcome, ExecutorPort, MessagingPort};
use choreo_core::value_objects::{Attributes, EventId};
use time::OffsetDateTime;
use tracing::{error, info};
use uuid::Uuid;

use super::deliberate::DeliberateUseCase;

#[derive(Debug, Clone)]
pub struct OrchestrateOutput {
    pub deliberation: Deliberation,
    pub winner: Proposal,
    pub execution: ExecutionOutcome,
}

pub struct OrchestrateUseCase {
    deliberate: Arc<DeliberateUseCase>,
    executor: Arc<dyn ExecutorPort>,
    messaging: Arc<dyn MessagingPort>,
    clock: Arc<dyn ClockPort>,
    source: String,
}

impl std::fmt::Debug for OrchestrateUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestrateUseCase")
            .field("source", &self.source)
            .finish()
    }
}

impl OrchestrateUseCase {
    #[must_use]
    pub fn new(
        deliberate: Arc<DeliberateUseCase>,
        executor: Arc<dyn ExecutorPort>,
        messaging: Arc<dyn MessagingPort>,
        clock: Arc<dyn ClockPort>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            deliberate,
            executor,
            messaging,
            clock,
            source: source.into(),
        }
    }

    pub async fn execute(
        &self,
        task: Task,
        execution_options: Attributes,
    ) -> Result<OrchestrateOutput, DomainError> {
        let task_id = task.id().clone();
        let specialty = task.specialty().clone();

        let out = self.deliberate.execute(task).await?;
        let winner = out
            .deliberation
            .proposals()
            .get(&out.winner_proposal_id)
            .cloned()
            .ok_or(DomainError::NotFound {
                what: "winner proposal",
            })?;

        self.messaging
            .publish_task_dispatched(&TaskDispatchedEvent::new(
                self.envelope(self.clock.now())?,
                task_id.clone(),
                specialty.clone(),
                None,
            ))
            .await?;

        match self.executor.execute(&winner, &execution_options).await {
            Ok(execution) => {
                self.messaging
                    .publish_task_completed(&TaskCompletedEvent::new(
                        self.envelope(self.clock.now())?,
                        task_id.clone(),
                        specialty.clone(),
                        Some(winner.author().clone()),
                        execution.duration,
                    ))
                    .await?;
                info!(
                    task_id = task_id.as_str(),
                    execution_id = execution.execution_id.as_str(),
                    "orchestration completed"
                );
                Ok(OrchestrateOutput {
                    deliberation: out.deliberation,
                    winner,
                    execution,
                })
            }
            Err(err) => {
                let event = TaskFailedEvent::new(
                    self.envelope(self.clock.now())?,
                    task_id.clone(),
                    specialty,
                    "executor.error",
                    err.to_string(),
                )?;
                self.messaging.publish_task_failed(&event).await?;
                error!(task_id = task_id.as_str(), error = %err, "orchestration failed");
                Err(err)
            }
        }
    }

    fn envelope(&self, emitted_at: OffsetDateTime) -> Result<EventEnvelope, DomainError> {
        EventEnvelope::new(
            EventId::new(Uuid::new_v4().to_string())?,
            emitted_at,
            self.source.clone(),
            None,
        )
    }
}
