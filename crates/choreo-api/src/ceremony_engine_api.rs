use crate::{ApiCapabilities, ApiError, CeremonySummary, StartCeremonyRequest};

/// What a consuming product may ask of the embedded engine.
///
/// Reads, plus one mutation: starting an instance from a **published**
/// definition. Advancing and publishing stay behind the engine's own surfaces,
/// where their transactionality and audit live; the contract grows by adding
/// named capabilities, never by widening what an existing one means (ADR-004).
/// A consumer checks the capability report before relying on any of this — and
/// keeps working, with a stub, when the engine is absent.
#[async_trait::async_trait]
pub trait CeremonyEngineApi: Send + Sync {
    /// What this implementation is and what it can do. Checked by consumers at
    /// startup, before anything is at stake.
    fn capabilities(&self) -> ApiCapabilities;

    /// Every ceremony instance the engine holds.
    ///
    /// The consumer filters by its own context keys; the engine does not know
    /// what they mean and is not asked to.
    async fn ceremonies(&self) -> Result<Vec<CeremonySummary>, ApiError>;

    /// One ceremony instance by identity.
    async fn ceremony(&self, ceremony_id: &str) -> Result<CeremonySummary, ApiError>;

    /// Start an instance from a published definition. Capability
    /// `start_ceremony`.
    ///
    /// `CeremonyNotFound` when nothing is published under that name and
    /// version — publishing is the remedy, not retrying. A taken instance
    /// identity is `Refused`: an identity is one instance forever, and the
    /// answer is a new identity, never a restart of someone else's.
    async fn start_ceremony(
        &self,
        request: StartCeremonyRequest,
    ) -> Result<CeremonySummary, ApiError>;
}
