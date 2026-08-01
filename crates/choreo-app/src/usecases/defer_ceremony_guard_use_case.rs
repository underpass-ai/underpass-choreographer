//! [`DeferCeremonyGuardUseCase`] — preserve a human decision deferral.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::ClockPort;
use choreo_core::value_objects::CeremonyRecordRef;

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use super::DeferCeremonyGuardInput;
use crate::services::{session_facts, SessionJournal, SessionMemoryRecorder};

pub struct DeferCeremonyGuardUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
    memory: Arc<SessionMemoryRecorder>,
}

impl std::fmt::Debug for DeferCeremonyGuardUseCase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("DeferCeremonyGuardUseCase").finish()
    }
}

impl DeferCeremonyGuardUseCase {
    #[must_use]
    pub fn new(
        definitions: Arc<ResolveCeremonyDefinitionUseCase>,
        journal: Arc<SessionJournal>,
        clock: Arc<dyn ClockPort>,
        memory: Arc<SessionMemoryRecorder>,
    ) -> Self {
        Self {
            definitions,
            journal,
            clock,
            memory,
        }
    }

    #[tracing::instrument(
        name = "defer_ceremony_guard",
        skip_all,
        fields(ceremony_id = %input.instance_id, guard_name = %input.guard_name)
    )]
    pub async fn execute(
        &self,
        input: DeferCeremonyGuardInput,
    ) -> Result<CeremonyInstance, DomainError> {
        // Loaded through the journal so the revision is read before
        // the session: the other order lets a concurrent write turn a
        // race into a silent overwrite.
        let mut session = self.journal.load(&input.instance_id).await?;
        // Resolved from the instance, never from the request: a session
        // bound to a published version must be advanced by the very
        // definition it recorded, and one that is unbound has only the
        // repository to go to.
        let definition = self.definitions.execute(&session.instance).await?;
        let decided = CeremonyRecordRef::guard_decision(input.guard_name.clone());
        let now = self.clock.now();
        session.instance.defer_guard(
            &definition,
            input.guard_name.clone(),
            input.content,
            input.role_id.clone(),
            input.role_kind,
            now,
        )?;
        let fact = session_facts::guard_deferred(
            &session.instance,
            &input.guard_name,
            &input.role_id,
            input.role_kind,
            now,
        )?;
        // Deciding not to decide is a decision, and it lands with its
        // record for the same reason an approval does.
        let instance = self.journal.commit(session, vec![fact]).await?.instance;
        // A human decision is the kind a later session weighs hardest,
        // and now it can say who made it.
        self.memory
            .remember_guard_decision(&instance, &decided)
            .await;
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::{CeremonyGuardDeferralContent, GuardName};

    use choreo_core::value_objects::{AuditActorKind, AuditEventType};

    use super::*;
    use crate::usecases::ceremony_test_support::{
        a_recorder, approval_definition, ceremony_id, definition_resolver, journal, journal_over,
        now, role_id, started_instance, DefinitionRepositoryFake, FixedClock,
        InstanceRepositoryFake,
    };

    #[tokio::test]
    async fn persists_a_human_guard_deferral_without_satisfying_the_guard() {
        let definition = approval_definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let usecase = DeferCeremonyGuardUseCase::new(
            definition_resolver(definitions),
            journal(instances.clone()),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );
        let guard_name = GuardName::new("human_approved").unwrap();

        let deferred = usecase
            .execute(DeferCeremonyGuardInput::new(
                ceremony_id(),
                guard_name.clone(),
                CeremonyGuardDeferralContent::new(
                    "I do not know.",
                    "The available evidence is inconclusive.",
                    vec!["New evidence clarifies the outcome.".to_owned()],
                )
                .unwrap(),
                role_id(),
                AuditActorKind::Human,
            ))
            .await
            .unwrap();

        assert!(!deferred.context().is_guard_approved(&guard_name));
        assert_eq!(deferred.guard_deferrals().len(), 1);
        assert_eq!(
            instances
                .saved(&ceremony_id())
                .await
                .guard_deferrals()
                .len(),
            1
        );
    }

    /// A deferral is a decision, and it leaves the same kind of trace.
    ///
    /// The guard stays unsatisfied either way, so the state cannot tell
    /// a session that was deliberately left open from one nobody ever
    /// looked at. The journal is the only place that difference exists.
    #[tokio::test]
    async fn seals_the_deferral_into_the_journal() {
        let definition = approval_definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let (journal, unit_of_work) = journal_over(instances.clone());
        let usecase = DeferCeremonyGuardUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        usecase
            .execute(DeferCeremonyGuardInput::new(
                ceremony_id(),
                GuardName::new("human_approved").unwrap(),
                CeremonyGuardDeferralContent::new(
                    "I do not know.",
                    "The available evidence is inconclusive.",
                    vec!["New evidence clarifies the outcome.".to_owned()],
                )
                .unwrap(),
                role_id(),
                AuditActorKind::Human,
            ))
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        assert_eq!(facts.len(), 1, "one deferral, one fact: {facts:?}");
        assert_eq!(facts[0].event_type, AuditEventType::HumanDeferralRecorded);
    }
}
