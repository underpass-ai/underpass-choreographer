//! [`RunCeremonyStepUseCase`] — acquire a step lease and invoke a handler.

use std::sync::Arc;

use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyStepHandlerPort, CeremonyStepHandlerRequest, CeremonyTranscriptStorePort, ClockPort,
    NoopCeremonyTranscriptStore,
};
use choreo_core::value_objects::{
    CeremonyStepContribution, StepErrorMessage, StepLease, StepResult,
};

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use super::run_ceremony_step_input::RunCeremonyStepInput;
use super::run_ceremony_step_output::RunCeremonyStepOutput;
use crate::services::{session_facts, SessionJournal};

pub struct RunCeremonyStepUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
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
        definitions: Arc<ResolveCeremonyDefinitionUseCase>,
        journal: Arc<SessionJournal>,
        handler: Arc<dyn CeremonyStepHandlerPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            definitions,
            journal,
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
        // Two commits, not one, and deliberately so: the claim has to
        // be durable before the handler is invoked, or a crash while it
        // runs leaves no record that anything took the step.
        let mut session = self.journal.load(&input.instance_id).await?;
        // Resolved from the instance, never from the request: a session
        // bound to a published version must be advanced by the very
        // definition it recorded, and one that is unbound has only the
        // repository to go to. Reading coordinates off the caller made
        // a bound session unadvanceable, because publishing writes to
        // the catalogue and not to the repository.
        let definition = self.definitions.execute(&session.instance).await?;
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
        let attempt = session.instance.start_step_as(
            &definition,
            &input.role_id,
            &input.step_id,
            lease,
            now,
        )?;
        let started = session_facts::step_started(
            &session.instance,
            &input.step_id,
            attempt,
            &input.role_id,
            input.role_kind,
            now,
        )?;
        let instance = self.journal.commit(session, vec![started]).await?.instance;

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
        .with_role(input.role_id.clone())
        .with_bound_specialty(instance.bound_specialty(&input.role_id).cloned());
        let result = self.execute_handler(request).await?;

        // Read again rather than reusing what was loaded before the
        // handler ran: it may have taken a while, and the revision that
        // was current then is not the one this commit has to hold.
        let mut session = self.journal.load(instance.id()).await?;
        let finished_at = self.clock.now();
        session.instance.apply_step_result(
            &definition,
            &input.step_id,
            result.clone(),
            finished_at,
        )?;
        let finished = session_facts::step_finished(
            &session.instance,
            &input.step_id,
            attempt,
            &result,
            &input.role_id,
            input.role_kind,
            finished_at,
        )?;
        let refreshed = self.journal.commit(session, vec![finished]).await?.instance;
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
    use choreo_core::value_objects::{
        AuditActorKind, AuditEventType, StepAttempt, StepErrorMessage, StepOutput, StepStatus,
    };

    use super::*;
    use crate::usecases::ceremony_test_support::{
        approval_definition, ceremony_id, definition, definition_resolver, idempotency_key,
        journal, journal_over, lease_owner, lease_ttl, now, resolver_with, role_id,
        started_instance, step_id, ContextStoreFake, DefinitionRepositoryFake, FixedClock,
        InstanceRepositoryFake, PublicationsFake, StepHandlerFake,
    };

    /// The regression this whole change exists for. Publishing writes
    /// to the catalogue and nowhere else, so a session bound to a
    /// published version used to be startable and then unadvanceable:
    /// the step resolved its definition from the repository, which had
    /// never heard of it. Here the repository deliberately holds a
    /// different ceremony, so the step can only run if resolution
    /// followed the binding.
    #[tokio::test]
    async fn a_bound_session_runs_the_definition_it_was_published_from() {
        let publications = Arc::new(PublicationsFake::default());
        let published = publications.seed(definition()).await;
        let elsewhere = Arc::new(DefinitionRepositoryFake::new(approval_definition()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&choreo_core::entities::CeremonyInstance::start_bound(
                ceremony_id(),
                &published,
                choreo_core::value_objects::CeremonyContext::empty(),
                now(),
            ))
            .await
            .unwrap();
        let usecase = RunCeremonyStepUseCase::new(
            resolver_with(elsewhere, publications),
            journal(instances.clone()),
            Arc::new(StepHandlerFake::succeeding(
                StepResult::completed(StepOutput::empty()).unwrap(),
            )),
            Arc::new(FixedClock::new(now())),
        );

        let output = usecase
            .execute(RunCeremonyStepInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
                step_id(),
                lease_owner(),
                idempotency_key("bound-1"),
                lease_ttl(),
            ))
            .await
            .expect("a bound session must be advanceable by what it is bound to");

        assert_eq!(output.result().status(), StepStatus::Completed);
        assert_eq!(
            output.instance().bound_definition(),
            Some(published.digest()),
            "advancing must not quietly unbind the session"
        );
    }

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
            definition_resolver(definitions),
            journal(instances.clone()),
            handler.clone(),
            Arc::new(FixedClock::new(now())),
        )
        .with_transcript_store(transcript_store.clone());

        let output = usecase
            .execute(RunCeremonyStepInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
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
            definition_resolver(definitions),
            journal(instances.clone()),
            handler,
            Arc::new(FixedClock::new(now())),
        );

        let output = usecase
            .execute(RunCeremonyStepInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
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
            definition_resolver(definitions),
            journal(instances),
            handler.clone(),
            Arc::new(FixedClock::new(now())),
        );

        let err = usecase
            .execute(RunCeremonyStepInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
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

    /// Taking a step and finishing it are two facts, and they are
    /// committed separately on purpose.
    ///
    /// The claim has to be durable before the handler is invoked. A
    /// crash while the handler runs must leave a session that says
    /// somebody took this step and never came back — not one that looks
    /// untouched.
    #[tokio::test]
    async fn seals_the_claim_before_the_work_and_the_ending_after() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = RunCeremonyStepUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(StepHandlerFake::succeeding(
                StepResult::completed(StepOutput::empty()).unwrap(),
            )),
            Arc::new(FixedClock::new(now())),
        );

        usecase
            .execute(RunCeremonyStepInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
                step_id(),
                lease_owner(),
                idempotency_key("run-1"),
                lease_ttl(),
            ))
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        let sealed = facts.iter().map(|fact| fact.event_type).collect::<Vec<_>>();
        assert_eq!(
            sealed,
            vec![AuditEventType::StepStarted, AuditEventType::StepCompleted],
            "taking the step and finishing it are two facts: {facts:?}"
        );
        assert!(facts
            .iter()
            .all(|fact| fact.actor.kind() == AuditActorKind::Agent));
    }

    /// A step that fails says so, rather than saying it ended.
    ///
    /// "Did anything fail here" is the first question asked of a
    /// session that went wrong, and it should not need reading into
    /// every entry to answer.
    #[tokio::test]
    async fn a_failed_step_is_sealed_as_a_failure() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = RunCeremonyStepUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(StepHandlerFake::succeeding(
                StepResult::failed(StepErrorMessage::new("the handler gave up").unwrap()).unwrap(),
            )),
            Arc::new(FixedClock::new(now())),
        );

        usecase
            .execute(RunCeremonyStepInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
                step_id(),
                lease_owner(),
                idempotency_key("run-1"),
                lease_ttl(),
            ))
            .await
            .unwrap();

        let sealed = unit_of_work
            .facts()
            .await
            .iter()
            .map(|fact| fact.event_type)
            .collect::<Vec<_>>();
        assert_eq!(
            sealed,
            vec![AuditEventType::StepStarted, AuditEventType::StepFailed],
            "a step that failed was sealed as something else"
        );
    }
}
