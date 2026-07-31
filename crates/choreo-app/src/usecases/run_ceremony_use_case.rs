//! [`RunCeremonyUseCase`] — execute a declarative ceremony to terminal state.

use std::sync::Arc;

use choreo_core::entities::{CeremonyDefinition, CeremonyInstance};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort, CeremonyStepHandlerPort,
    CeremonyStepHandlerRequest, CeremonyTranscriptStorePort, ClockPort, MetricsRecorderPort,
    NoopMetricsRecorder,
};
use choreo_core::value_objects::{
    CeremonyOutcome, CeremonyStepContribution, CeremonyTranscript, DurationMs, IdempotencyKey,
    LeaseOwnerId, RoleId, StepAttempt, StepErrorMessage, StepId, StepLease, StepResult,
};
use time::OffsetDateTime;

use super::ceremony_step_trace::CeremonyStepTrace;
use super::run_ceremony_input::RunCeremonyInput;
use super::run_ceremony_output::RunCeremonyOutput;

/// Whole-millisecond duration between two clock readings, saturating at
/// zero so a non-monotonic clock can never produce a negative latency.
fn ms_since(start: OffsetDateTime, end: OffsetDateTime) -> DurationMs {
    DurationMs::from_millis(u64::try_from((end - start).whole_milliseconds()).unwrap_or(0))
}

pub struct RunCeremonyUseCase {
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
    handler: Arc<dyn CeremonyStepHandlerPort>,
    transcript_store: Arc<dyn CeremonyTranscriptStorePort>,
    clock: Arc<dyn ClockPort>,
    metrics: Arc<dyn MetricsRecorderPort>,
}

impl std::fmt::Debug for RunCeremonyUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunCeremonyUseCase").finish()
    }
}

impl RunCeremonyUseCase {
    #[must_use]
    pub fn new(
        definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
        instances: Arc<dyn CeremonyInstanceRepositoryPort>,
        handler: Arc<dyn CeremonyStepHandlerPort>,
        transcript_store: Arc<dyn CeremonyTranscriptStorePort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            definitions,
            instances,
            handler,
            transcript_store,
            clock,
            metrics: Arc::new(NoopMetricsRecorder),
        }
    }

    /// Attach a metrics recorder so ceremony outcomes, durations and step
    /// status are counted. The composition root wires the real recorder;
    /// the default no-op keeps tests and bespoke uses free of one.
    #[must_use]
    pub fn with_metrics(mut self, metrics: Arc<dyn MetricsRecorderPort>) -> Self {
        self.metrics = metrics;
        self
    }

    // The ceremony driver is one cohesive FSM loop; splitting it would
    // scatter the state-machine logic and its interleaved instrumentation.
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(
        name = "run_ceremony",
        skip_all,
        fields(ceremony_id = %input.id())
    )]
    pub async fn execute(&self, input: RunCeremonyInput) -> Result<RunCeremonyOutput, DomainError> {
        let (id, definition, context, lease_owner_id, lease_ttl) = input.into_parts();
        let ceremony_name = definition.name().as_str().to_owned();
        if self.instances.exists(&id).await? {
            self.metrics
                .record_ceremony_outcome(&ceremony_name, CeremonyOutcome::AlreadyExists);
            return Err(DomainError::AlreadyExists {
                what: "ceremony_instance",
            });
        }
        self.definitions.save(&definition).await?;

        let started_at = self.clock.now();
        let mut instance = CeremonyInstance::start(id.clone(), &definition, context, started_at);
        self.instances.save(&instance).await?;

        let max_iterations = definition
            .states()
            .len()
            .saturating_add(definition.transitions().len())
            .saturating_add(1);
        let mut step_traces = Vec::new();
        for _ in 0..max_iterations {
            if instance.is_completed(&definition) {
                self.metrics.observe_ceremony_duration(
                    &ceremony_name,
                    ms_since(started_at, self.clock.now()),
                );
                self.metrics
                    .record_ceremony_outcome(&ceremony_name, CeremonyOutcome::Completed);
                return Ok(RunCeremonyOutput::new(definition, instance, step_traces));
            }

            let state_id = instance.current_state().clone();
            let step_ids = definition
                .steps_for_state(&state_id)
                .map(|step| step.id().clone())
                .collect::<Vec<_>>();
            for step_id in step_ids {
                if instance
                    .step_record(&step_id)
                    .is_some_and(|record| record.status().is_success())
                {
                    continue;
                }
                let role_id = definition.role_id_for_step(&step_id)?;
                let transcript = self.transcript_store.transcript(&id).await?;
                let step_started = self.clock.now();
                let (attempt, step_result) = self
                    .run_step(
                        &definition,
                        &mut instance,
                        &role_id,
                        &step_id,
                        &lease_owner_id,
                        lease_ttl,
                        step_traces.len(),
                        transcript,
                    )
                    .await?;
                self.metrics.observe_ceremony_step_duration(
                    &ceremony_name,
                    step_id.as_str(),
                    ms_since(step_started, self.clock.now()),
                );
                self.metrics.record_ceremony_step(
                    &ceremony_name,
                    step_id.as_str(),
                    step_result.status(),
                );
                if step_result.is_success() {
                    self.transcript_store
                        .append(
                            &id,
                            CeremonyStepContribution::new(
                                step_id.clone(),
                                role_id.clone(),
                                step_result.output().clone(),
                            ),
                        )
                        .await?;
                }
                step_traces.push(CeremonyStepTrace::new(
                    state_id.clone(),
                    step_id,
                    role_id,
                    attempt,
                    step_result.status(),
                    step_result.output().clone(),
                ));
                if !step_result.is_success() {
                    self.metrics
                        .record_ceremony_outcome(&ceremony_name, CeremonyOutcome::StepFailed);
                    return Err(DomainError::InvariantViolated {
                        reason: "ceremony step did not complete successfully",
                    });
                }
            }

            if instance.is_completed(&definition) {
                self.metrics.observe_ceremony_duration(
                    &ceremony_name,
                    ms_since(started_at, self.clock.now()),
                );
                self.metrics
                    .record_ceremony_outcome(&ceremony_name, CeremonyOutcome::Completed);
                return Ok(RunCeremonyOutput::new(definition, instance, step_traces));
            }
            let Some(transition) = definition.next_satisfied_transition(
                &state_id,
                instance.step_records(),
                instance.context(),
            ) else {
                self.metrics
                    .record_ceremony_transition_blocked(&ceremony_name, state_id.as_str());
                self.metrics
                    .record_ceremony_outcome(&ceremony_name, CeremonyOutcome::NoTransition);
                return Err(DomainError::InvariantViolated {
                    reason: "no satisfied ceremony transition is available",
                });
            };
            let role_id = definition.role_id_for_transition(transition.trigger())?;
            instance.apply_transition_as(
                &definition,
                &role_id,
                transition.trigger(),
                self.clock.now(),
            )?;
            self.instances.save(&instance).await?;
        }

        self.metrics
            .record_ceremony_outcome(&ceremony_name, CeremonyOutcome::IterationLimit);
        Err(DomainError::InvariantViolated {
            reason: "ceremony execution exceeded transition safety limit",
        })
    }

    async fn run_step(
        &self,
        definition: &CeremonyDefinition,
        instance: &mut CeremonyInstance,
        role_id: &RoleId,
        step_id: &StepId,
        lease_owner_id: &LeaseOwnerId,
        lease_ttl: DurationMs,
        trace_index: usize,
        transcript: CeremonyTranscript,
    ) -> Result<(StepAttempt, StepResult), DomainError> {
        let step = definition
            .step(step_id)
            .cloned()
            .ok_or(DomainError::NotFound {
                what: "ceremony_step",
            })?;
        let now = self.clock.now();
        let lease = StepLease::acquire(
            lease_owner_id.clone(),
            IdempotencyKey::new(format!(
                "{}:{}:{}",
                instance.id().as_str(),
                step_id.as_str(),
                trace_index + 1
            ))?,
            now,
            lease_ttl,
        )?;
        let attempt = instance.start_step_as(definition, role_id, step_id, lease, now)?;
        self.instances.save(instance).await?;

        let request = CeremonyStepHandlerRequest::new(
            instance.id().clone(),
            instance.definition_name().clone(),
            instance.definition_version().clone(),
            instance.current_state().clone(),
            step.id().clone(),
            step.handler_kind().clone(),
            step.handler_config().clone(),
            instance.context().clone(),
            attempt,
        )
        .with_transcript(transcript)
        .with_role(role_id.clone())
        .with_bound_specialty(instance.bound_specialty(role_id).cloned());
        let step_result = self.execute_handler(request).await?;

        let mut refreshed = self.instances.get(instance.id()).await?;
        refreshed.apply_step_result(definition, step_id, step_result.clone(), self.clock.now())?;
        self.instances.save(&refreshed).await?;
        *instance = refreshed;

        Ok((attempt, step_result))
    }

    async fn execute_handler(
        &self,
        request: CeremonyStepHandlerRequest,
    ) -> Result<StepResult, DomainError> {
        match self.handler.execute(request).await {
            Ok(result) => Ok(result),
            Err(error) => {
                let message = StepErrorMessage::new(error.to_string())?;
                StepResult::failed(message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use choreo_core::error::DomainError;
    use choreo_core::ports::{CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort};
    use choreo_core::value_objects::{CeremonyContext, StepOutput, StepResult, StepStatus};

    use super::*;
    use crate::usecases::ceremony_test_support::{
        approval_definition, ceremony_id, definition, lease_owner, lease_ttl, now,
        started_instance, step_id, two_step_definition, ContextStoreFake, DefinitionRepositoryFake,
        FixedClock, InstanceRepositoryFake, StepHandlerFake,
    };

    #[tokio::test]
    async fn executes_linear_ceremony_to_terminal_state() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::default());
        let instances = Arc::new(InstanceRepositoryFake::default());
        let handler = Arc::new(StepHandlerFake::succeeding(
            StepResult::completed(StepOutput::empty()).unwrap(),
        ));
        let usecase = RunCeremonyUseCase::new(
            definitions,
            instances.clone(),
            handler.clone(),
            Arc::new(ContextStoreFake::default()),
            Arc::new(FixedClock::new(now())),
        );

        let output = usecase
            .execute(RunCeremonyInput::new(
                ceremony_id(),
                definition.clone(),
                CeremonyContext::empty(),
                lease_owner(),
                lease_ttl(),
            ))
            .await
            .unwrap();

        assert!(output.instance().is_completed(&definition));
        assert_eq!(output.step_traces().len(), 1);
        assert_eq!(output.step_traces()[0].step_id(), &step_id());
        assert_eq!(output.step_traces()[0].status(), StepStatus::Completed);
        assert_eq!(handler.requests().await.len(), 1);
        assert!(instances
            .saved(&ceremony_id())
            .await
            .is_completed(&definition));
    }

    #[tokio::test]
    async fn aborts_when_a_step_does_not_complete_successfully() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::default());
        let instances = Arc::new(InstanceRepositoryFake::default());
        let handler = Arc::new(StepHandlerFake::failing(DomainError::InvariantViolated {
            reason: "handler rejected step",
        }));
        let usecase = RunCeremonyUseCase::new(
            definitions,
            instances.clone(),
            handler,
            Arc::new(ContextStoreFake::default()),
            Arc::new(FixedClock::new(now())),
        );

        let err = usecase
            .execute(RunCeremonyInput::new(
                ceremony_id(),
                definition.clone(),
                CeremonyContext::empty(),
                lease_owner(),
                lease_ttl(),
            ))
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "ceremony step did not complete successfully"
            }
        ));
        // The failed step is recorded; the ceremony did not reach its
        // terminal state.
        assert!(!instances
            .saved(&ceremony_id())
            .await
            .is_completed(&definition));
    }

    #[tokio::test]
    async fn fails_when_no_outgoing_transition_is_satisfied() {
        // The approval ceremony can only advance through a human-approval
        // guard; with no approval in the context, no transition is
        // enabled out of the initial state.
        let definition = approval_definition();
        let definitions = Arc::new(DefinitionRepositoryFake::default());
        let instances = Arc::new(InstanceRepositoryFake::default());
        let handler = Arc::new(StepHandlerFake::succeeding(
            StepResult::completed(StepOutput::empty()).unwrap(),
        ));
        let usecase = RunCeremonyUseCase::new(
            definitions,
            instances,
            handler.clone(),
            Arc::new(ContextStoreFake::default()),
            Arc::new(FixedClock::new(now())),
        );

        let err = usecase
            .execute(RunCeremonyInput::new(
                ceremony_id(),
                definition,
                CeremonyContext::empty(),
                lease_owner(),
                lease_ttl(),
            ))
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "no satisfied ceremony transition is available"
            }
        ));
        // The approval ceremony declares no steps, so the handler is
        // never invoked.
        assert!(handler.requests().await.is_empty());
    }

    #[tokio::test]
    async fn duplicate_instance_id_is_rejected() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::default());
        let instances = Arc::new(InstanceRepositoryFake::default());
        let handler = Arc::new(StepHandlerFake::succeeding(
            StepResult::completed(StepOutput::empty()).unwrap(),
        ));
        let usecase = RunCeremonyUseCase::new(
            definitions,
            instances,
            handler,
            Arc::new(ContextStoreFake::default()),
            Arc::new(FixedClock::new(now())),
        );
        let input = || {
            RunCeremonyInput::new(
                ceremony_id(),
                definition.clone(),
                CeremonyContext::empty(),
                lease_owner(),
                lease_ttl(),
            )
        };

        usecase.execute(input()).await.unwrap();
        let err = usecase.execute(input()).await.unwrap_err();

        assert!(matches!(
            err,
            DomainError::AlreadyExists {
                what: "ceremony_instance"
            }
        ));
    }

    #[tokio::test]
    async fn duplicate_instance_id_is_rejected_before_saving_definition() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::default());
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let handler = Arc::new(StepHandlerFake::succeeding(
            StepResult::completed(StepOutput::empty()).unwrap(),
        ));
        let usecase = RunCeremonyUseCase::new(
            definitions.clone(),
            instances,
            handler.clone(),
            Arc::new(ContextStoreFake::default()),
            Arc::new(FixedClock::new(now())),
        );

        let err = usecase
            .execute(RunCeremonyInput::new(
                ceremony_id(),
                definition,
                CeremonyContext::empty(),
                lease_owner(),
                lease_ttl(),
            ))
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DomainError::AlreadyExists {
                what: "ceremony_instance"
            }
        ));
        assert!(definitions.list().await.unwrap().is_empty());
        assert!(handler.requests().await.is_empty());
    }

    #[tokio::test]
    async fn threads_prior_step_output_into_the_next_step() {
        let definition = two_step_definition();
        let definitions = Arc::new(DefinitionRepositoryFake::default());
        let instances = Arc::new(InstanceRepositoryFake::default());
        let handler = Arc::new(StepHandlerFake::succeeding(
            StepResult::completed(StepOutput::empty()).unwrap(),
        ));
        let usecase = RunCeremonyUseCase::new(
            definitions,
            instances,
            handler.clone(),
            Arc::new(ContextStoreFake::default()),
            Arc::new(FixedClock::new(now())),
        );

        usecase
            .execute(RunCeremonyInput::new(
                ceremony_id(),
                definition,
                CeremonyContext::empty(),
                lease_owner(),
                lease_ttl(),
            ))
            .await
            .unwrap();

        let requests = handler.requests().await;
        assert_eq!(requests.len(), 2);
        // The first step opens the meeting with an empty transcript.
        assert!(requests[0].transcript().is_empty());
        // The second step receives the first step's contribution.
        let transcript = requests[1].transcript();
        assert_eq!(transcript.len(), 1);
        assert_eq!(transcript.contributions()[0].step_id().as_str(), "open");
        assert_eq!(
            transcript.contributions()[0].role_id().as_str(),
            "FACILITATOR"
        );
    }
}
