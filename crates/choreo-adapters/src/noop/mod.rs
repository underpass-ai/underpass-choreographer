//! No-op adapters.
//!
//! These implementations honour the port contract without performing
//! any externally-visible side effect. They are the safe defaults for
//! deployments that disable a subsystem (e.g. `nats_enabled=false`)
//! and are also used extensively in tests.

mod agent;
mod agent_factory;
mod ceremony_evidence_source;
mod ceremony_step_handler;
mod executor;
mod messaging;

pub use agent::NoopAgent;
pub use agent_factory::{NoopAgentFactory, NOOP_AGENT_KIND};
pub use ceremony_evidence_source::NoopCeremonyEvidenceSource;
pub use ceremony_step_handler::NoopCeremonyStepHandler;
pub use executor::NoopExecutor;
pub use messaging::NoopMessaging;
