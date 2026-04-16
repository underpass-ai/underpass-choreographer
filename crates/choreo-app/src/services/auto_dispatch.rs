//! [`AutoDispatchService`] — fan out one inbound trigger event into
//! one deliberation per requested specialty.
//!
//! Neutral port of `application/services/auto_dispatch_service.py`:
//! this layer is the bridge between the messaging adapter (which
//! receives a [`TriggerEvent`]) and the core deliberation use case.
//!
//! The service is fail-soft per specialty: if one deliberation fails,
//! the others still run; a summary [`AutoDispatchOutcome`] is returned
//! so callers can report both successes and failures.

use std::sync::Arc;

use choreo_core::entities::{Deliberation, Task, TaskConstraints};
use choreo_core::error::DomainError;
use choreo_core::events::TriggerEvent;
use choreo_core::value_objects::{Attributes, Specialty, TaskDescription, TaskId};
use tracing::{error, info};
use uuid::Uuid;

use crate::usecases::DeliberateUseCase;

/// Outcome of processing one trigger event.
#[derive(Debug, Clone, Default)]
pub struct AutoDispatchOutcome {
    pub successes: Vec<(Specialty, Deliberation)>,
    pub failures: Vec<(Specialty, DomainError)>,
}

impl AutoDispatchOutcome {
    #[must_use]
    pub fn accepted(&self) -> bool {
        !self.successes.is_empty()
    }

    #[must_use]
    pub fn dispatched_task_ids(&self) -> Vec<TaskId> {
        self.successes
            .iter()
            .map(|(_, d)| d.task_id().clone())
            .collect()
    }
}

pub struct AutoDispatchService {
    deliberate: Arc<DeliberateUseCase>,
    default_description: String,
}

impl std::fmt::Debug for AutoDispatchService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutoDispatchService")
            .field("default_description", &self.default_description)
            .finish()
    }
}

impl AutoDispatchService {
    /// `default_description` is the fallback prompt used when the
    /// trigger event does not carry a `task_description_template`.
    /// It must be non-empty.
    pub fn new(
        deliberate: Arc<DeliberateUseCase>,
        default_description: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let text = default_description.into();
        if text.trim().is_empty() {
            return Err(DomainError::EmptyField {
                field: "auto_dispatch.default_description",
            });
        }
        Ok(Self {
            deliberate,
            default_description: text,
        })
    }

    pub async fn dispatch(&self, event: &TriggerEvent) -> Result<AutoDispatchOutcome, DomainError> {
        let mut outcome = AutoDispatchOutcome::default();

        let description_text: String = match event.task_description_template() {
            Some(d) => d.as_str().to_owned(),
            None => self.default_description.clone(),
        };
        let description = TaskDescription::new(description_text)?;

        for specialty in event.requested_specialties() {
            let task = Self::build_task(
                specialty,
                description.clone(),
                event.constraints().clone(),
                event.payload().clone(),
            )?;
            match self.deliberate.execute(task).await {
                Ok(out) => {
                    info!(
                        event_id = event.envelope().event_id().as_str(),
                        specialty = specialty.as_str(),
                        "auto-dispatch succeeded"
                    );
                    outcome
                        .successes
                        .push((specialty.clone(), out.deliberation));
                }
                Err(err) => {
                    error!(
                        event_id = event.envelope().event_id().as_str(),
                        specialty = specialty.as_str(),
                        error = %err,
                        "auto-dispatch failed for specialty"
                    );
                    outcome.failures.push((specialty.clone(), err));
                }
            }
        }

        Ok(outcome)
    }

    fn build_task(
        specialty: &Specialty,
        description: TaskDescription,
        constraints: TaskConstraints,
        attributes: Attributes,
    ) -> Result<Task, DomainError> {
        Ok(Task::new(
            TaskId::new(Uuid::new_v4().to_string())?,
            specialty.clone(),
            description,
            constraints,
            attributes,
        ))
    }
}
