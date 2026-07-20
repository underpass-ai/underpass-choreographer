//! [`RespondToCeremonyInterventionUseCase`] — contribute to a live agenda item.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort, ClockPort,
};

use super::RespondToCeremonyInterventionInput;

pub struct RespondToCeremonyInterventionUseCase {
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
    clock: Arc<dyn ClockPort>,
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
        let definition = self
            .definitions
            .get(&input.definition_name, &input.definition_version)
            .await?;
        let mut instance = self.instances.get(&input.instance_id).await?;
        instance.respond_to_intervention_as(
            &definition,
            &input.intervention_id,
            input.role_id,
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
    use choreo_core::value_objects::{
        Attributes, CeremonyInterventionContent, CeremonyInterventionId, CeremonyInterventionKind,
        CeremonyInterventionTarget,
    };

    use super::*;
    use crate::usecases::ceremony_test_support::{
        ceremony_id, definition, definition_name, now, respondent_role_id, role_id,
        started_instance, version, DefinitionRepositoryFake, FixedClock, InstanceRepositoryFake,
    };

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
            definitions,
            instances,
            Arc::new(FixedClock::new(now())),
        );

        let instance = usecase
            .execute(RespondToCeremonyInterventionInput::new(
                ceremony_id(),
                definition_name(),
                version(),
                intervention_id.clone(),
                respondent_role_id(),
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
}
