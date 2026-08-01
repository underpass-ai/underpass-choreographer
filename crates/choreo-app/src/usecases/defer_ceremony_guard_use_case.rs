//! [`DeferCeremonyGuardUseCase`] — preserve a human decision deferral.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyInstanceRepositoryPort, ClockPort};
use choreo_core::value_objects::CeremonyRecordRef;

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use super::DeferCeremonyGuardInput;
use crate::services::SessionMemoryRecorder;

pub struct DeferCeremonyGuardUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
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
        name = "defer_ceremony_guard",
        skip_all,
        fields(ceremony_id = %input.instance_id, guard_name = %input.guard_name)
    )]
    pub async fn execute(
        &self,
        input: DeferCeremonyGuardInput,
    ) -> Result<CeremonyInstance, DomainError> {
        let mut instance = self.instances.get(&input.instance_id).await?;
        // Resolved from the instance, never from the request: a session
        // bound to a published version must be advanced by the very
        // definition it recorded, and one that is unbound has only the
        // repository to go to. Reading coordinates off the caller made
        // a bound session unadvanceable, because publishing writes to
        // the catalogue and not to the repository.
        let definition = self.definitions.execute(&instance).await?;
        let decided = CeremonyRecordRef::guard_decision(input.guard_name.clone());
        instance.defer_guard(
            &definition,
            input.guard_name,
            input.content,
            input.role_id,
            self.clock.now(),
        )?;
        self.instances.save(&instance).await?;
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

    use super::*;
    use crate::usecases::ceremony_test_support::{
        a_recorder, approval_definition, ceremony_id, definition_resolver, now, role_id,
        started_instance, DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
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
            instances.clone(),
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
}
