//! [`BindCeremonyParticipantsUseCase`] — who sits at this table.
//!
//! A definition says what each role does. Seating says who is doing
//! it in this session, and it is per session on purpose: the same
//! ceremony run twice can and should be able to seat different people.

use std::collections::BTreeMap;
use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyInstanceRepositoryPort, ClockPort};
use choreo_core::value_objects::{CeremonyId, RoleId, Specialty};

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindCeremonyParticipantsInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) seating: BTreeMap<RoleId, Specialty>,
}

impl BindCeremonyParticipantsInput {
    /// Seating with nobody in it is a call that would change nothing,
    /// and answering "done" to it would be a small lie.
    pub fn new(
        instance_id: CeremonyId,
        seating: impl IntoIterator<Item = (RoleId, Specialty)>,
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
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
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
        name = "bind_ceremony_participants",
        skip_all,
        fields(ceremony_id = %input.instance_id)
    )]
    pub async fn execute(
        &self,
        input: BindCeremonyParticipantsInput,
    ) -> Result<CeremonyInstance, DomainError> {
        let mut instance = self.instances.get(&input.instance_id).await?;
        let definition = self.definitions.execute(&instance).await?;
        let now = self.clock.now();

        // All of it or none of it: a seat the ceremony never declared
        // stops the call before anything is saved. A caller seating
        // three roles and getting two would have to work out which,
        // and a half-seated table is not something anyone asked for.
        for (role_id, specialty) in &input.seating {
            instance.bind_participant(&definition, role_id.clone(), specialty.clone(), now)?;
        }
        self.instances.save(&instance).await?;
        Ok(instance)
    }
}
