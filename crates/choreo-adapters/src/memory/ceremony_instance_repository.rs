//! In-memory [`CeremonyInstanceRepositoryPort`] implementation.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::CeremonyInstanceRepositoryPort;
use choreo_core::value_objects::CeremonyId;
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
pub struct InMemoryCeremonyInstanceRepository {
    inner: Arc<RwLock<BTreeMap<CeremonyId, CeremonyInstance>>>,
}

impl InMemoryCeremonyInstanceRepository {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}

#[async_trait]
impl CeremonyInstanceRepositoryPort for InMemoryCeremonyInstanceRepository {
    async fn save(&self, instance: &CeremonyInstance) -> Result<(), DomainError> {
        self.inner
            .write()
            .await
            .insert(instance.id().clone(), instance.clone());
        Ok(())
    }

    async fn get(&self, id: &CeremonyId) -> Result<CeremonyInstance, DomainError> {
        self.inner
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or(DomainError::NotFound {
                what: "ceremony_instance",
            })
    }

    async fn list(&self) -> Result<Vec<CeremonyInstance>, DomainError> {
        Ok(self.inner.read().await.values().cloned().collect())
    }

    async fn exists(&self, id: &CeremonyId) -> Result<bool, DomainError> {
        Ok(self.inner.read().await.contains_key(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::entities::CeremonyDefinition;
    use choreo_core::value_objects::{
        CeremonyContext, CeremonyName, CeremonyState, CeremonyVersion, StateId,
    };
    use time::macros::datetime;

    fn definition() -> CeremonyDefinition {
        CeremonyDefinition::new(
            CeremonyName::new("planning_ceremony").unwrap(),
            CeremonyVersion::v1(),
            None,
            Vec::new(),
            Vec::new(),
            vec![CeremonyState::initial(StateId::new("STARTED").unwrap())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    }

    fn instance(id: &str) -> CeremonyInstance {
        CeremonyInstance::start(
            CeremonyId::new(id).unwrap(),
            &definition(),
            CeremonyContext::empty(),
            datetime!(2026-06-06 12:00:00 UTC),
        )
    }

    #[tokio::test]
    async fn save_then_get_roundtrips() {
        let repository = InMemoryCeremonyInstanceRepository::new();

        repository.save(&instance("ceremony-1")).await.unwrap();
        let got = repository
            .get(&CeremonyId::new("ceremony-1").unwrap())
            .await
            .unwrap();

        assert_eq!(got.id().as_str(), "ceremony-1");
    }

    #[tokio::test]
    async fn save_overwrites_same_instance_id() {
        let repository = InMemoryCeremonyInstanceRepository::new();

        repository.save(&instance("ceremony-1")).await.unwrap();
        repository.save(&instance("ceremony-1")).await.unwrap();

        assert_eq!(repository.len().await, 1);
        assert!(!repository.is_empty().await);
    }

    #[tokio::test]
    async fn list_returns_instances_in_stable_id_order() {
        let repository = InMemoryCeremonyInstanceRepository::new();

        repository.save(&instance("ceremony-2")).await.unwrap();
        repository.save(&instance("ceremony-1")).await.unwrap();

        let ids = repository
            .list()
            .await
            .unwrap()
            .into_iter()
            .map(|item| item.id().as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["ceremony-1", "ceremony-2"]);
    }

    #[tokio::test]
    async fn missing_instance_returns_not_found() {
        let repository = InMemoryCeremonyInstanceRepository::new();

        let err = repository
            .get(&CeremonyId::new("missing").unwrap())
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "ceremony_instance"
            }
        ));
    }

    #[tokio::test]
    async fn exists_reflects_presence() {
        let repository = InMemoryCeremonyInstanceRepository::new();
        let id = CeremonyId::new("ceremony-1").unwrap();

        assert!(!repository.exists(&id).await.unwrap());
        repository.save(&instance("ceremony-1")).await.unwrap();
        assert!(repository.exists(&id).await.unwrap());
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let repository = InMemoryCeremonyInstanceRepository::new();
        let clone = repository.clone();

        repository.save(&instance("ceremony-1")).await.unwrap();

        assert_eq!(clone.len().await, 1);
    }
}
