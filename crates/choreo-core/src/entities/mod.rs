//! Domain entities and aggregates.
//!
//! Entities have identity that persists across state changes. Aggregate
//! roots own invariants spanning multiple objects; state transitions
//! happen through their methods, not by mutating fields directly.

mod council;
mod deliberation;
mod proposal;
mod statistics;
mod task;
mod validation;

pub use council::Council;
pub use deliberation::{Deliberation, DeliberationPhase, RankedOutcome};
pub use proposal::Proposal;
pub use statistics::Statistics;
pub use task::{Task, TaskConstraints};
pub use validation::{ValidationOutcome, ValidatorReport};
