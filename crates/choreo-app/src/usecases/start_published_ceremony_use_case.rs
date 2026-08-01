//! [`StartPublishedCeremonyUseCase`] — run a published definition, and
//! record which one.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyDefinitionPublicationPort, ClockPort};

use super::start_ceremony_input::StartCeremonyInput;
use crate::services::{session_facts, SessionJournal};

pub struct StartPublishedCeremonyUseCase {
    publications: Arc<dyn CeremonyDefinitionPublicationPort>,
    journal: Arc<SessionJournal>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for StartPublishedCeremonyUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartPublishedCeremonyUseCase").finish()
    }
}

impl StartPublishedCeremonyUseCase {
    #[must_use]
    pub fn new(
        publications: Arc<dyn CeremonyDefinitionPublicationPort>,
        journal: Arc<SessionJournal>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            publications,
            journal,
            clock,
        }
    }

    /// Resolve the published version and bind the instance to its
    /// digest.
    ///
    /// Deliberately not a fallback to an unpublished definition of the
    /// same name: a caller that asked for a published version and
    /// silently received something else would be told it is governed
    /// when it is not.
    #[tracing::instrument(
        name = "start_published_ceremony",
        skip_all,
        fields(ceremony_id = %input.id)
    )]
    pub async fn execute(
        &self,
        input: StartCeremonyInput,
    ) -> Result<CeremonyInstance, DomainError> {
        // No `exists` check before storing. Asking and then storing
        // leaves a gap two concurrent starts both walk through, and the
        // second would replace the first in silence. The commit itself
        // refuses, because it expects the session to be new.
        let published = self
            .publications
            .published(&input.definition_name, &input.definition_version)
            .await?
            .ok_or(DomainError::NotFound {
                what: "published_ceremony_definition",
            })?;

        let now = self.clock.now();
        let instance = CeremonyInstance::start_bound(input.id, &published, input.context, now);
        // Built before the commit so a caller who named themselves
        // badly is refused without a session being left behind.
        let fact =
            session_facts::ceremony_started(&instance, &input.actor_id, input.actor_kind, now)?;
        self.journal
            .open(instance, vec![fact])
            .await
            .map(|session| session.instance)
    }
}
