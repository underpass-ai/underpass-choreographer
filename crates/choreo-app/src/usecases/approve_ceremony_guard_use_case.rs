//! [`ApproveCeremonyGuardUseCase`] — record a human guard approval.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::ClockPort;
use choreo_core::value_objects::CeremonyRecordRef;

use super::approve_ceremony_guard_input::ApproveCeremonyGuardInput;
use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use crate::services::{session_facts, SessionJournal, SessionMemoryRecorder};

pub struct ApproveCeremonyGuardUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
    memory: Arc<SessionMemoryRecorder>,
}

impl std::fmt::Debug for ApproveCeremonyGuardUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApproveCeremonyGuardUseCase").finish()
    }
}

impl ApproveCeremonyGuardUseCase {
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
        name = "approve_ceremony_guard",
        skip_all,
        fields(
            ceremony_id = %input.instance_id,
            guard_name = %input.guard_name,
            role_id = %input.role_id,
        )
    )]
    pub async fn execute(
        &self,
        input: ApproveCeremonyGuardInput,
    ) -> Result<CeremonyInstance, DomainError> {
        // Loaded through the journal so the revision is read before
        // the session: the other order lets a concurrent write turn a
        // race into a silent overwrite.
        let mut session = self.journal.load(&input.instance_id).await?;
        let definition = self.definitions.execute(&session.instance).await?;
        let now = self.clock.now();
        session.instance.approve_guard(
            &definition,
            &input.guard_name,
            input.role_id.clone(),
            input.role_kind,
            now,
        )?;
        let fact = session_facts::guard_approved(
            &session.instance,
            &input.guard_name,
            &input.role_id,
            input.role_kind,
            now,
        )?;
        // The session and the record of a human having decided land
        // together. Apart, a crash between them leaves an approval
        // nobody can show was ever made.
        let instance = self.journal.commit(session, vec![fact]).await?.instance;
        // A human decision is the kind a later session weighs hardest,
        // and now it can say who made it.
        self.memory
            .remember_guard_decision(
                &instance,
                &CeremonyRecordRef::guard_decision(input.guard_name.clone()),
            )
            .await;
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::GuardName;

    use choreo_core::error::DomainError;
    use choreo_core::value_objects::{AuditActorKind, AuditEventType};

    use super::*;
    use crate::usecases::ceremony_test_support::{
        a_recorder, approval_definition, ceremony_id, definition_resolver, journal,
        journal_losing_every_race, journal_over, now, role_id, started_instance,
        DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
    };

    #[tokio::test]
    async fn records_human_guard_approval_in_context() {
        let definition = approval_definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let usecase = ApproveCeremonyGuardUseCase::new(
            definition_resolver(definitions),
            journal(instances.clone()),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );
        let guard_name = GuardName::new("human_approved").unwrap();

        let approved = usecase
            .execute(ApproveCeremonyGuardInput::new(
                ceremony_id(),
                guard_name.clone(),
                role_id(),
                AuditActorKind::Human,
            ))
            .await
            .unwrap();

        assert!(approved.context().is_guard_approved(&guard_name));
        assert!(instances
            .saved(&ceremony_id())
            .await
            .context()
            .is_guard_approved(&guard_name));
    }

    /// An approval that lost a race must say so, not win it quietly.
    ///
    /// The failure being excluded is `Ok`. A silent overwrite returns
    /// the approved session and looks like success to everyone, so the
    /// only observable difference between the safe implementation and
    /// the dangerous one is whether this call is refused.
    ///
    /// What makes the refusal happen is the order the journal reads
    /// in, and the fake is built to tell the two orders apart: swap
    /// the two reads in `SessionJournal::load` and this test fails,
    /// because the approval then succeeds over the other writer.
    #[tokio::test]
    async fn refuses_to_approve_over_a_session_someone_else_moved_on() {
        let definition = approval_definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let usecase = ApproveCeremonyGuardUseCase::new(
            definition_resolver(definitions),
            journal_losing_every_race(instances.clone()),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        let refused = usecase
            .execute(ApproveCeremonyGuardInput::new(
                ceremony_id(),
                GuardName::new("human_approved").unwrap(),
                role_id(),
                AuditActorKind::Human,
            ))
            .await;

        assert!(
            matches!(
                refused,
                Err(DomainError::Conflict {
                    what: "ceremony_instance"
                })
            ),
            "a lost race must be reported as one, got {refused:?}"
        );
    }

    /// The approval and the record of it land together.
    ///
    /// A guard that opens without leaving a fact behind is the failure
    /// the unit of work exists to prevent, and it is invisible from the
    /// state alone — the session looks exactly the same either way.
    #[tokio::test]
    async fn seals_the_approval_into_the_journal() {
        let definition = approval_definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let (journal, unit_of_work) = journal_over(instances.clone());
        let usecase = ApproveCeremonyGuardUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        usecase
            .execute(ApproveCeremonyGuardInput::new(
                ceremony_id(),
                GuardName::new("human_approved").unwrap(),
                role_id(),
                AuditActorKind::Human,
            ))
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        assert_eq!(facts.len(), 1, "one approval, one fact: {facts:?}");
        assert_eq!(facts[0].event_type, AuditEventType::HumanApprovalRecorded);
        assert_eq!(facts[0].actor.kind(), AuditActorKind::Human);
    }
}
