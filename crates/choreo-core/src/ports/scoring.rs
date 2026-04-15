//! [`ScoringPort`] — domain-agnostic aggregation of validator reports
//! into a single [`Score`] for a proposal.
//!
//! Keeping scoring behind a trait lets operators plug their own policy
//! (weighted average, fail-fast on any failed report, learned
//! combinator, …) without touching the core.

use async_trait::async_trait;

use crate::entities::ValidatorReport;
use crate::error::DomainError;
use crate::value_objects::Score;

#[async_trait]
pub trait ScoringPort: Send + Sync {
    /// Combine a list of validator reports into a single score.
    async fn score(&self, reports: &[ValidatorReport]) -> Result<Score, DomainError>;
}
