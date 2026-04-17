//! In-memory [`StatisticsPort`] backed by a `RwLock<Statistics>`.
//!
//! Cheap to clone; internal state is shared through `Arc<RwLock>`.
//! For multi-replica deployments, a persistent adapter (e.g. Redis
//! / Neo4j / Postgres) drops in without touching the application
//! layer — the port is the contract.

use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::entities::Statistics;
use choreo_core::error::DomainError;
use choreo_core::ports::StatisticsPort;
use choreo_core::value_objects::{DurationMs, Specialty};
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
pub struct InMemoryStatistics {
    inner: Arc<RwLock<Statistics>>,
}

impl InMemoryStatistics {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl StatisticsPort for InMemoryStatistics {
    async fn record_deliberation(
        &self,
        specialty: &Specialty,
        duration: DurationMs,
    ) -> Result<(), DomainError> {
        self.inner
            .write()
            .await
            .record_deliberation(specialty, duration);
        Ok(())
    }

    async fn record_orchestration(&self, duration: DurationMs) -> Result<(), DomainError> {
        self.inner.write().await.record_orchestration(duration);
        Ok(())
    }

    async fn snapshot(&self) -> Result<Statistics, DomainError> {
        Ok(self.inner.read().await.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(s: &str) -> Specialty {
        Specialty::new(s).unwrap()
    }

    #[tokio::test]
    async fn fresh_snapshot_is_empty() {
        let stats = InMemoryStatistics::new();
        let snap = stats.snapshot().await.unwrap();
        assert_eq!(snap.total_deliberations(), 0);
        assert_eq!(snap.total_orchestrations(), 0);
        assert_eq!(snap.total_duration(), DurationMs::ZERO);
        assert!(snap.per_specialty().is_empty());
    }

    #[tokio::test]
    async fn record_deliberation_advances_counters() {
        let stats = InMemoryStatistics::new();
        stats
            .record_deliberation(&sp("triage"), DurationMs::from_millis(100))
            .await
            .unwrap();
        stats
            .record_deliberation(&sp("triage"), DurationMs::from_millis(50))
            .await
            .unwrap();
        stats
            .record_deliberation(&sp("reviewer"), DurationMs::from_millis(200))
            .await
            .unwrap();

        let snap = stats.snapshot().await.unwrap();
        assert_eq!(snap.total_deliberations(), 3);
        assert_eq!(snap.total_duration(), DurationMs::from_millis(350));
        assert_eq!(snap.per_specialty().get(&sp("triage")).copied(), Some(2));
        assert_eq!(snap.per_specialty().get(&sp("reviewer")).copied(), Some(1));
    }

    #[tokio::test]
    async fn record_orchestration_advances_counters() {
        let stats = InMemoryStatistics::new();
        stats
            .record_orchestration(DurationMs::from_millis(500))
            .await
            .unwrap();
        let snap = stats.snapshot().await.unwrap();
        assert_eq!(snap.total_orchestrations(), 1);
        assert_eq!(snap.total_duration(), DurationMs::from_millis(500));
    }

    #[tokio::test]
    async fn snapshot_returns_independent_clone() {
        let stats = InMemoryStatistics::new();
        stats
            .record_deliberation(&sp("x"), DurationMs::from_millis(10))
            .await
            .unwrap();
        let snap_a = stats.snapshot().await.unwrap();
        stats
            .record_deliberation(&sp("x"), DurationMs::from_millis(20))
            .await
            .unwrap();
        let snap_b = stats.snapshot().await.unwrap();
        assert_eq!(snap_a.total_deliberations(), 1);
        assert_eq!(snap_b.total_deliberations(), 2);
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let a = InMemoryStatistics::new();
        let b = a.clone();
        a.record_deliberation(&sp("x"), DurationMs::from_millis(5))
            .await
            .unwrap();
        let snap = b.snapshot().await.unwrap();
        assert_eq!(snap.total_deliberations(), 1);
    }
}
