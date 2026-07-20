//! Domain entities and aggregates.
//!
//! Entities have identity that persists across state changes. Aggregate
//! roots own invariants spanning multiple objects; state transitions
//! happen through their methods, not by mutating fields directly.

mod ceremony_definition;
mod ceremony_instance;
mod ceremony_intervention;
mod council;
mod deliberation;
mod external_context;
mod proposal;
mod statistics;
mod task;
mod validation;

pub use ceremony_definition::CeremonyDefinition;
pub use ceremony_instance::CeremonyInstance;
pub use ceremony_intervention::CeremonyIntervention;
pub use council::Council;
pub use deliberation::{Deliberation, DeliberationPhase, RankedOutcome};
pub use external_context::{ContextItem, ContextReference, ContextSummary, ExternalContextBundle};
pub use proposal::Proposal;
pub use statistics::Statistics;
pub use task::{Task, TaskConstraints, TaskMetadata};
pub use validation::{ValidationOutcome, ValidatorReport};
