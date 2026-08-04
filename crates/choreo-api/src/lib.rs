//! The published contract of the embedded Choreographer.
//!
//! This crate is what a consuming product is allowed to know. It holds plain
//! views, a capability report, an error vocabulary and one trait — and no
//! domain types, no adapters, no storage. A consumer that compiles against this
//! crate alone can be tested with a stub and swapped onto any implementation
//! that honours the same contract.
//!
//! Versioned deliberately. [`CONTRACT_VERSION`] moves when the meaning of this
//! surface changes, independently of the library's own release number: two
//! builds of the same release can differ in features, and a consumer that
//! guessed capabilities from a version string would find out mid-run. Consumers
//! check [`ApiCapabilities`] at startup instead.
//!
//! Vocabulary note (ADR-001): these types speak the engine's own language —
//! ceremonies. A consuming product maps them to its own terms at its own
//! boundary; nothing of that product's vocabulary appears here.

mod api_capabilities;
mod api_error;
mod ceremony_engine_api;
mod ceremony_summary;

pub use api_capabilities::ApiCapabilities;
pub use api_error::ApiError;
pub use ceremony_engine_api::CeremonyEngineApi;
pub use ceremony_summary::{CeremonyParticipant, CeremonySummary};

/// The revision of this contract.
///
/// Moves on meaning, not on release: adding a capability keeps the version,
/// changing what an existing field or method means raises it.
pub const CONTRACT_VERSION: u32 = 1;
