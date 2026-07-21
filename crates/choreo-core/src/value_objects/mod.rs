//! Value objects for the Choreographer domain.
//!
//! Every public function in the domain exchanges value objects instead
//! of primitives. Each value object validates its invariants on
//! construction and cannot be mutated afterwards.

mod agent_kind;
mod attributes;
mod ceremony;
mod ceremony_outcome;
mod council_selector;
mod deliberation_outcome;
mod discrimination;
mod duration;
mod ids;
mod llm_error_kind;
mod num_agents;
mod output_contract;
mod rounds;
mod rubric;
mod score;
mod scoring_mode;
mod specialty;
mod task_description;
mod token_usage;
mod trace_context;
mod validation_mode;

pub use agent_kind::AgentKind;
pub use attributes::Attributes;
pub use ceremony::{
    CeremonyContext, CeremonyDescription, CeremonyEvidenceSourceId, CeremonyGuard,
    CeremonyGuardDeferral, CeremonyGuardDeferralContent, CeremonyId, CeremonyInputDefinition,
    CeremonyInterventionContent, CeremonyInterventionId, CeremonyInterventionKind,
    CeremonyInterventionProvenance, CeremonyInterventionResponse, CeremonyInterventionStatus,
    CeremonyInterventionTarget, CeremonyName, CeremonyOutputDefinition, CeremonyRole,
    CeremonyState, CeremonyStateKind, CeremonyStep, CeremonyStepContribution, CeremonyTranscript,
    CeremonyTransition, CeremonyVersion, GuardCondition, GuardName, IdempotencyKey, InputName,
    InputRequirement, LeaseOwnerId, OutputName, RetryPolicy, RoleAction, RoleId, StateId,
    StepAttempt, StepErrorMessage, StepExecutionRecord, StepHandlerConfig, StepHandlerKind, StepId,
    StepLease, StepOutput, StepResult, StepStatus, StepTimeout, TransitionTrigger,
};
pub use ceremony_outcome::CeremonyOutcome;
pub use council_selector::CouncilSelector;
pub use deliberation_outcome::DeliberationOutcome;
pub use discrimination::Discrimination;
pub use duration::DurationMs;
pub use ids::{AgentId, CouncilId, EventId, ProposalId, TaskId};
pub use llm_error_kind::LlmErrorKind;
pub use num_agents::NumAgents;
pub use output_contract::{
    EvidenceGroundingRule, OutputContract, OutputFieldRule, OutputFormat, SemanticSupportRule,
    DEFAULT_SUPPORT_MIN_CONFIDENCE,
};
pub use rounds::Rounds;
pub use rubric::Rubric;
pub use score::Score;
pub use scoring_mode::ScoringMode;
pub use specialty::Specialty;
pub use task_description::TaskDescription;
pub use token_usage::TokenUsage;
pub use trace_context::TraceContext;
pub use validation_mode::ValidationMode;
