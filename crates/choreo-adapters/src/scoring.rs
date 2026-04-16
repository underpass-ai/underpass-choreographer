//! Scoring adapters.
//!
//! The domain defines `ScoringPort` as a single trait that aggregates
//! validator reports into a [`Score`]. This module ships one minimal,
//! honestly-described implementation; operators can plug in their own
//! policy (weighted average, fail-fast, learned combinator, …) by
//! implementing `ScoringPort` elsewhere.

use async_trait::async_trait;
use choreo_core::entities::ValidatorReport;
use choreo_core::error::DomainError;
use choreo_core::ports::ScoringPort;
use choreo_core::value_objects::Score;

/// Uniform scoring: the score is the fraction of reports that passed.
///
/// With zero reports the score is [`Score::MIN`] — no evidence means
/// no confidence. That choice biases operators to configure at least
/// one validator rather than silently returning a perfect score from
/// thin air.
#[derive(Debug, Default, Clone, Copy)]
pub struct UniformScoring;

impl UniformScoring {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ScoringPort for UniformScoring {
    async fn score(&self, reports: &[ValidatorReport]) -> Result<Score, DomainError> {
        if reports.is_empty() {
            return Ok(Score::MIN);
        }
        let passed = reports.iter().filter(|r| r.passed()).count();
        let total = reports.len();
        #[allow(clippy::cast_precision_loss)]
        let value = passed as f64 / total as f64;
        Score::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::Attributes;

    fn report(passed: bool) -> ValidatorReport {
        ValidatorReport::new("k", passed, "", Attributes::empty()).unwrap()
    }

    #[tokio::test]
    async fn empty_reports_score_to_min() {
        let s = UniformScoring::new().score(&[]).await.unwrap();
        assert_eq!(s, Score::MIN);
    }

    #[tokio::test]
    async fn all_passed_scores_to_max() {
        let s = UniformScoring::new()
            .score(&[report(true), report(true)])
            .await
            .unwrap();
        assert_eq!(s, Score::MAX);
    }

    #[tokio::test]
    async fn mixed_reports_score_to_pass_fraction() {
        let s = UniformScoring::new()
            .score(&[report(true), report(false), report(true), report(false)])
            .await
            .unwrap();
        assert_eq!(s.get(), 0.5);
    }

    #[tokio::test]
    async fn all_failed_scores_to_min() {
        let s = UniformScoring::new()
            .score(&[report(false), report(false)])
            .await
            .unwrap();
        assert_eq!(s, Score::MIN);
    }
}
