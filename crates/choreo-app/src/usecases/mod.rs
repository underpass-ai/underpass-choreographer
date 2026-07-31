//! Use cases.
//!
//! Each use case is a struct, constructor-injected with the ports it
//! needs, and exposes a single `execute` method. This keeps
//! responsibilities narrow (SRP) and dependencies explicit (DIP).

#[cfg(test)]
mod ceremony_test_support;

mod apply_ceremony_transition_input;
mod apply_ceremony_transition_use_case;
mod approve_ceremony_guard_input;
mod approve_ceremony_guard_use_case;
mod ceremony_draft_view;
mod ceremony_instance_view;
mod ceremony_participant_descriptor;
mod ceremony_step_trace;
mod close_ceremony_intervention_input;
mod close_ceremony_intervention_use_case;
mod collect_ceremony_evidence_input;
mod collect_ceremony_evidence_use_case;
mod complete_ceremony_step_input;
mod complete_ceremony_step_use_case;
mod create_council;
mod defer_ceremony_guard_input;
mod defer_ceremony_guard_use_case;
mod delete_council;
mod deliberate;
mod diff_ceremony_definitions_use_case;
mod get_ceremony_definition;
mod get_ceremony_instance;
mod get_ceremony_transcript;
mod get_deliberation;
mod list_ceremony_definitions;
mod list_ceremony_instances;
mod list_councils;
mod mount_ceremony_definitions_output;
mod mount_ceremony_definitions_use_case;
mod orchestrate;
mod prepare_ceremony_participants_input;
mod prepare_ceremony_participants_use_case;
mod publish_ceremony_definition_use_case;
mod publish_outbox_input;
mod publish_outbox_round;
mod publish_outbox_use_case;
mod register_agent;
mod request_ceremony_intervention_input;
mod request_ceremony_intervention_use_case;
mod resolve_ceremony_definition_use_case;
mod respond_to_ceremony_intervention_input;
mod respond_to_ceremony_intervention_use_case;
mod run_ceremony_input;
mod run_ceremony_output;
mod run_ceremony_step_input;
mod run_ceremony_step_output;
mod run_ceremony_step_use_case;
mod run_ceremony_use_case;
mod run_council_decision;
mod start_ceremony_input;
mod start_ceremony_step_input;
mod start_ceremony_step_use_case;
mod start_ceremony_use_case;
mod start_published_ceremony_use_case;
mod unregister_agent;
mod winner_selection;

pub use apply_ceremony_transition_input::ApplyCeremonyTransitionInput;
pub use apply_ceremony_transition_use_case::ApplyCeremonyTransitionUseCase;
pub use approve_ceremony_guard_input::ApproveCeremonyGuardInput;
pub use approve_ceremony_guard_use_case::ApproveCeremonyGuardUseCase;
pub use ceremony_draft_view::{CeremonyDraftSummary, CeremonyDraftView};
pub use ceremony_instance_view::{
    CeremonyGuardView, CeremonyInstanceView, CeremonyStepView, CeremonyTransitionView,
};
pub use ceremony_participant_descriptor::CeremonyParticipantDescriptor;
pub use ceremony_step_trace::CeremonyStepTrace;
pub use close_ceremony_intervention_input::CloseCeremonyInterventionInput;
pub use close_ceremony_intervention_use_case::CloseCeremonyInterventionUseCase;
pub use collect_ceremony_evidence_input::CollectCeremonyEvidenceInput;
pub use collect_ceremony_evidence_use_case::CollectCeremonyEvidenceUseCase;
pub use complete_ceremony_step_input::CompleteCeremonyStepInput;
pub use complete_ceremony_step_use_case::CompleteCeremonyStepUseCase;
pub use create_council::{CreateCouncilInput, CreateCouncilUseCase};
pub use defer_ceremony_guard_input::DeferCeremonyGuardInput;
pub use defer_ceremony_guard_use_case::DeferCeremonyGuardUseCase;
pub use delete_council::DeleteCouncilUseCase;
pub use deliberate::{DeliberateOutput, DeliberateUseCase};
pub use diff_ceremony_definitions_use_case::{
    CeremonyDefinitionSource, DiffCeremonyDefinitionsUseCase,
};
pub use get_ceremony_definition::GetCeremonyDefinitionUseCase;
pub use get_ceremony_instance::GetCeremonyInstanceUseCase;
pub use get_ceremony_transcript::GetCeremonyTranscriptUseCase;
pub use get_deliberation::GetDeliberationUseCase;
pub use list_ceremony_definitions::ListCeremonyDefinitionsUseCase;
pub use list_ceremony_instances::ListCeremonyInstancesUseCase;
pub use list_councils::ListCouncilsUseCase;
pub use mount_ceremony_definitions_output::MountCeremonyDefinitionsOutput;
pub use mount_ceremony_definitions_use_case::MountCeremonyDefinitionsUseCase;
pub use orchestrate::{OrchestrateOutput, OrchestrateUseCase};
pub use prepare_ceremony_participants_input::PrepareCeremonyParticipantsInput;
pub use prepare_ceremony_participants_use_case::PrepareCeremonyParticipantsUseCase;
pub use publish_ceremony_definition_use_case::PublishCeremonyDefinitionUseCase;
pub use publish_outbox_input::PublishOutboxInput;
pub use publish_outbox_round::PublishOutboxRound;
pub use publish_outbox_use_case::PublishOutboxUseCase;
pub use register_agent::RegisterAgentUseCase;
pub use request_ceremony_intervention_input::RequestCeremonyInterventionInput;
pub use request_ceremony_intervention_use_case::RequestCeremonyInterventionUseCase;
pub use resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
pub use respond_to_ceremony_intervention_input::RespondToCeremonyInterventionInput;
pub use respond_to_ceremony_intervention_use_case::RespondToCeremonyInterventionUseCase;
pub use run_ceremony_input::RunCeremonyInput;
pub use run_ceremony_output::RunCeremonyOutput;
pub use run_ceremony_step_input::RunCeremonyStepInput;
pub use run_ceremony_step_output::RunCeremonyStepOutput;
pub use run_ceremony_step_use_case::RunCeremonyStepUseCase;
pub use run_ceremony_use_case::RunCeremonyUseCase;
pub use run_council_decision::{
    RunCouncilDecisionInput, RunCouncilDecisionOutput, RunCouncilDecisionUseCase,
};
pub use start_ceremony_input::StartCeremonyInput;
pub use start_ceremony_step_input::StartCeremonyStepInput;
pub use start_ceremony_step_use_case::StartCeremonyStepUseCase;
pub use start_ceremony_use_case::StartCeremonyUseCase;
pub use start_published_ceremony_use_case::StartPublishedCeremonyUseCase;
pub use unregister_agent::UnregisterAgentUseCase;
