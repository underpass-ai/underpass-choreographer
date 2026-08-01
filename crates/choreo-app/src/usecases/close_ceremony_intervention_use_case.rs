//! [`CloseCeremonyInterventionUseCase`] — close a live agenda item.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::ClockPort;

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use super::CloseCeremonyInterventionInput;
use crate::services::{session_facts, SessionJournal};

pub struct CloseCeremonyInterventionUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for CloseCeremonyInterventionUseCase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CloseCeremonyInterventionUseCase")
            .finish()
    }
}

impl CloseCeremonyInterventionUseCase {
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
        name = "close_ceremony_intervention",
        skip_all,
        fields(
            ceremony_id = %input.instance_id,
            intervention_id = %input.intervention_id,
            role_id = %input.role_id,
        )
    )]
    pub async fn execute(
        &self,
        input: CloseCeremonyInterventionInput,
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
        let now = self.clock.now();
        session.instance.close_intervention_as(
            &definition,
            &input.intervention_id,
            &input.role_id,
            now,
        )?;
        // The item closing and the record of a seat having closed it
        // land together.
        let fact = session_facts::intervention_closed(
            &session.instance,
            &input.intervention_id,
            &input.role_id,
            input.role_kind,
            now,
        )?;
        self.journal.commit(session, vec![fact]).await
    }
}

#[cfg(test)]
mod tests {
    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::{
        Attributes, AuditActorKind, AuditEventType, CeremonyInterventionContent,
        CeremonyInterventionId, CeremonyInterventionKind, CeremonyInterventionStatus,
        CeremonyInterventionTarget,
    };

    use super::*;
    use crate::usecases::ceremony_test_support::{
        ceremony_id, definition, definition_resolver, journal, journal_over, now, role_id,
        started_instance, DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
    };

    #[tokio::test]
    async fn requester_closes_the_dynamic_agenda_item() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let intervention_id = CeremonyInterventionId::new("ask-table").unwrap();
        let mut instance = started_instance(&definition);
        instance
            .request_intervention_as(
                &definition,
                intervention_id.clone(),
                role_id(),
                CeremonyInterventionKind::Opinion,
                CeremonyInterventionTarget::table(),
                CeremonyInterventionContent::new("What do you think?", Attributes::empty())
                    .unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();
        let usecase = CloseCeremonyInterventionUseCase::new(
            definition_resolver(definitions),
            journal(instances),
            Arc::new(FixedClock::new(now())),
        );

        let instance = usecase
            .execute(CloseCeremonyInterventionInput::new(
                ceremony_id(),
                intervention_id.clone(),
                role_id(),
                AuditActorKind::Human,
            ))
            .await
            .unwrap();

        assert_eq!(
            instance.intervention(&intervention_id).unwrap().status(),
            CeremonyInterventionStatus::Closed
        );
    }

    /// A session with an open agenda item, ready to have it closed.
    async fn with_an_open_item() -> (
        Arc<DefinitionRepositoryFake>,
        Arc<InstanceRepositoryFake>,
        CeremonyInterventionId,
    ) {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let intervention_id = CeremonyInterventionId::new("ask-table").unwrap();
        let mut instance = started_instance(&definition);
        instance
            .request_intervention_as(
                &definition,
                intervention_id.clone(),
                role_id(),
                CeremonyInterventionKind::Opinion,
                CeremonyInterventionTarget::table(),
                CeremonyInterventionContent::new("What do you think?", Attributes::empty())
                    .unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();
        (definitions, instances, intervention_id)
    }

    /// Closing is a decision, and it leaves a record of who made it.
    #[tokio::test]
    async fn seals_the_closure_into_the_journal() {
        let (definitions, instances, intervention_id) = with_an_open_item().await;
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = CloseCeremonyInterventionUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
        );

        usecase
            .execute(CloseCeremonyInterventionInput::new(
                ceremony_id(),
                intervention_id,
                role_id(),
                AuditActorKind::Human,
            ))
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        assert_eq!(facts.len(), 1, "one closure, one fact: {facts:?}");
        assert_eq!(facts[0].event_type, AuditEventType::InterventionClosed);
        assert_eq!(facts[0].actor.kind(), AuditActorKind::Human);
    }

    /// Why the closure's id needs no seat, unlike a response's.
    ///
    /// A response is keyed on the answering seat because an item put to
    /// the table is answered by several. A closure is not: the session
    /// refuses a second one, so the item alone identifies it and a
    /// retry derives the same id rather than a second entry.
    ///
    /// This is the assertion behind that comment. Without it, the day
    /// closing twice becomes legal the journal would quietly record the
    /// second closure under the first one's id and lose it.
    #[tokio::test]
    async fn an_item_is_closed_once_and_the_session_says_so() {
        let (definitions, instances, intervention_id) = with_an_open_item().await;
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = CloseCeremonyInterventionUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
        );
        let closing = || {
            usecase.execute(CloseCeremonyInterventionInput::new(
                ceremony_id(),
                intervention_id.clone(),
                role_id(),
                AuditActorKind::Human,
            ))
        };

        closing().await.unwrap();
        let refused = closing().await;

        assert!(
            refused.is_err(),
            "closing an already-closed item was accepted, and its fact would collide with the \
             first: {refused:?}"
        );
        assert_eq!(
            unit_of_work.facts().await.len(),
            1,
            "a refused closure sealed a fact"
        );
    }
}
