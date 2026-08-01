//! [`CompleteCeremonyStepUseCase`] — apply a step result to a ceremony.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::ClockPort;

use super::complete_ceremony_step_input::CompleteCeremonyStepInput;
use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use crate::services::{session_facts, SessionJournal};

pub struct CompleteCeremonyStepUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for CompleteCeremonyStepUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompleteCeremonyStepUseCase").finish()
    }
}

impl CompleteCeremonyStepUseCase {
    #[must_use]
    pub fn new(
        definitions: Arc<ResolveCeremonyDefinitionUseCase>,
        journal: Arc<SessionJournal>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            definitions,
            journal,
            clock,
        }
    }

    #[tracing::instrument(
        name = "complete_ceremony_step",
        skip_all,
        fields(ceremony_id = %input.instance_id, step_id = %input.step_id)
    )]
    pub async fn execute(
        &self,
        input: CompleteCeremonyStepInput,
    ) -> Result<CeremonyInstance, DomainError> {
        // Loaded through the journal so the revision is read before
        // the session: the other order lets a concurrent write turn a
        // race into a silent overwrite.
        let mut session = self.journal.load(&input.instance_id).await?;
        // Resolved from the instance, never from the request: a session
        // bound to a published version must be advanced by the very
        // definition it recorded, and one that is unbound has only the
        // repository to go to. Reading coordinates off the caller made
        // a bound session unadvanceable, because publishing writes to
        // the catalogue and not to the repository.
        let definition = self.definitions.execute(&session.instance).await?;
        // The seat is the definition's to say, as it is everywhere a
        // step is run. Only what filled it had to be declared.
        let finished_by = definition.role_id_for_step(&input.step_id)?;
        let now = self.clock.now();
        let result = input.result;
        session
            .instance
            .apply_step_result(&definition, &input.step_id, result.clone(), now)?;
        // Read back rather than carried in: the attempt this result
        // belongs to is the one the session recorded when the step was
        // claimed, and a caller reporting a result is in no position to
        // say which attempt it was.
        let attempt = session
            .instance
            .step_record(&input.step_id)
            .ok_or(DomainError::NotFound {
                what: "ceremony_step",
            })?
            .attempt();
        let fact = session_facts::step_finished(
            &session.instance,
            &input.step_id,
            attempt,
            &result,
            &finished_by,
            input.actor_kind,
            now,
        )?;
        self.journal.commit(session, vec![fact]).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::{
        AuditActorKind, AuditEventType, StepOutput, StepResult, StepStatus,
    };

    use super::*;
    use crate::usecases::ceremony_test_support::{
        ceremony_id, definition, definition_resolver, idempotency_key, journal, journal_over,
        lease_owner, now, role_id, started_instance, step_id, DefinitionRepositoryFake, FixedClock,
        InstanceRepositoryFake,
    };

    #[tokio::test]
    async fn applies_step_result_and_clears_lease() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let mut instance = started_instance(&definition);
        let lease = choreo_core::value_objects::StepLease::new(
            lease_owner(),
            idempotency_key("lease-1"),
            now(),
            now() + time::Duration::seconds(60),
        )
        .unwrap();
        instance
            .start_step_as(&definition, &role_id(), &step_id(), lease, now())
            .unwrap();
        instances.save(&instance).await.unwrap();
        let usecase = CompleteCeremonyStepUseCase::new(
            definition_resolver(definitions),
            journal(instances.clone()),
            Arc::new(FixedClock::new(now())),
        );

        let completed = usecase
            .execute(CompleteCeremonyStepInput::new(
                ceremony_id(),
                step_id(),
                StepResult::completed(StepOutput::empty()).unwrap(),
                AuditActorKind::Agent,
            ))
            .await
            .unwrap();

        let record = completed.step_record(&step_id()).unwrap();
        assert_eq!(record.status(), StepStatus::Completed);
        assert!(record.lease().is_none());
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

    /// A result reported from outside still names an attempt, and the
    /// session is what names it.
    ///
    /// This path exists for hosts that run the work themselves, so the
    /// engine never saw the step run and has only what it recorded when
    /// the step was claimed. Taking the attempt from the caller would
    /// let a late result be filed against an attempt that is no longer
    /// the one running — the retry's ending recorded under the attempt
    /// it replaced.
    #[tokio::test]
    async fn files_the_ending_under_the_attempt_the_session_recorded() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let mut instance = started_instance(&definition);
        instance
            .start_step(
                &definition,
                &step_id(),
                choreo_core::value_objects::StepLease::new(
                    lease_owner(),
                    idempotency_key("lease-1"),
                    now(),
                    now() + time::Duration::seconds(60),
                )
                .unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();
        let attempt_when_claimed = instance.step_record(&step_id()).unwrap().attempt();
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = CompleteCeremonyStepUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
        );

        usecase
            .execute(CompleteCeremonyStepInput::new(
                ceremony_id(),
                step_id(),
                StepResult::completed(StepOutput::empty()).unwrap(),
                AuditActorKind::Human,
            ))
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        assert_eq!(facts.len(), 1, "one ending, one fact: {facts:?}");
        assert_eq!(facts[0].event_type, AuditEventType::StepCompleted);
        assert_eq!(facts[0].actor.kind(), AuditActorKind::Human);
        assert!(
            facts[0]
                .event_id
                .as_str()
                .contains(&format!("attempt:{}", attempt_when_claimed.get())),
            "the ending was filed under an attempt the session never claimed: {}",
            facts[0].event_id.as_str()
        );
        // The seat came from the definition, not from the caller, who
        // never named one.
        assert_eq!(
            facts[0].actor.role_id(),
            Some(&definition.role_id_for_step(&step_id()).unwrap())
        );
    }
}
