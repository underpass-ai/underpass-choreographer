//! [`RunCeremonyStepUseCase`] — acquire a step lease and invoke a handler.

use std::sync::Arc;

use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort, CeremonyStepHandlerPort,
    CeremonyStepHandlerRequest, CeremonyTranscriptStorePort, ClockPort,
    NoopCeremonyTranscriptStore,
};
use choreo_core::value_objects::{
    CeremonyStepContribution, StepErrorMessage, StepLease, StepResult,
};

use super::run_ceremony_step_input::RunCeremonyStepInput;
use super::run_ceremony_step_output::RunCeremonyStepOutput;

pub struct RunCeremonyStepUseCase {
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
    handler: Arc<dyn CeremonyStepHandlerPort>,
    transcript_store: Arc<dyn CeremonyTranscriptStorePort>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for RunCeremonyStepUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunCeremonyStepUseCase").finish()
    }
}

impl RunCeremonyStepUseCase {
    #[must_use]
    pub fn new(
        definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
        instances: Arc<dyn CeremonyInstanceRepositoryPort>,
        handler: Arc<dyn CeremonyStepHandlerPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            definitions,
            instances,
            handler,
            transcript_store: Arc::new(NoopCeremonyTranscriptStore),
            clock,
        }
    }

    /// Attach transcript persistence for hosts that execute steps
    /// incrementally. The default no-op preserves the original constructor
    /// contract for callers that do not need cross-step context.
    #[must_use]
    pub fn with_transcript_store(
        mut self,
        transcript_store: Arc<dyn CeremonyTranscriptStorePort>,
    ) -> Self {
        self.transcript_store = transcript_store;
        self
    }

    #[tracing::instrument(
        name = "run_ceremony_step",
        skip_all,
        fields(ceremony_id = %input.instance_id, step_id = %input.step_id)
    )]
    pub async fn execute(
        &self,
        input: RunCeremonyStepInput,
    ) -> Result<RunCeremonyStepOutput, DomainError> {
        let definition = self
            .definitions
            .get(&input.definition_name, &input.definition_version)
            .await?;
        let mut instance = self.instances.get(&input.instance_id).await?;
        let step = definition
            .step(&input.step_id)
            .cloned()
            .ok_or(DomainError::NotFound {
                what: "ceremony_step",
            })?;

        let now = self.clock.now();
        let lease = StepLease::acquire(
            input.lease_owner_id,
            input.idempotency_key,
            now,
            input.lease_ttl,
        )?;
        let attempt =
            instance.start_step_as(&definition, &input.role_id, &input.step_id, lease, now)?;
        self.instances.save(&instance).await?;

        let transcript = self.transcript_store.transcript(instance.id()).await?;
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
        .with_interventions(instance.interventions().to_vec())
        .with_role(input.role_id.clone());
        let result = self.execute_handler(request).await?;

        let mut refreshed = self.instances.get(instance.id()).await?;
        refreshed.apply_step_result(
            &definition,
            &input.step_id,
            result.clone(),
            self.clock.now(),
        )?;
        self.instances.save(&refreshed).await?;
        if result.is_success() {
            self.transcript_store
                .append(
                    refreshed.id(),
                    CeremonyStepContribution::new(
                        input.step_id.clone(),
                        input.role_id,
                        result.output().clone(),
                    ),
                )
                .await?;
        }

        Ok(RunCeremonyStepOutput::new(refreshed, attempt, result))
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
    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::{StepAttempt, StepOutput, StepStatus};

    use super::*;
    use crate::usecases::ceremony_test_support::{
        ceremony_id, definition, definition_name, idempotency_key, lease_owner, lease_ttl, now,
        role_id, started_instance, step_id, version, ContextStoreFake, DefinitionRepositoryFake,
        FixedClock, InstanceRepositoryFake, StepHandlerFake,
    };

    #[tokio::test]
    async fn invokes_handler_and_persists_completed_result() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let handler = Arc::new(StepHandlerFake::succeeding(
            StepResult::completed(StepOutput::empty()).unwrap(),
        ));
        let transcript_store = Arc::new(ContextStoreFake::default());
        let usecase = RunCeremonyStepUseCase::new(
            definitions,
            instances.clone(),
            handler.clone(),
            Arc::new(FixedClock::new(now())),
        )
        .with_transcript_store(transcript_store.clone());

        let output = usecase
            .execute(RunCeremonyStepInput::new(
                ceremony_id(),
                definition_name(),
                version(),
                role_id(),
                step_id(),
                lease_owner(),
                idempotency_key("run-1"),
                lease_ttl(),
            ))
            .await
            .unwrap();

        assert_eq!(output.attempt(), StepAttempt::FIRST);
        assert_eq!(output.result().status(), StepStatus::Completed);
        let requests = handler.requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].step_id(), &step_id());
        assert_eq!(requests[0].handler_kind().as_str(), "multiagent_round");
        assert_eq!(requests[0].role_id(), Some(&role_id()));
        assert!(requests[0].transcript().is_empty());
        assert!(requests[0].interventions().is_empty());
        let transcript = transcript_store.transcript(&ceremony_id()).await.unwrap();
        assert_eq!(transcript.len(), 1);
        assert_eq!(transcript.contributions()[0].step_id(), &step_id());
        assert_eq!(
            instances
                .saved(&ceremony_id())
                .await
                .step_record(&step_id())
                .unwrap()
                .status(),
            StepStatus::Completed
        );
    }

    #[tokio::test]
    async fn handler_domain_error_is_persisted_as_failed_step() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let handler = Arc::new(StepHandlerFake::failing(DomainError::InvariantViolated {
            reason: "handler rejected step",
        }));
        let usecase = RunCeremonyStepUseCase::new(
            definitions,
            instances.clone(),
            handler,
            Arc::new(FixedClock::new(now())),
        );

        let output = usecase
            .execute(RunCeremonyStepInput::new(
                ceremony_id(),
                definition_name(),
                version(),
                role_id(),
                step_id(),
                lease_owner(),
                idempotency_key("run-1"),
                lease_ttl(),
            ))
            .await
            .unwrap();

        assert_eq!(output.result().status(), StepStatus::Failed);
        let saved = instances.saved(&ceremony_id()).await;
        let record = saved.step_record(&step_id()).unwrap();
        assert_eq!(record.status(), StepStatus::Failed);
        assert!(record.error_message().is_some());
    }

    #[tokio::test]
    async fn active_lease_blocks_runner_before_handler_invocation() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let mut instance = started_instance(&definition);
        instance
            .start_step(
                &definition,
                &step_id(),
                StepLease::new(
                    lease_owner(),
                    idempotency_key("existing-run"),
                    now(),
                    now() + time::Duration::seconds(60),
                )
                .unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();
        let handler = Arc::new(StepHandlerFake::succeeding(
            StepResult::completed(StepOutput::empty()).unwrap(),
        ));
        let usecase = RunCeremonyStepUseCase::new(
            definitions,
            instances,
            handler.clone(),
            Arc::new(FixedClock::new(now())),
        );

        let err = usecase
            .execute(RunCeremonyStepInput::new(
                ceremony_id(),
                definition_name(),
                version(),
                role_id(),
                step_id(),
                lease_owner(),
                idempotency_key("new-run"),
                lease_ttl(),
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::InvariantViolated { .. }));
        assert!(handler.requests().await.is_empty());
    }
}
