//! In-memory [`DeliberationRepositoryPort`] backed by a `RwLock<BTreeMap>`.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::entities::Deliberation;
use choreo_core::error::DomainError;
use choreo_core::ports::DeliberationRepositoryPort;
use choreo_core::value_objects::TaskId;
use tokio::sync::RwLock;

/// In-memory deliberation repository keyed by [`TaskId`].
///
/// Insertions keep the latest persisted value per task id.
#[derive(Debug, Default, Clone)]
pub struct InMemoryDeliberationRepository {
    inner: Arc<RwLock<BTreeMap<TaskId, Deliberation>>>,
}

impl InMemoryDeliberationRepository {
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
impl DeliberationRepositoryPort for InMemoryDeliberationRepository {
    async fn save(&self, deliberation: &Deliberation) -> Result<(), DomainError> {
        self.inner
            .write()
            .await
            .insert(deliberation.task_id().clone(), deliberation.clone());
        Ok(())
    }

    async fn get(&self, task_id: &TaskId) -> Result<Deliberation, DomainError> {
        self.inner
            .read()
            .await
            .get(task_id)
            .cloned()
            .ok_or(DomainError::NotFound {
                what: "deliberation",
            })
    }

    async fn exists(&self, task_id: &TaskId) -> Result<bool, DomainError> {
        Ok(self.inner.read().await.contains_key(task_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{Rounds, Specialty};
    use time::macros::datetime;

    fn deliberation(task: &str) -> Deliberation {
        Deliberation::start(
            TaskId::new(task).unwrap(),
            Specialty::new("triage").unwrap(),
            Rounds::default(),
            datetime!(2026-04-15 12:00:00 UTC),
        )
    }

    #[tokio::test]
    async fn save_then_get_roundtrips() {
        let repo = InMemoryDeliberationRepository::new();
        repo.save(&deliberation("t1")).await.unwrap();
        let got = repo.get(&TaskId::new("t1").unwrap()).await.unwrap();
        assert_eq!(got.task_id().as_str(), "t1");
    }

    #[tokio::test]
    async fn missing_task_returns_not_found() {
        let repo = InMemoryDeliberationRepository::new();
        let err = repo
            .get(&TaskId::new("missing").unwrap())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "deliberation"
            }
        ));
    }

    #[tokio::test]
    async fn save_overwrites_previous_version() {
        let repo = InMemoryDeliberationRepository::new();
        repo.save(&deliberation("t1")).await.unwrap();
        repo.save(&deliberation("t1")).await.unwrap();
        assert_eq!(repo.len().await, 1);
    }

    #[tokio::test]
    async fn exists_reflects_presence() {
        let repo = InMemoryDeliberationRepository::new();
        let id = TaskId::new("t1").unwrap();
        assert!(!repo.exists(&id).await.unwrap());
        repo.save(&deliberation("t1")).await.unwrap();
        assert!(repo.exists(&id).await.unwrap());
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let a = InMemoryDeliberationRepository::new();
        let b = a.clone();
        a.save(&deliberation("t1")).await.unwrap();
        assert_eq!(b.len().await, 1);
    }
}
