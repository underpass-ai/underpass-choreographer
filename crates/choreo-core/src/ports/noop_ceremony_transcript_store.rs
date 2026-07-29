use async_trait::async_trait;

use crate::error::DomainError;
use crate::value_objects::{CeremonyId, CeremonyStepContribution, CeremonyTranscript};

use super::CeremonyTranscriptStorePort;

/// Context store used when a caller does not opt into transcript persistence.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopCeremonyTranscriptStore;

#[async_trait]
impl CeremonyTranscriptStorePort for NoopCeremonyTranscriptStore {
    async fn append(
        &self,
        _instance_id: &CeremonyId,
        _contribution: CeremonyStepContribution,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn transcript(
        &self,
        _instance_id: &CeremonyId,
    ) -> Result<CeremonyTranscript, DomainError> {
        Ok(CeremonyTranscript::empty())
    }
}

#[cfg(test)]
mod tests {
    use crate::value_objects::{RoleId, StepId, StepOutput};

    use super::*;

    #[tokio::test]
    async fn accepts_contributions_and_always_returns_an_empty_transcript() {
        let store = NoopCeremonyTranscriptStore;
        let ceremony_id = CeremonyId::new("ceremony-1").unwrap();
        let contribution = CeremonyStepContribution::new(
            StepId::new("step_1").unwrap(),
            RoleId::new("ROLE").unwrap(),
            StepOutput::empty(),
        );

        store.append(&ceremony_id, contribution).await.unwrap();

        assert!(store.transcript(&ceremony_id).await.unwrap().is_empty());
    }
}
