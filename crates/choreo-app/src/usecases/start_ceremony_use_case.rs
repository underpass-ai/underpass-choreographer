//! [`StartCeremonyUseCase`] — create a ceremony instance from a definition.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyDefinitionRepositoryPort, ClockPort};

use super::start_ceremony_input::StartCeremonyInput;
use crate::services::{session_facts, SessionJournal};

pub struct StartCeremonyUseCase {
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for StartCeremonyUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartCeremonyUseCase").finish()
    }
}

impl StartCeremonyUseCase {
    #[must_use]
    pub fn new(
        definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
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
        name = "start_ceremony",
        skip_all,
        fields(ceremony_id = %input.id)
    )]
    pub async fn execute(
        &self,
        input: StartCeremonyInput,
    ) -> Result<CeremonyInstance, DomainError> {
        // No `exists` check before storing. Asking and then storing
        // leaves a gap two concurrent starts both walk through, and the
        // second would replace the first in silence. The commit itself
        // refuses, because it expects the session to be new.
        let definition = self
            .definitions
            .get(&input.definition_name, &input.definition_version)
            .await?;
        let now = self.clock.now();
        let instance = CeremonyInstance::start(input.id, &definition, input.context, now);
        // Built before the commit so a caller who named themselves
        // badly is refused without a session being left behind.
        let fact =
            session_facts::ceremony_started(&instance, &input.actor_id, input.actor_kind, now)?;
        self.journal
            .open(instance, vec![fact])
            .await
            .map(|session| session.instance)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use choreo_core::error::DomainError;
    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::{AuditActorKind, AuditEventType, CeremonyContext, StateId};

    use super::*;
    use crate::usecases::ceremony_test_support::{
        ceremony_id, definition, definition_name, journal, journal_over, now, started_instance,
        version, DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
    };

    #[tokio::test]
    async fn starts_and_persists_instance_at_initial_state() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let usecase = StartCeremonyUseCase::new(
            definitions,
            journal(instances.clone()),
            Arc::new(FixedClock::new(now())),
        );

        let instance = usecase
            .execute(StartCeremonyInput::new(
                ceremony_id(),
                definition_name(),
                version(),
                CeremonyContext::empty(),
                "operator-1",
                AuditActorKind::Service,
            ))
            .await
            .unwrap();

        assert_eq!(
            instance.current_state(),
            &StateId::new("COLLECTING_VOICES").unwrap()
        );
        let saved = instances.saved(&ceremony_id()).await;
        assert_eq!(saved.id(), &ceremony_id());
    }

    #[tokio::test]
    async fn duplicate_instance_id_is_rejected_before_overwrite() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let usecase = StartCeremonyUseCase::new(
            definitions,
            journal(instances),
            Arc::new(FixedClock::new(now())),
        );

        let err = usecase
            .execute(StartCeremonyInput::new(
                ceremony_id(),
                definition_name(),
                version(),
                CeremonyContext::empty(),
                "operator-1",
                AuditActorKind::Service,
            ))
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DomainError::AlreadyExists {
                what: "ceremony_instance"
            }
        ));
    }

    /// Opening a session is the journal's first entry.
    ///
    /// The actor has no seat, on purpose: at the start the definition's
    /// roles are not filled, and whoever opened this may never take
    /// part in it.
    #[tokio::test]
    async fn seals_the_opening_into_the_journal() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let (journal, unit_of_work) = journal_over(instances);
        let usecase =
            StartCeremonyUseCase::new(definitions, journal, Arc::new(FixedClock::new(now())));

        usecase
            .execute(StartCeremonyInput::new(
                ceremony_id(),
                definition_name(),
                version(),
                CeremonyContext::empty(),
                "scheduler-1",
                AuditActorKind::Service,
            ))
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        assert_eq!(facts.len(), 1, "one opening, one fact: {facts:?}");
        assert_eq!(facts[0].event_type, AuditEventType::CeremonyInstanceStarted);
        assert_eq!(facts[0].actor.kind(), AuditActorKind::Service);
        assert_eq!(facts[0].actor.actor_id(), "scheduler-1");
        assert!(
            facts[0].actor.role_id().is_none(),
            "the opener was given a seat this ceremony never assigned"
        );
    }

    /// A session opened badly leaves nothing behind.
    ///
    /// The fact is built before the commit, so a caller who names
    /// themselves with something the journal will not accept is refused
    /// without a session existing that has no record of being opened.
    #[tokio::test]
    async fn a_caller_who_cannot_be_named_opens_nothing() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let usecase = StartCeremonyUseCase::new(
            definitions,
            journal(instances.clone()),
            Arc::new(FixedClock::new(now())),
        );

        usecase
            .execute(StartCeremonyInput::new(
                ceremony_id(),
                definition_name(),
                version(),
                CeremonyContext::empty(),
                "   ",
                AuditActorKind::Service,
            ))
            .await
            .unwrap_err();

        assert!(
            !instances.exists(&ceremony_id()).await.unwrap(),
            "a session was opened that the journal cannot account for"
        );
    }
}
