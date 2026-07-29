//! In-memory [`CeremonyTranscriptStorePort`] implementation.
//!
//! The generic, product-agnostic context store: contributions live in a
//! lock-guarded map keyed by ceremony instance. A deployment that needs
//! durable or shared context — for example one backed by the Underpass
//! knowledge plane — swaps this for another adapter behind the same port,
//! without the domain changing.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::CeremonyTranscriptStorePort;
use choreo_core::value_objects::{CeremonyId, CeremonyStepContribution, CeremonyTranscript};
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
pub struct InMemoryCeremonyTranscriptStore {
    inner: Arc<RwLock<BTreeMap<CeremonyId, Vec<CeremonyStepContribution>>>>,
}

impl InMemoryCeremonyTranscriptStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CeremonyTranscriptStorePort for InMemoryCeremonyTranscriptStore {
    async fn append(
        &self,
        instance_id: &CeremonyId,
        contribution: CeremonyStepContribution,
    ) -> Result<(), DomainError> {
        self.inner
            .write()
            .await
            .entry(instance_id.clone())
            .or_default()
            .push(contribution);
        Ok(())
    }

    async fn transcript(
        &self,
        instance_id: &CeremonyId,
    ) -> Result<CeremonyTranscript, DomainError> {
        Ok(CeremonyTranscript::new(
            self.inner
                .read()
                .await
                .get(instance_id)
                .cloned()
                .unwrap_or_default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use choreo_core::value_objects::{RoleId, StepId, StepOutput};

    use super::*;

    fn contribution(step: &str, role: &str) -> CeremonyStepContribution {
        CeremonyStepContribution::new(
            StepId::new(step).unwrap(),
            RoleId::new(role).unwrap(),
            StepOutput::empty(),
        )
    }

    #[tokio::test]
    async fn unknown_instance_has_empty_transcript() {
        let store = InMemoryCeremonyTranscriptStore::new();

        let transcript = store
            .transcript(&CeremonyId::new("ceremony-1").unwrap())
            .await
            .unwrap();

        assert!(transcript.is_empty());
    }

    #[tokio::test]
    async fn appends_accumulate_in_order_per_instance() {
        let store = InMemoryCeremonyTranscriptStore::new();
        let instance = CeremonyId::new("ceremony-1").unwrap();

        store
            .append(&instance, contribution("open_room", "FACILITATOR"))
            .await
            .unwrap();
        store
            .append(&instance, contribution("risk_check", "RISK_REVIEWER"))
            .await
            .unwrap();

        let transcript = store.transcript(&instance).await.unwrap();
        assert_eq!(transcript.len(), 2);
        assert_eq!(
            transcript.contributions()[0].step_id().as_str(),
            "open_room"
        );
        assert_eq!(
            transcript.contributions()[1].step_id().as_str(),
            "risk_check"
        );
    }

    #[tokio::test]
    async fn transcripts_are_isolated_per_instance() {
        let store = InMemoryCeremonyTranscriptStore::new();
        let first = CeremonyId::new("ceremony-1").unwrap();
        let second = CeremonyId::new("ceremony-2").unwrap();

        store
            .append(&first, contribution("open_room", "FACILITATOR"))
            .await
            .unwrap();

        assert_eq!(store.transcript(&first).await.unwrap().len(), 1);
        assert!(store.transcript(&second).await.unwrap().is_empty());
    }
}
