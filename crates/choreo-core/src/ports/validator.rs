//! [`ValidatorPort`] — domain-agnostic validation of a proposal.
//!
//! A validator knows how to produce one [`ValidatorReport`] for a given
//! proposal. Adapters compose the reports (lint, policy, fact-check,
//! clinical-safety, …) into a single [`ValidationOutcome`] in the
//! application layer.

use async_trait::async_trait;

use crate::entities::{TaskConstraints, ValidatorReport};
use crate::error::DomainError;

#[async_trait]
pub trait ValidatorPort: Send + Sync {
    /// Stable identifier of what this validator checks (e.g.
    /// `"lint"`, `"policy"`, `"clinical-safety"`). Used to tag the
    /// emitted report.
    fn kind(&self) -> &str;

    /// Validate proposal content. The returned report's `kind` must
    /// match [`Self::kind`].
    async fn validate(
        &self,
        proposal_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError>;
}
