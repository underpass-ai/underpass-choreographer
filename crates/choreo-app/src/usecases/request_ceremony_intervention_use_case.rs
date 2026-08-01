//! [`RequestCeremonyInterventionUseCase`] — add a live agenda item.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::ClockPort;

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use super::RequestCeremonyInterventionInput;
use crate::services::{session_facts, SessionJournal};

pub struct RequestCeremonyInterventionUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for RequestCeremonyInterventionUseCase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RequestCeremonyInterventionUseCase")
            .finish()
    }
}

impl RequestCeremonyInterventionUseCase {
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
        name = "request_ceremony_intervention",
        skip_all,
        fields(
            ceremony_id = %input.instance_id,
            intervention_id = %input.intervention_id,
            role_id = %input.role_id,
        )
    )]
    pub async fn execute(
        &self,
        input: RequestCeremonyInterventionInput,
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
        let intervention_id = input.intervention_id.clone();
        let requested_by = input.role_id.clone();
        session.instance.request_intervention_with_provenance_as(
            &definition,
            input.intervention_id,
            input.role_id,
            input.kind,
            input.target,
            input.content,
            input.provenance,
            now,
        )?;
        // An intervention that asks the table for something, and the
        // record of somebody having asked, land together.
        let fact = session_facts::intervention_requested(
            &session.instance,
            &intervention_id,
            &requested_by,
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
        CeremonyInterventionId, CeremonyInterventionKind, CeremonyInterventionTarget,
    };

    use super::*;
    use crate::usecases::ceremony_test_support::{
        ceremony_id, definition, definition_resolver, journal, journal_over, now, role_id,
        started_instance, DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
    };

    #[tokio::test]
    async fn persists_a_dynamic_intervention_on_the_running_instance() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let usecase = RequestCeremonyInterventionUseCase::new(
            definition_resolver(definitions),
            journal(instances.clone()),
            Arc::new(FixedClock::new(now())),
        );
        let intervention_id = CeremonyInterventionId::new("ask-table").unwrap();

        let instance = usecase
            .execute(RequestCeremonyInterventionInput::new(
                ceremony_id(),
                intervention_id.clone(),
                role_id(),
                AuditActorKind::Human,
                CeremonyInterventionKind::Opinion,
                CeremonyInterventionTarget::table(),
                CeremonyInterventionContent::new("What does the table think?", Attributes::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(
            instance
                .intervention(&intervention_id)
                .unwrap()
                .requested_by(),
            &role_id()
        );
        assert!(instances
            .saved(&ceremony_id())
            .await
            .intervention(&intervention_id)
            .is_some());
    }

    /// Asking the table for something is a fact about the session.
    ///
    /// The agenda item itself says what was asked and by which seat.
    /// What it cannot say is that somebody asked at a moment — that is
    /// the journal's, and it is what a reader reconstructing the
    /// session's shape follows.
    #[tokio::test]
    async fn seals_the_request_into_the_journal() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = RequestCeremonyInterventionUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
        );

        usecase
            .execute(RequestCeremonyInterventionInput::new(
                ceremony_id(),
                CeremonyInterventionId::new("ask-table").unwrap(),
                role_id(),
                AuditActorKind::Agent,
                CeremonyInterventionKind::Opinion,
                CeremonyInterventionTarget::table(),
                CeremonyInterventionContent::new("What does the table think?", Attributes::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        assert_eq!(facts.len(), 1, "one request, one fact: {facts:?}");
        assert_eq!(facts[0].event_type, AuditEventType::InterventionRequested);
        // Declared by the caller and carried through. A guard requiring
        // a human says one was required; this says an agent asked.
        assert_eq!(facts[0].actor.kind(), AuditActorKind::Agent);
    }

    /// Two agenda items are two facts, not one written twice.
    ///
    /// The event id is derived, so it has to derive from something
    /// that differs between requests. Keyed on the session alone, the
    /// second ask would collide with the first and a reader would see
    /// one request where there were two.
    #[tokio::test]
    async fn two_requests_are_two_distinct_facts() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = RequestCeremonyInterventionUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
        );

        for asked in ["ask-one", "ask-two"] {
            usecase
                .execute(RequestCeremonyInterventionInput::new(
                    ceremony_id(),
                    CeremonyInterventionId::new(asked).unwrap(),
                    role_id(),
                    AuditActorKind::Agent,
                    CeremonyInterventionKind::Opinion,
                    CeremonyInterventionTarget::table(),
                    CeremonyInterventionContent::new(asked, Attributes::empty()).unwrap(),
                ))
                .await
                .unwrap();
        }

        let ids = unit_of_work
            .facts()
            .await
            .iter()
            .map(|fact| fact.event_id.as_str().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), 2, "two asks collapsed into one fact: {ids:?}");
    }
}
