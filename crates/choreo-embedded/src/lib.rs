//! In-process distribution of the Underpass Choreographer ceremony engine.
//!
//! [`EmbeddedChoreographer`] executes the same `choreo-app` use cases as the
//! deployable service without opening sockets or reading process-wide
//! configuration. Hosts may use the local defaults or inject any adapter that
//! implements the ports from `choreo-core`.

#![deny(missing_debug_implementations)]

mod callback_ceremony_evidence_source;
mod callback_ceremony_step_handler;
mod embedded_choreographer;
mod embedded_choreographer_builder;
mod in_process_ceremony_definition_source;

pub use callback_ceremony_evidence_source::CallbackCeremonyEvidenceSource;
pub use callback_ceremony_step_handler::CallbackCeremonyStepHandler;
pub use embedded_choreographer::EmbeddedChoreographer;
pub use embedded_choreographer_builder::EmbeddedChoreographerBuilder;
pub use in_process_ceremony_definition_source::InProcessCeremonyDefinitionSource;

/// Choreographer release version used by this embedded distribution.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
