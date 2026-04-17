//! Use cases.
//!
//! Each use case is a struct, constructor-injected with the ports it
//! needs, and exposes a single `execute` method. This keeps
//! responsibilities narrow (SRP) and dependencies explicit (DIP).

mod create_council;
mod delete_council;
mod deliberate;
mod get_deliberation;
mod list_councils;
mod orchestrate;
mod register_agent;
mod unregister_agent;

pub use create_council::{CreateCouncilInput, CreateCouncilUseCase};
pub use delete_council::DeleteCouncilUseCase;
pub use deliberate::{DeliberateOutput, DeliberateUseCase};
pub use get_deliberation::GetDeliberationUseCase;
pub use list_councils::ListCouncilsUseCase;
pub use orchestrate::{OrchestrateOutput, OrchestrateUseCase};
pub use register_agent::RegisterAgentUseCase;
pub use unregister_agent::UnregisterAgentUseCase;
