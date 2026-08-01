//! [`RespondToCeremonyInterventionUseCase`] — contribute to a live agenda item.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::ClockPort;

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use super::RespondToCeremonyInterventionInput;
use crate::services::{session_facts, SessionJournal, SessionMemoryRecorder};

pub struct RespondToCeremonyInterventionUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
    memory: Arc<SessionMemoryRecorder>,
}

impl std::fmt::Debug for RespondToCeremonyInterventionUseCase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RespondToCeremonyInterventionUseCase")
            .finish()
    }
}

impl RespondToCeremonyInterventionUseCase {
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
        name = "respond_to_ceremony_intervention",
        skip_all,
        fields(
            ceremony_id = %input.instance_id,
            intervention_id = %input.intervention_id,
            role_id = %input.role_id,
        )
    )]
    pub async fn execute(
        &self,
        input: RespondToCeremonyInterventionInput,
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
        let responded_by = input.role_id.clone();
        session.instance.respond_to_intervention_as(
            &definition,
            &input.intervention_id,
            input.role_id,
            input.content,
            now,
        )?;
        // The contribution and the record of a seat having made it
        // land together.
        let fact = session_facts::intervention_responded(
            &session.instance,
            &input.intervention_id,
            &responded_by,
            input.role_kind,
            now,
        )?;
        let instance = self.journal.commit(session, vec![fact]).await?;
        // After the session is safely stored, never before: a memory
        // of something that failed to persist would outlive the thing
        // it describes.
        self.memory
            .remember_contribution(&instance, &input.intervention_id)
            .await;
        Ok(instance)
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
        a_recorder, ceremony_id, definition, definition_resolver, journal, journal_over, now,
        respondent_role_id, role_id, started_instance, DefinitionRepositoryFake, FixedClock,
        InstanceRepositoryFake,
    };
    use crate::usecases::{RequestCeremonyInterventionInput, RequestCeremonyInterventionUseCase};

    #[tokio::test]
    async fn persists_a_table_members_response() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let intervention_id = CeremonyInterventionId::new("inspect-queue").unwrap();
        let mut instance = started_instance(&definition);
        instance
            .request_intervention_as(
                &definition,
                intervention_id.clone(),
                role_id(),
                CeremonyInterventionKind::Investigation,
                CeremonyInterventionTarget::roles([respondent_role_id()]).unwrap(),
                CeremonyInterventionContent::new("Inspect the queue.", Attributes::empty())
                    .unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();
        let usecase = RespondToCeremonyInterventionUseCase::new(
            definition_resolver(definitions),
            journal(instances),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        let instance = usecase
            .execute(RespondToCeremonyInterventionInput::new(
                ceremony_id(),
                intervention_id.clone(),
                respondent_role_id(),
                AuditActorKind::Agent,
                CeremonyInterventionContent::new("Queue depth is stable.", Attributes::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(
            instance
                .intervention(&intervention_id)
                .unwrap()
                .responses()
                .len(),
            1
        );
    }

    /// Answering is a fact about the session, distinct from asking.
    ///
    /// A request and its answer collapsing into one entry would lose
    /// the shape a reader follows: what was asked, and separately that
    /// somebody came back with something.
    #[tokio::test]
    async fn seals_the_response_apart_from_the_request() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let (journal, unit_of_work) = journal_over(instances.clone());
        let intervention_id = CeremonyInterventionId::new("ask-table").unwrap();
        RequestCeremonyInterventionUseCase::new(
            definition_resolver(definitions.clone()),
            journal.clone(),
            Arc::new(FixedClock::new(now())),
        )
        .execute(RequestCeremonyInterventionInput::new(
            ceremony_id(),
            intervention_id.clone(),
            role_id(),
            AuditActorKind::Human,
            CeremonyInterventionKind::Opinion,
            CeremonyInterventionTarget::roles([respondent_role_id()]).unwrap(),
            CeremonyInterventionContent::new("What does the table think?", Attributes::empty())
                .unwrap(),
        ))
        .await
        .unwrap();

        RespondToCeremonyInterventionUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        )
        .execute(RespondToCeremonyInterventionInput::new(
            ceremony_id(),
            intervention_id,
            respondent_role_id(),
            AuditActorKind::Agent,
            CeremonyInterventionContent::new("Queue depth is stable.", Attributes::empty())
                .unwrap(),
        ))
        .await
        .unwrap();

        let facts = unit_of_work.facts().await;
        let sealed = facts.iter().map(|fact| fact.event_type).collect::<Vec<_>>();
        assert_eq!(
            sealed,
            vec![
                AuditEventType::InterventionRequested,
                AuditEventType::InterventionResponded
            ],
            "asking and answering are two facts: {facts:?}"
        );
        // Two different parties, two different declared kinds, neither
        // deduced from the other.
        assert_eq!(facts[0].actor.kind(), AuditActorKind::Human);
        assert_eq!(facts[1].actor.kind(), AuditActorKind::Agent);
        // The answering seat is part of what identifies the fact. An
        // item put to the whole table is answered by more than one of
        // them, and keyed on the item alone the second answer would
        // derive the first one's id and be lost.
        assert!(
            facts[1]
                .event_id
                .as_str()
                .contains(respondent_role_id().as_str()),
            "the answering seat is not part of the fact's identity: {}",
            facts[1].event_id.as_str()
        );
    }
}
