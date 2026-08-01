//! [`ApplyCeremonyTransitionUseCase`] — apply a guarded ceremony transition.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::ClockPort;

use super::apply_ceremony_transition_input::ApplyCeremonyTransitionInput;
use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use crate::services::{session_facts, SessionJournal, SessionMemoryRecorder};

pub struct ApplyCeremonyTransitionUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
    memory: Arc<SessionMemoryRecorder>,
}

impl std::fmt::Debug for ApplyCeremonyTransitionUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApplyCeremonyTransitionUseCase").finish()
    }
}

impl ApplyCeremonyTransitionUseCase {
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
        name = "apply_ceremony_transition",
        skip_all,
        fields(ceremony_id = %input.instance_id, trigger = %input.trigger)
    )]
    pub async fn execute(
        &self,
        input: ApplyCeremonyTransitionInput,
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
        session
            .instance
            .apply_transition_as(&definition, &input.role_id, &input.trigger, now)?;
        let facts = session_facts::transition_applied(
            &session.instance,
            &definition,
            &input.role_id,
            input.role_kind,
            now,
        )?;
        // The move and the state it moved to land together. Apart, a
        // crash between them leaves a session sitting in a state no
        // recorded move can account for.
        let instance = self.journal.commit(session, facts).await?;
        // A transition is how a session reaches its end, so this is
        // where an ending becomes something a later session can weigh.
        // Nothing is written while it is still running.
        self.memory.remember_ending(&instance, &definition).await;
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use choreo_core::error::DomainError;
    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::{
        AuditActorKind, AuditEventType, StateId, StepOutput, StepResult,
    };

    use super::*;
    use crate::usecases::ceremony_test_support::{
        a_recorder, ceremony_id, definition, definition_resolver, journal,
        journal_losing_every_race, journal_over, now, role_id, started_instance, step_id, trigger,
        DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
    };

    #[tokio::test]
    async fn applies_guarded_transition_after_step_completion() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let mut instance = started_instance(&definition);
        instance
            .start_step(
                &definition,
                &step_id(),
                choreo_core::value_objects::StepLease::new(
                    crate::usecases::ceremony_test_support::lease_owner(),
                    crate::usecases::ceremony_test_support::idempotency_key("lease-1"),
                    now(),
                    now() + time::Duration::seconds(60),
                )
                .unwrap(),
                now(),
            )
            .unwrap();
        instance
            .apply_step_result(
                &definition,
                &step_id(),
                StepResult::completed(StepOutput::empty()).unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();
        let usecase = ApplyCeremonyTransitionUseCase::new(
            definition_resolver(definitions),
            journal(instances.clone()),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        let transitioned = usecase
            .execute(ApplyCeremonyTransitionInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
                trigger(),
            ))
            .await
            .unwrap();

        assert_eq!(
            transitioned.current_state(),
            &StateId::new("COMPLETED").unwrap()
        );
        assert!(instances
            .saved(&ceremony_id())
            .await
            .is_completed(&definition));
    }

    #[tokio::test]
    async fn unsatisfied_guard_is_rejected() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let usecase = ApplyCeremonyTransitionUseCase::new(
            definition_resolver(definitions),
            journal(instances),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        let err = usecase
            .execute(ApplyCeremonyTransitionInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
                trigger(),
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    /// A session ready to make its last move.
    async fn ready_to_finish() -> (Arc<DefinitionRepositoryFake>, Arc<InstanceRepositoryFake>) {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        let mut instance = started_instance(&definition);
        instance
            .start_step(
                &definition,
                &step_id(),
                choreo_core::value_objects::StepLease::new(
                    crate::usecases::ceremony_test_support::lease_owner(),
                    crate::usecases::ceremony_test_support::idempotency_key("lease-1"),
                    now(),
                    now() + time::Duration::seconds(60),
                )
                .unwrap(),
                now(),
            )
            .unwrap();
        instance
            .apply_step_result(
                &definition,
                &step_id(),
                StepResult::completed(StepOutput::empty()).unwrap(),
                now(),
            )
            .unwrap();
        instances.save(&instance).await.unwrap();
        (definitions, instances)
    }

    /// The move and the end it reached are two facts, not one.
    ///
    /// A reader asking whether this session finished should find that
    /// answered outright. Left to be worked out from the state the last
    /// move landed in, the answer depends on holding the definition
    /// too, which the journal does not carry.
    #[tokio::test]
    async fn seals_the_move_and_the_end_it_reached() {
        let (definitions, instances) = ready_to_finish().await;
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = ApplyCeremonyTransitionUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        usecase
            .execute(ApplyCeremonyTransitionInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
                trigger(),
            ))
            .await
            .unwrap();

        let facts = unit_of_work.facts().await;
        let sealed = facts.iter().map(|fact| fact.event_type).collect::<Vec<_>>();
        assert_eq!(
            sealed,
            vec![
                AuditEventType::TransitionApplied,
                AuditEventType::CeremonyCompleted
            ],
            "the move and the ending must both be sealed: {facts:?}"
        );
        // Declared by the caller, carried through untouched. The seat
        // came from the definition; what filled it did not.
        assert!(facts
            .iter()
            .all(|fact| fact.actor.kind() == AuditActorKind::Agent));
    }

    /// A move that was refused leaves nothing behind.
    ///
    /// The whole point of committing state and facts together: a
    /// rejected transition must not seal a fact saying it happened, and
    /// a sealed fact must not survive a transition that did not.
    #[tokio::test]
    async fn a_refused_move_seals_nothing() {
        let definition = definition();
        let definitions = Arc::new(DefinitionRepositoryFake::new(definition.clone()));
        let instances = Arc::new(InstanceRepositoryFake::default());
        instances
            .save(&started_instance(&definition))
            .await
            .unwrap();
        let (journal, unit_of_work) = journal_over(instances);
        let usecase = ApplyCeremonyTransitionUseCase::new(
            definition_resolver(definitions),
            journal,
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        usecase
            .execute(ApplyCeremonyTransitionInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
                trigger(),
            ))
            .await
            .unwrap_err();

        assert!(
            unit_of_work.facts().await.is_empty(),
            "an unsatisfied guard sealed a fact"
        );
    }

    /// The same race the guards refuse, refused here too.
    ///
    /// Swap the two reads in `SessionJournal::load` and this fails,
    /// because the move then lands over the other writer's.
    #[tokio::test]
    async fn refuses_to_move_a_session_someone_else_moved_on() {
        let (definitions, instances) = ready_to_finish().await;
        let usecase = ApplyCeremonyTransitionUseCase::new(
            definition_resolver(definitions),
            journal_losing_every_race(instances),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        let refused = usecase
            .execute(ApplyCeremonyTransitionInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
                trigger(),
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

    /// The assumption `session_facts` rests on.
    ///
    /// `AuditEventType::CeremonyFailed` has no producer, because a
    /// session reaches a terminal state only by moving into one and
    /// that always stamps it completed. This test is where that stops
    /// being true: an ending that is not a completion breaks it, and
    /// whoever adds one is then pointed at the two places — here and
    /// `session_memory_projection::ending_entry` — that already have a
    /// branch waiting for it.
    #[tokio::test]
    async fn a_terminal_session_is_always_a_finished_one() {
        let (definitions, instances) = ready_to_finish().await;
        let definition = definition();
        let usecase = ApplyCeremonyTransitionUseCase::new(
            definition_resolver(definitions),
            journal(instances),
            Arc::new(FixedClock::new(now())),
            a_recorder(),
        );

        let ended = usecase
            .execute(ApplyCeremonyTransitionInput::new(
                ceremony_id(),
                role_id(),
                AuditActorKind::Agent,
                trigger(),
            ))
            .await
            .unwrap();

        assert!(ended.is_terminal(&definition));
        assert!(
            ended.is_completed(&definition),
            "a terminal session that is not completed now exists, and the audit cannot say so"
        );
    }
}
