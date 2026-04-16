//! No-op adapters.
//!
//! These implementations honour the port contract without performing
//! any externally-visible side effect. They are the safe defaults for
//! deployments that disable a subsystem (e.g. `nats_enabled=false`)
//! and are also used extensively in tests.

mod agent;
mod executor;
mod messaging;

pub use agent::NoopAgent;
pub use executor::NoopExecutor;
pub use messaging::NoopMessaging;
