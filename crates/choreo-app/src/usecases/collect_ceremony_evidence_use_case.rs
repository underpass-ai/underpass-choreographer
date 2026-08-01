//! [`CollectCeremonyEvidenceUseCase`] — attach host evidence to a live intervention.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyEvidenceSourcePort, ClockPort};

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use super::CollectCeremonyEvidenceInput;
use crate::services::{session_facts, SessionJournal, SessionMemoryRecorder};

pub struct CollectCeremonyEvidenceUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
    evidence_source: Arc<dyn CeremonyEvidenceSourcePort>,
    clock: Arc<dyn ClockPort>,
    memory: Arc<SessionMemoryRecorder>,
}

impl std::fmt::Debug for CollectCeremonyEvidenceUseCase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CollectCeremonyEvidenceUseCase")
            .finish()
    }
}

impl CollectCeremonyEvidenceUseCase {
    #[must_use]
    pub fn new(
        definitions: Arc<ResolveCeremonyDefinitionUseCase>,
        journal: Arc<SessionJournal>,
        evidence_source: Arc<dyn CeremonyEvidenceSourcePort>,
        clock: Arc<dyn ClockPort>,
        memory: Arc<SessionMemoryRecorder>,
    ) -> Self {
        Self {
            definitions,
            journal,
            evidence_source,
            clock,
            memory,
        }
    }

    #[tracing::instrument(
        name = "collect_ceremony_evidence",
        skip_all,
        fields(
            ceremony_id = %input.instance_id,
            intervention_id = %input.intervention_id,
            role_id = %input.role_id,
            source_id = %input.source_id,
        )
    )]
    pub async fn execute(
        &self,
        input: CollectCeremonyEvidenceInput,
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
        let source_id = input.source_id.clone();
        let request = session.instance.prepare_evidence_request_as(
            &definition,
            input.intervention_id,
            input.role_id,
            input.source_id,
            input.query,
        )?;
        let evidence_pack = self.evidence_source.collect(request.clone()).await?;
        request.ensure_matches(&evidence_pack)?;
        let now = self.clock.now();
        session.instance.respond_to_intervention_with_evidence_as(
            &definition,
            request.intervention_id(),
            request.role_id().clone(),
            evidence_pack,
            now,
        )?;
        // Both facts and the contribution land together. The pack was
        // already fetched by now, so a crash here loses the whole act
        // rather than half of it.
        let facts = session_facts::evidence_collected(
            &session.instance,
            request.intervention_id(),
            &source_id,
            request.role_id(),
            input.role_kind,
            now,
        )?;
        let instance = self.journal.commit(session, facts).await?;
        // The contribution and what backs it are remembered together,
        // because an observation whose evidence arrived separately
        // would read as a claim nobody checked.
        self.memory
            .remember_contribution(&instance, request.intervention_id())
            .await;
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use choreo_core::entities::{
        CeremonyEvidencePack, ContextItem, ContextSummary, ExternalContextBundle,
    };
    use choreo_core::ports::{
        CeremonyEvidenceRequest, CeremonyEvidenceSourcePort, CeremonyInstanceRepositoryPort,
    };
    use choreo_core::value_objects::{
        Attributes, AuditActorKind, AuditEventType, CeremonyEvidenceSourceId,
        CeremonyInterventionContent, CeremonyInterventionId, CeremonyInterventionKind,
        CeremonyInterventionTarget,
    };

    use super::*;
    use crate::usecases::ceremony_test_support::{
        a_recorder, ceremony_id, definition, definition_resolver, journal, journal_over, now,
        respondent_role_id, role_id, started_instance, DefinitionRepositoryFake, FixedClock,
        InstanceRepositoryFake,
    };

    #[derive(Debug)]
    struct EvidenceSourceFake {
        pack: CeremonyEvidencePack,
    }

    #[async_trait]
    impl CeremonyEvidenceSourcePort for EvidenceSourceFake {
        async fn collect(
            &self,
            _request: CeremonyEvidenceRequest,
        ) -> Result<CeremonyEvidencePack, DomainError> {
            Ok(self.pack.clone())
        }
    }

    fn evidence_pack() -> CeremonyEvidencePack {
        let item = ContextItem::new(
            "error-rate",
            "metric",
            "Checkout error rate",
            Some("Error rate is 18%.".to_owned()),
            Attributes::empty(),
            Vec::new(),
        )
        .unwrap();
        let bundle = ExternalContextBundle::new(
            "observability-1",
            "1.0",
            Some(ContextSummary::new("Error rate increased.", Attributes::empty()).unwrap()),
            vec![item],
            Vec::new(),
            Attributes::empty(),
        )
        .unwrap();
        CeremonyEvidencePack::new(
            CeremonyEvidenceSourceId::new("observability").unwrap(),
            bundle,
            now(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn persists_source_evidence_as_a_typed_intervention_response() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let intervention_id = CeremonyInterventionId::new("inspect-observability").unwrap();
        let mut instance = started_instance(&definition);
        instance
            .request_intervention_as(
                &definition,
                intervention_id.clone(),
                role_id(),
                CeremonyInterventionKind::Investigation,
                CeremonyInterventionTarget::roles([respondent_role_id()]).unwrap(),
                CeremonyInterventionContent::new("Inspect observability.", Attributes::empty())
                    .unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();
        let usecase = CollectCeremonyEvidenceUseCase::new(
            definition_resolver(definitions),
            journal(instances.clone()),
            Arc::new(EvidenceSourceFake {
                pack: evidence_pack(),
            }),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        let instance = usecase
            .execute(CollectCeremonyEvidenceInput::new(
                ceremony_id(),
                intervention_id.clone(),
                respondent_role_id(),
                AuditActorKind::Agent,
                CeremonyEvidenceSourceId::new("observability").unwrap(),
                CeremonyInterventionContent::new(
                    "Inspect the last five minutes.",
                    Attributes::empty(),
                )
                .unwrap(),
            ))
            .await
            .unwrap();

        let response = &instance.intervention(&intervention_id).unwrap().responses()[0];
        assert_eq!(response.content().message(), "Error rate increased.");
        assert_eq!(
            response.evidence_pack().unwrap().source_id().as_str(),
            "observability"
        );
        assert_eq!(
            instances
                .saved(&ceremony_id())
                .await
                .intervention(&intervention_id)
                .unwrap()
                .responses()
                .len(),
            1
        );
    }

    /// Answering out of a source is two things having happened.
    ///
    /// The source was consulted, and the item was answered. Sealing
    /// only the first would leave a contribution on the session that no
    /// response fact accounts for, and a reader counting what the table
    /// said would get a different number depending on how each answer
    /// happened to arrive.
    #[tokio::test]
    async fn seals_both_the_lookup_and_the_answer_it_produced() {
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
                CeremonyInterventionContent::new("Look at the queue.", Attributes::empty())
                    .unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = CollectCeremonyEvidenceUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(EvidenceSourceFake {
                pack: evidence_pack(),
            }),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        usecase
            .execute(CollectCeremonyEvidenceInput::new(
                ceremony_id(),
                intervention_id,
                respondent_role_id(),
                AuditActorKind::Agent,
                CeremonyEvidenceSourceId::new("observability").unwrap(),
                CeremonyInterventionContent::new(
                    "Inspect the last five minutes.",
                    Attributes::empty(),
                )
                .unwrap(),
            ))
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        let sealed = facts.iter().map(|fact| fact.event_type).collect::<Vec<_>>();
        assert_eq!(
            sealed,
            vec![
                AuditEventType::EvidenceCollected,
                AuditEventType::InterventionResponded
            ],
            "a lookup and the answer it produced are both facts: {facts:?}"
        );
        // The answer is sealed exactly as a plain response is, so the
        // two paths are one thing to a reader.
        assert!(
            facts[1]
                .event_id
                .as_str()
                .contains(respondent_role_id().as_str()),
            "the answer does not derive its id the way a plain response does: {}",
            facts[1].event_id.as_str()
        );
        assert!(facts
            .iter()
            .all(|fact| fact.actor.kind() == AuditActorKind::Agent));
    }
}
