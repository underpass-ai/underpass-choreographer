//! [`DeferCeremonyGuardUseCase`] — preserve a human decision deferral.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort, ClockPort,
};

use super::DeferCeremonyGuardInput;

pub struct DeferCeremonyGuardUseCase {
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for DeferCeremonyGuardUseCase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("DeferCeremonyGuardUseCase").finish()
    }
}

impl DeferCeremonyGuardUseCase {
    #[must_use]
    pub fn new(
        definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
        instances: Arc<dyn CeremonyInstanceRepositoryPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            definitions,
            instances,
            clock,
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
        let definition = self
            .definitions
            .get(&input.definition_name, &input.definition_version)
            .await?;
        let mut instance = self.instances.get(&input.instance_id).await?;
        instance.defer_guard(
            &definition,
            input.guard_name,
            input.content,
            self.clock.now(),
        )?;
        self.instances.save(&instance).await?;
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::{CeremonyGuardDeferralContent, GuardName};

    use super::*;
    use crate::usecases::ceremony_test_support::{
        approval_definition, approval_definition_name, ceremony_id, now, started_instance, version,
        DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
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
            definitions,
            instances.clone(),
            Arc::new(FixedClock::new(now())),
        );
        let guard_name = GuardName::new("human_approved").unwrap();

        let deferred = usecase
            .execute(DeferCeremonyGuardInput::new(
                ceremony_id(),
                approval_definition_name(),
                version(),
                guard_name.clone(),
                CeremonyGuardDeferralContent::new(
                    "I do not know.",
                    "The available evidence is inconclusive.",
                    vec!["New evidence clarifies the outcome.".to_owned()],
                )
                .unwrap(),
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
