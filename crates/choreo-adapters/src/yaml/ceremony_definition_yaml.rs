use std::fs;
use std::path::Path;

use choreo_core::entities::{CeremonyDefinition, CeremonyDefinitionDraft};
use choreo_core::error::DomainError;

use super::ceremony_definition_document::CeremonyDefinitionDocument;

#[derive(Debug, Default, Clone, Copy)]
pub struct CeremonyDefinitionYaml;

impl CeremonyDefinitionYaml {
    pub fn parse_str(raw: &str) -> Result<CeremonyDefinition, DomainError> {
        Self::parse_draft_str(raw)?.publish()
    }

    /// Parse into an authoring draft, which may not be publishable.
    ///
    /// Only syntax and value-object failures surface here; structural
    /// defects are reported by the draft's own analysis.
    pub fn parse_draft_str(raw: &str) -> Result<CeremonyDefinitionDraft, DomainError> {
        let document: CeremonyDefinitionDocument =
            serde_yaml::from_str(raw).map_err(|_| DomainError::InvariantViolated {
                reason: "invalid ceremony yaml",
            })?;
        document.into_draft()
    }

    pub fn parse_path(path: impl AsRef<Path>) -> Result<CeremonyDefinition, DomainError> {
        let raw = fs::read_to_string(path).map_err(|_| DomainError::InvariantViolated {
            reason: "failed to read ceremony yaml",
        })?;
        Self::parse_str(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{
        CeremonyName, GuardCondition, GuardName, RoleAction, RoleId, StepId, StepStatus,
        TransitionTrigger,
    };

    const MULTI_STEP: &str = r#"
version: "1.0"
name: "e2e_multi_step"
description: "E2E ceremony"
inputs:
  required:
    - input_data
  optional: []
outputs:
  deliberation:
    type: object
states:
  - id: DELIBERATING
    initial: true
    terminal: false
  - id: COMPLETED
    initial: false
    terminal: true
transitions:
  - from: DELIBERATING
    to: COMPLETED
    trigger: "deliberation_done"
    guards:
      - deliberation_completed
steps:
  - id: deliberate
    state: DELIBERATING
    handler: deliberation_step
    config:
      prompt: "Deliberate on inputs"
guards:
  deliberation_completed:
    type: automated
    check: "step_status:deliberate:COMPLETED"
roles:
  - id: SYSTEM
    allowed_actions:
      - deliberate
      - deliberation_done
timeouts:
  step_default: 60
retry_policies:
  default:
    max_attempts: 2
    backoff_seconds: 1
"#;

    #[test]
    fn parses_laboratory_yaml_into_domain_definition() {
        let definition = CeremonyDefinitionYaml::parse_str(MULTI_STEP).unwrap();

        assert_eq!(
            definition.name(),
            &CeremonyName::new("e2e_multi_step").unwrap()
        );
        assert!(definition
            .step(&StepId::new("deliberate").unwrap())
            .is_some());
        assert!(definition
            .guards()
            .contains_key(&GuardName::new("deliberation_completed").unwrap()));
        assert_eq!(
            definition
                .step(&StepId::new("deliberate").unwrap())
                .unwrap()
                .retry_policy()
                .max_attempts()
                .get(),
            2
        );
        assert_eq!(
            definition
                .step(&StepId::new("deliberate").unwrap())
                .unwrap()
                .timeout()
                .unwrap()
                .duration()
                .get(),
            60_000
        );
        assert!(definition.role_allows(
            &RoleId::new("SYSTEM").unwrap(),
            &RoleAction::transition(TransitionTrigger::new("deliberation_done").unwrap())
        ));
    }

    #[test]
    fn automated_step_status_guard_is_typed() {
        let definition = CeremonyDefinitionYaml::parse_str(MULTI_STEP).unwrap();
        let guard = definition
            .guards()
            .get(&GuardName::new("deliberation_completed").unwrap())
            .unwrap();

        assert!(matches!(
            guard.condition(),
            GuardCondition::StepStatus { step_id, status }
                if step_id == &StepId::new("deliberate").unwrap()
                    && status == &StepStatus::Completed
        ));
    }

    #[test]
    fn human_guard_is_typed() {
        let yaml = r#"
version: "1.0"
name: "approval_ceremony"
states:
  - id: STARTED
    initial: true
  - id: APPROVED
    terminal: true
transitions:
  - from: STARTED
    to: APPROVED
    trigger: approve
    guards:
      - human_approved
guards:
  human_approved:
    type: human
    check: manual_approval
roles:
  - id: PRODUCT_OWNER
    allowed_actions:
      - approve
"#;

        let definition = CeremonyDefinitionYaml::parse_str(yaml).unwrap();
        let guard = definition
            .guards()
            .get(&GuardName::new("human_approved").unwrap())
            .unwrap();

        assert!(matches!(guard.condition(), GuardCondition::HumanApproval));
    }

    #[test]
    fn preserves_yaml_step_declaration_order() {
        let yaml = r#"
version: "1.0"
name: "ordered_steps"
states:
  - id: WORKING
    initial: true
  - id: COMPLETED
    terminal: true
steps:
  - id: write_plan
    state: WORKING
    handler: manual_review
  - id: challenge_plan
    state: WORKING
    handler: manual_review
  - id: archive_plan
    state: WORKING
    handler: manual_review
"#;

        let definition = CeremonyDefinitionYaml::parse_str(yaml).unwrap();
        let step_ids = definition
            .steps_for_state(&choreo_core::value_objects::StateId::new("WORKING").unwrap())
            .map(|step| step.id().as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            step_ids,
            vec!["write_plan", "challenge_plan", "archive_plan"]
        );
    }

    #[test]
    fn invalid_yaml_is_rejected() {
        let err = CeremonyDefinitionYaml::parse_str("version: [").unwrap_err();

        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn unknown_guard_check_is_rejected() {
        let yaml = MULTI_STEP.replace(
            "check: \"step_status:deliberate:COMPLETED\"",
            "check: \"unsupported:expression\"",
        );

        let err = CeremonyDefinitionYaml::parse_str(&yaml).unwrap_err();

        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn unknown_role_action_is_rejected_by_domain_invariants() {
        let yaml = MULTI_STEP.replace(
            "      - deliberation_done",
            "      - deliberation_done\n      - missing_step",
        );

        let err = CeremonyDefinitionYaml::parse_str(&yaml).unwrap_err();

        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "ceremony_role.step_action"
            }
        ));
    }

    #[test]
    fn dynamic_intervention_capabilities_are_valid_role_actions() {
        let yaml = MULTI_STEP.replace(
            "      - deliberation_done",
            "      - deliberation_done\n      - request_intervention\n      - respond_to_intervention",
        );

        let definition = CeremonyDefinitionYaml::parse_str(&yaml).unwrap();
        let role_id = RoleId::new("SYSTEM").unwrap();

        assert!(definition.role_allows(&role_id, &RoleAction::request_intervention()));
        assert!(definition.role_allows(&role_id, &RoleAction::respond_to_intervention()));
    }

    #[test]
    fn parse_path_reports_missing_file_as_domain_error() {
        let err =
            CeremonyDefinitionYaml::parse_path("/tmp/underpass-missing-ceremony.yaml").unwrap_err();

        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }
}
