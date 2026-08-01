//! [`ApproveCeremonyGuardUseCase`] — record a human guard approval.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyInstanceRepositoryPort, ClockPort};
use choreo_core::value_objects::CeremonyRecordRef;

use super::approve_ceremony_guard_input::ApproveCeremonyGuardInput;
use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use crate::services::SessionMemoryRecorder;

pub struct ApproveCeremonyGuardUseCase {
    definitions: Arc<ResolveCeremonyDefinitionUseCase>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
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
        let mut instance = self.instances.get(&input.instance_id).await?;
        let definition = self.definitions.execute(&instance).await?;
        instance.approve_guard(
            &definition,
            &input.guard_name,
            input.role_id,
            input.role_kind,
            self.clock.now(),
        )?;
        self.instances.save(&instance).await?;
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

    use choreo_core::value_objects::AuditActorKind;

    use super::*;
    use crate::usecases::ceremony_test_support::{
        a_recorder, approval_definition, ceremony_id, definition_resolver, now, role_id,
        started_instance, DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
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
            instances.clone(),
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
}
