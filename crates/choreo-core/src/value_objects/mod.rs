//! Value objects for the Choreographer domain.
//!
//! Every public function in the domain exchanges value objects instead
//! of primitives. Each value object validates its invariants on
//! construction and cannot be mutated afterwards.

mod attributes;
mod duration;
mod ids;
mod num_agents;
mod rounds;
mod rubric;
mod score;
mod specialty;
mod task_description;

pub use attributes::Attributes;
pub use duration::DurationMs;
pub use ids::{AgentId, CouncilId, EventId, ProposalId, TaskId};
pub use num_agents::NumAgents;
pub use rounds::Rounds;
pub use rubric::Rubric;
pub use score::Score;
pub use specialty::Specialty;
pub use task_description::TaskDescription;
