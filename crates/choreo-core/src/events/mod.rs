//! Domain events.
//!
//! Immutable records of something that has happened (or has been
//! requested) in the Choreographer domain. Every event carries the
//! common [`EventEnvelope`] metadata plus its own payload.
//!
//! The wire contract for these events lives in
//! `specs/asyncapi/choreographer.asyncapi.yaml`; anything added here
//! must keep the contract as source of truth.

mod deliberation_completed;
mod envelope;
mod phase_changed;
mod task_completed;
mod task_dispatched;
mod task_failed;
mod trigger;

pub use deliberation_completed::DeliberationCompletedEvent;
pub use envelope::EventEnvelope;
pub use phase_changed::PhaseChangedEvent;
pub use task_completed::TaskCompletedEvent;
pub use task_dispatched::TaskDispatchedEvent;
pub use task_failed::TaskFailedEvent;
pub use trigger::TriggerEvent;
