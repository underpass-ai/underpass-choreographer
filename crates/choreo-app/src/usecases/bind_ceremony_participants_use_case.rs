//! [`BindCeremonyParticipantsUseCase`] — who sits at this table.
//!
//! A definition says what each role does. Seating says who is doing
//! it in this session, and it is per session on purpose: the same
//! ceremony run twice can and should be able to seat different people.

use std::collections::BTreeMap;
use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::ClockPort;
use choreo_core::value_objects::{AuditActorKind, CeremonyId, RoleId, Specialty};

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use crate::services::{session_facts, SessionJournal};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindCeremonyParticipantsInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) seating: BTreeMap<RoleId, Specialty>,
    /// Who is seating them, in the caller's own terms.
    ///
    /// Not a role from the definition: seating the table is done to a
    /// session rather than in it, and whoever does it need hold no
    /// seat at all.
    pub(crate) actor_id: String,
    /// What kind of party that is.
    ///
    /// Carried, never worked out.
    pub(crate) actor_kind: AuditActorKind,
}

impl BindCeremonyParticipantsInput {
    /// Seating with nobody in it is a call that would change nothing,
    /// and answering "done" to it would be a small lie.
    pub fn new(
        instance_id: CeremonyId,
        seating: impl IntoIterator<Item = (RoleId, Specialty)>,
        actor_id: impl Into<String>,
        actor_kind: AuditActorKind,
    ) -> Result<Self, DomainError> {
        let seating: BTreeMap<RoleId, Specialty> = seating.into_iter().collect();
        if seating.is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "ceremony_participant_binding.seating",
            });
        }
        Ok(Self {
            instance_id,
            seating,
            actor_id: actor_id.into(),
            actor_kind,
        })
    }

    #[must_use]
    pub fn instance_id(&self) -> &CeremonyId {
        &self.instance_id
    }

    #[must_use]
    pub fn seating(&self) -> &BTreeMap<RoleId, Specialty> {
        &self.seating
    }
}

pub struct BindCeremonyParticipantsUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for BindCeremonyParticipantsUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BindCeremonyParticipantsUseCase").finish()
    }
}

impl BindCeremonyParticipantsUseCase {
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
        name = "bind_ceremony_participants",
        skip_all,
        fields(ceremony_id = %input.instance_id)
    )]
    pub async fn execute(
        &self,
        input: BindCeremonyParticipantsInput,
    ) -> Result<CeremonyInstance, DomainError> {
        // Loaded through the journal so the revision is read before
        // the session: the other order lets a concurrent write turn a
        // race into a silent overwrite.
        let mut session = self.journal.load(&input.instance_id).await?;
        let definition = self.definitions.execute(&session.instance).await?;
        let now = self.clock.now();

        // All of it or none of it: a seat the ceremony never declared
        // stops the call before anything is saved. A caller seating
        // three roles and getting two would have to work out which,
        // and a half-seated table is not something anyone asked for.
        for (role_id, specialty) in &input.seating {
            session.instance.bind_participant(
                &definition,
                role_id.clone(),
                specialty.clone(),
                now,
            )?;
        }
        // The seating and the record of somebody having done it land
        // together, for the same reason the loop is all-or-nothing.
        let fact = session_facts::participants_bound(
            &session.instance,
            &input.seating,
            &input.actor_id,
            input.actor_kind,
            now,
        )?;
        self.journal
            .commit(session, vec![fact])
            .await
            .map(|session| session.instance)
    }
}

#[cfg(test)]
mod tests {
    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::AuditEventType;

    use super::*;
    use crate::usecases::ceremony_test_support::{
        ceremony_id, definition, definition_resolver, journal_over, now, role_id, started_instance,
        DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
    };

    async fn seated() -> (Arc<DefinitionRepositoryFake>, Arc<InstanceRepositoryFake>) {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        (definitions, instances)
    }

    /// Seating the table is a fact about the session, and the seater
    /// holds no seat in it.
    #[tokio::test]
    async fn seals_the_seating_into_the_journal() {
        let (definitions, instances) = seated().await;
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = BindCeremonyParticipantsUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
        );

        usecase
            .execute(
                BindCeremonyParticipantsInput::new(
                    ceremony_id(),
                    [(role_id(), Specialty::new("reviewer").unwrap())],
                    "operator-1",
                    AuditActorKind::Human,
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        assert_eq!(facts.len(), 1, "one seating, one fact: {facts:?}");
        assert_eq!(facts[0].event_type, AuditEventType::ParticipantsBound);
        assert_eq!(facts[0].actor.kind(), AuditActorKind::Human);
        assert!(
            facts[0].actor.role_id().is_none(),
            "the seater was given a seat this ceremony never assigned"
        );
    }

    /// Seating the same role somewhere else is a second fact.
    ///
    /// The id is the seating itself, so this is the case that decides
    /// whether the scheme works: a role moved to another specialty must
    /// not derive the id of where it used to sit.
    #[tokio::test]
    async fn re_seating_a_role_elsewhere_is_a_distinct_fact() {
        let (definitions, instances) = seated().await;
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = BindCeremonyParticipantsUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
        );

        for specialty in ["reviewer", "auditor"] {
            usecase
                .execute(
                    BindCeremonyParticipantsInput::new(
                        ceremony_id(),
                        [(role_id(), Specialty::new(specialty).unwrap())],
                        "operator-1",
                        AuditActorKind::Human,
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
        }

        let ids = unit_of_work
            .facts()
            .await
            .iter()
            .map(|fact| fact.event_id.as_str().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            ids.len(),
            2,
            "moving a role to another specialty derived the id of where it used to sit: {ids:?}"
        );
    }
}
