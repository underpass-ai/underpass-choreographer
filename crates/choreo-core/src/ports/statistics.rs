//! [`StatisticsPort`] — operational counter surface.
//!
//! Every [`DeliberateUseCase`](super) invocation that completes
//! records its duration and specialty through this port; every
//! [`OrchestrateUseCase`](super) invocation records the orchestration
//! duration. Adapters decide where the numbers live — in-memory for
//! a single replica, external store for multi-replica setups — and
//! whatever Prometheus / gRPC exposer the composition root wires
//! reads [`Statistics`] through `snapshot`.

use async_trait::async_trait;

use crate::entities::Statistics;
use crate::error::DomainError;
use crate::value_objects::{DurationMs, Specialty};

#[async_trait]
pub trait StatisticsPort: Send + Sync {
    /// Record that a deliberation for `specialty` completed in
    /// `duration`.
    async fn record_deliberation(
        &self,
        specialty: &Specialty,
        duration: DurationMs,
    ) -> Result<(), DomainError>;

    /// Record that an orchestration (deliberate + execute) completed
    /// in `duration`.
    async fn record_orchestration(&self, duration: DurationMs) -> Result<(), DomainError>;

    /// Read-only snapshot. Callers receive a clone so the returned
    /// value is safe to serialise without holding any lock.
    async fn snapshot(&self) -> Result<Statistics, DomainError>;
}
