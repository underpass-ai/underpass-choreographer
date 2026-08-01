//! [`AssertCeremonyReasonUseCase`] — say why one thing here led to another.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyInstanceRepositoryPort, ClockPort};

use super::assert_ceremony_reason_input::AssertCeremonyReasonInput;
use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use crate::services::SessionMemoryRecorder;

pub struct AssertCeremonyReasonUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
    clock: Arc<dyn ClockPort>,
    memory: Arc<SessionMemoryRecorder>,
}

impl std::fmt::Debug for AssertCeremonyReasonUseCase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AssertCeremonyReasonUseCase")
            .finish()
    }
}

impl AssertCeremonyReasonUseCase {
    #[must_use]
    pub fn new(
        definitions: Arc<ResolveCeremonyDefinitionUseCase>,
        instances: Arc<dyn CeremonyInstanceRepositoryPort>,
        clock: Arc<dyn ClockPort>,
        memory: Arc<SessionMemoryRecorder>,
    ) -> Self {
        Self {
            definitions,
            instances,
            clock,
            memory,
        }
    }

    #[tracing::instrument(
        name = "assert_ceremony_reason",
        skip_all,
        fields(
            ceremony_id = %input.instance_id,
            role_id = %input.role_id,
            kind = input.kind.as_label(),
        )
    )]
    pub async fn execute(
        &self,
        input: AssertCeremonyReasonInput,
    ) -> Result<CeremonyInstance, DomainError> {
        let mut instance = self.instances.get(&input.instance_id).await?;
        let definition = self.definitions.execute(&instance).await?;
        instance.assert_reason_as(
            &definition,
            input.role_id,
            input.from,
            input.to,
            input.kind,
            input.why,
            input.confidence,
            self.clock.now(),
        )?;
        self.instances.save(&instance).await?;
        // The reason a later session will follow, sent on once the
        // session that holds it is safely stored.
        self.memory
            .remember_reason(&instance, instance.reasons().len().saturating_sub(1))
            .await;
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::{
        Attributes, AuditActorKind, CeremonyInterventionContent, CeremonyInterventionId,
        CeremonyInterventionKind, CeremonyInterventionTarget, CeremonyReasonKind,
        CeremonyRecordRef, MemoryConfidence,
    };

    use super::*;
    use crate::usecases::ceremony_test_support::{
        ceremony_id, definition, definition_resolver, journal, now, recorder, recording_memory,
        respondent_role_id, role_id, started_instance, DefinitionRepositoryFake, FixedClock,
        InstanceRepositoryFake,
    };
    use crate::usecases::{
        RespondToCeremonyInterventionInput, RespondToCeremonyInterventionUseCase,
    };

    /// The whole point, end to end: a session contributes, explains
    /// itself, and memory receives the entry **and the edge**.
    ///
    /// The edge is the part that was impossible until now. Without it
    /// a later session can read what was said and never work out what
    /// made anyone say it.
    #[tokio::test]
    async fn a_reason_reaches_memory_as_an_edge_between_two_entries() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let memory = recording_memory();
        let agenda_item = CeremonyInterventionId::new("inspect-queue").unwrap();

        let mut instance = started_instance(&definition);
        for id in [
            &agenda_item,
            &CeremonyInterventionId::new("what-next").unwrap(),
        ] {
            instance
                .request_intervention_as(
                    &definition,
                    id.clone(),
                    role_id(),
                    CeremonyInterventionKind::Investigation,
                    CeremonyInterventionTarget::roles([respondent_role_id()]).unwrap(),
                    CeremonyInterventionContent::new("Look.", Attributes::empty()).unwrap(),
                    now(),
                )
                .unwrap();
        }
        instances.save(&instance).await.unwrap();

        let respond = RespondToCeremonyInterventionUseCase::new(
            definition_resolver(definitions.clone()),
            journal(instances.clone()),
            Arc::new(FixedClock::new(now())),
            recorder(memory.clone()),
        );
        let later = CeremonyInterventionId::new("what-next").unwrap();
        for (id, said) in [
            (&agenda_item, "the queue was backing up"),
            (&later, "roll back rather than restart"),
        ] {
            respond
                .execute(RespondToCeremonyInterventionInput::new(
                    ceremony_id(),
                    id.clone(),
                    respondent_role_id(),
                    AuditActorKind::Agent,
                    CeremonyInterventionContent::new(said, Attributes::empty()).unwrap(),
                ))
                .await
                .unwrap();
        }

        AssertCeremonyReasonUseCase::new(
            definition_resolver(definitions),
            instances,
            Arc::new(FixedClock::new(now())),
            recorder(memory.clone()),
        )
        .execute(AssertCeremonyReasonInput::new(
            ceremony_id(),
            respondent_role_id(),
            CeremonyRecordRef::contribution(later, 0),
            CeremonyRecordRef::contribution(agenda_item, 0),
            CeremonyReasonKind::ChosenBecause,
            "the queue growth is what made a rollback necessary",
            MemoryConfidence::High,
        ))
        .await
        .unwrap();

        let entries = memory.entries().await;
        assert_eq!(entries.len(), 2, "both contributions were remembered");

        let relations = memory.relations().await;
        let [edge] = relations.as_slice() else {
            panic!("expected exactly one edge, got {relations:?}");
        };
        assert_eq!(
            edge.why(),
            "the queue growth is what made a rollback necessary"
        );
        assert_eq!(edge.confidence(), MemoryConfidence::High);
        assert_eq!(edge.from().as_str(), "agenda:what-next:contribution:0");
        assert_eq!(edge.to().as_str(), "agenda:inspect-queue:contribution:0");
    }

    /// A reason whose end was never remembered is not sent.
    ///
    /// A step is machinery and memory keeps no kind for it, so an edge
    /// into one would claim an explanation exists and give no way to
    /// reach it.
    #[tokio::test]
    async fn a_reason_into_something_unremembered_is_not_sent() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let memory = recording_memory();
        let agenda_item = CeremonyInterventionId::new("inspect-queue").unwrap();

        let mut instance = started_instance(&definition);
        instance
            .request_intervention_as(
                &definition,
                agenda_item.clone(),
                role_id(),
                CeremonyInterventionKind::Investigation,
                CeremonyInterventionTarget::roles([respondent_role_id()]).unwrap(),
                CeremonyInterventionContent::new("Look.", Attributes::empty()).unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();

        RespondToCeremonyInterventionUseCase::new(
            definition_resolver(definitions.clone()),
            journal(instances.clone()),
            Arc::new(FixedClock::new(now())),
            recorder(memory.clone()),
        )
        .execute(RespondToCeremonyInterventionInput::new(
            ceremony_id(),
            agenda_item.clone(),
            respondent_role_id(),
            AuditActorKind::Agent,
            CeremonyInterventionContent::new("the queue was backing up", Attributes::empty())
                .unwrap(),
        ))
        .await
        .unwrap();

        AssertCeremonyReasonUseCase::new(
            definition_resolver(definitions),
            instances,
            Arc::new(FixedClock::new(now())),
            recorder(memory.clone()),
        )
        .execute(AssertCeremonyReasonInput::new(
            ceremony_id(),
            respondent_role_id(),
            CeremonyRecordRef::contribution(agenda_item, 0),
            CeremonyRecordRef::agenda_item(CeremonyInterventionId::new("inspect-queue").unwrap()),
            CeremonyReasonKind::FollowsFrom,
            "the item is where the finding came from",
            MemoryConfidence::Low,
        ))
        .await
        .unwrap();

        assert!(
            memory.relations().await.is_empty(),
            "an edge into something memory never kept was sent anyway"
        );
    }
}
