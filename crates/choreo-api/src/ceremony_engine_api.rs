use crate::{
    ApiCapabilities, ApiError, CeremonySummary, DefinitionAnalysisView, PublishedDefinitionView,
    RaiseInterventionRequest, RespondToInterventionRequest, StartCeremonyRequest,
};

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

    /// Put a question, investigation or proposed action to the table.
    /// Capability `raise_intervention`.
    async fn raise_intervention(
        &self,
        request: RaiseInterventionRequest,
    ) -> Result<CeremonySummary, ApiError>;

    /// Answer an open intervention. Capability `respond_to_intervention`.
    ///
    /// A closed intervention refuses: the answer arrived after the table moved
    /// on, and recording it as if it had been heard would misstate the
    /// conversation the audit trail exists to keep.
    async fn respond_to_intervention(
        &self,
        request: RespondToInterventionRequest,
    ) -> Result<CeremonySummary, ApiError>;

    /// Analyze a definition draft, reporting every defect at once.
    /// Capability `analyze_definition`.
    ///
    /// A draft that does not even parse is `Refused` — it is not a defective
    /// definition, it is not a definition. Anything that parses gets the full
    /// report, publishable or not.
    async fn analyze_definition(
        &self,
        definition_yaml: &str,
    ) -> Result<DefinitionAnalysisView, ApiError>;

    /// Publish a definition, immutably. Capability `publish_definition`.
    ///
    /// Idempotent on identical content: republishing the same bytes under the
    /// same name and version answers `already_published` rather than refusing,
    /// which is what makes a retry safe. A version taken by *different*
    /// content is `Refused` — a published version is immutable, and the
    /// answer is a new version, never an overwrite.
    async fn publish_definition(
        &self,
        definition_yaml: &str,
    ) -> Result<PublishedDefinitionView, ApiError>;
}
