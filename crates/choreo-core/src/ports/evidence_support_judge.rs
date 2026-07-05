//! [`EvidenceSupportJudgePort`] — does a claim's cited evidence
//! actually support the claim?
//!
//! The grounding gate (`claims-evidence-grounded`) proves a citation
//! *exists*; this port answers whether the citation *holds*. That is a
//! semantic judgment, so implementations are typically model-backed
//! (an LLM or NLI adapter) — but the port keeps the core
//! provider-agnostic, exactly like [`super::AgentPort`], and the
//! *decision* stays deterministic: the validator compares the verdict
//! against the contract's configured threshold and records the verdict
//! itself in the report, so the model's opinion becomes evidence in
//! the decision record rather than the last word.

use async_trait::async_trait;

use crate::error::DomainError;

/// One evidence excerpt the judge reads: the pack reference id and the
/// body text it resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceExcerpt {
    pub reference: String,
    pub body: String,
}

/// The judge's verdict for one claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportVerdict {
    /// Whether the cited evidence, on its own, supports the claim.
    pub supported: bool,
    /// Confidence in the verdict, percent (0–100).
    pub confidence: u8,
    /// One short sentence explaining the verdict; recorded in the
    /// validator report so it survives into spans and logs.
    pub rationale: String,
}

#[async_trait]
pub trait EvidenceSupportJudgePort: Send + Sync {
    /// Assess whether `evidence` supports `claim_text`. `evidence`
    /// carries only the excerpts the claim actually cited — the judge
    /// must not be able to lean on evidence the claim did not invoke.
    async fn assess(
        &self,
        claim_text: &str,
        evidence: &[EvidenceExcerpt],
    ) -> Result<SupportVerdict, DomainError>;
}
