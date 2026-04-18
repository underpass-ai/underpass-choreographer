//! Composition root for the Underpass Choreographer.
//!
//! The binary entry point (`src/main.rs`) is kept tiny: it installs
//! tracing, calls [`compose`] to wire every adapter, then runs the
//! resulting [`Application`]. All wiring logic lives here so it can
//! be unit-tested without starting a server.

pub mod compose;
pub mod health;
pub mod runtime;
pub mod seeding;
pub mod telemetry;

pub use compose::{compose, Application, ComposeError};
pub use health::{router as health_router, HealthState};
pub use runtime::serve;
pub use telemetry::{init_tracing, TelemetryGuard};
