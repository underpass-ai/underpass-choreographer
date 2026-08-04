use crate::{ApiCapabilities, ApiError, CeremonySummary};

/// What a consuming product may ask of the embedded engine.
///
/// Reads only. Starting, advancing and publishing are mutations with their own
/// use cases, transactionality and audit inside the engine; a consumer reaches
/// them through the engine's own surfaces, not through this contract. What this
/// trait promises is the part a consumer needs to *observe* ceremonies from its
/// own bounded context — and to keep working, with a stub, when the engine is
/// absent.
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
}
