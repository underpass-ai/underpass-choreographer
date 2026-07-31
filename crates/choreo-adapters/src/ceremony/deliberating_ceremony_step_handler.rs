use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_app::usecases::DeliberateUseCase;
use choreo_core::entities::{Task, TaskConstraints};
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyStepHandlerPort, CeremonyStepHandlerRequest};
use choreo_core::value_objects::{
    Attributes, RoleId, StepOutput, StepResult, TaskDescription, TaskId,
};
use serde_json::{json, Value};
use tracing::info;

use super::DeliberationStepConfig;

/// Attribute key under which a deliberation winner's content is stored.
pub(crate) const WINNER_CONTENT_KEY: &str = "winner_content";
/// Defensive per-turn cap when rendering a prior contribution into the
/// prompt, so one runaway turn cannot dominate the brief.
const MAX_RENDERED_CONTRIBUTION_LEN: usize = 4000;

#[derive(Debug, Clone)]
pub struct DeliberatingCeremonyStepHandler {
    deliberate: Arc<DeliberateUseCase>,
}

impl DeliberatingCeremonyStepHandler {
    #[must_use]
    pub fn new(deliberate: Arc<DeliberateUseCase>) -> Self {
        Self { deliberate }
    }
}

#[async_trait]
impl CeremonyStepHandlerPort for DeliberatingCeremonyStepHandler {
    async fn execute(
        &self,
        request: CeremonyStepHandlerRequest,
    ) -> Result<StepResult, DomainError> {
        let config = DeliberationStepConfig::from_request(&request)?;
        let task = task_from(&request, &config)?;
        let output = self.deliberate.execute(task).await?;
        let ranked = output.deliberation.ranked_outcomes()?;
        let winner = ranked
            .iter()
            .find(|candidate| candidate.proposal().id() == &output.winner_proposal_id)
            .ok_or(DomainError::InvariantViolated {
                reason: "ceremony step winner proposal is missing from ranked outcomes",
            })?;

        info!(
            ceremony_id = request.instance_id().as_str(),
            step_id = request.step_id().as_str(),
            specialty = config.specialty().as_str(),
            winner_proposal_id = winner.proposal().id().as_str(),
            "ceremony step deliberation completed"
        );

        StepResult::completed(StepOutput::new(Attributes::new(output_attributes(
            &request,
            &output.winner_proposal_id,
            winner.proposal().content(),
            ranked.len(),
        ))?))
    }
}

fn task_from(
    request: &CeremonyStepHandlerRequest,
    config: &DeliberationStepConfig,
) -> Result<Task, DomainError> {
    let mut constraints = TaskConstraints::new(
        choreo_core::value_objects::Rubric::empty(),
        config.rounds(),
        config.num_agents(),
        None,
    );
    // A step-declared contract turns the deliberation into a policy
    // gate: the contract validators judge every proposal, and with no
    // satisfying proposal the step fails as NoValidProposal naming the
    // contract_id — deterministic rejection, not another model opinion.
    if let Some(contract) = config.output_contract() {
        constraints = constraints.with_output_contract(contract.clone());
    }
    Ok(Task::new(
        task_id_from(request)?,
        // Whom the work is put to. The session's seating wins over the
        // step's configuration when there is any: a definition says
        // what a role does, and a binding says who is doing it here.
        request
            .bound_specialty()
            .unwrap_or_else(|| config.specialty())
            .clone(),
        build_description(request, config)?,
        constraints,
        Attributes::new(task_attributes(request, config))?,
    ))
}

/// Build the brief an agent deliberates on: the ceremony role and meeting
/// frame, the prior interventions rendered as prose (when the step opts
/// into the transcript via `see_prior`), and finally the step's own
/// instruction. Framing it as a natural, role-aware brief — rather than a
/// raw JSON context bundle — is what lets the agents speak in character
/// and answer what came before.
fn build_description(
    request: &CeremonyStepHandlerRequest,
    config: &DeliberationStepConfig,
) -> Result<TaskDescription, DomainError> {
    let mut text = String::new();
    let ceremony = request.definition_name().as_str();
    let stage = request.current_state().as_str();

    match request.role_id() {
        Some(role) => {
            let _ = writeln!(
                text,
                "You are acting as {role} in the \"{ceremony}\" ceremony (current stage: {stage}).",
                role = role.as_str(),
            );
        }
        None => {
            let _ = writeln!(
                text,
                "You are a participant in the \"{ceremony}\" ceremony (current stage: {stage}).",
            );
        }
    }

    let brief = render_brief(request.context().attributes());
    if !brief.is_empty() {
        text.push_str("\nMission brief:\n");
        text.push_str(&brief);
    }

    if config.see_prior() && !request.transcript().is_empty() {
        text.push_str("\nThe meeting so far:\n");
        for contribution in request.transcript().contributions() {
            let said = contribution
                .output()
                .attributes()
                .get(WINNER_CONTENT_KEY)
                .and_then(Value::as_str)
                .unwrap_or("(no content recorded)");
            let _ = writeln!(
                text,
                "- {role} ({step}): {said}",
                role = contribution.role_id().as_str(),
                step = contribution.step_id().as_str(),
                said = truncate(said, MAX_RENDERED_CONTRIBUTION_LEN),
            );
        }
    }

    text.push_str(&render_interventions(request));

    let _ = write!(
        text,
        "\nYour task now: {}",
        config.task_description().as_str()
    );

    TaskDescription::new(text)
}

/// Render participant-created agenda items visible to the executing role.
/// Scoped interventions remain private to the requester and their targets;
/// table-wide interventions are visible to every role.
fn render_interventions(request: &CeremonyStepHandlerRequest) -> String {
    let mut out = String::new();
    for intervention in request.interventions().iter().filter(|intervention| {
        request.role_id().is_none_or(|role_id| {
            intervention.requested_by() == role_id || intervention.target().accepts(role_id)
        })
    }) {
        if out.is_empty() {
            out.push_str("\nLive participant requests:\n");
        }
        let target = intervention.target().role_ids().map_or_else(
            || "the whole table".to_owned(),
            |role_ids| {
                role_ids
                    .iter()
                    .map(RoleId::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        );
        let _ = writeln!(
            out,
            "- [{status}] {kind} requested by {requester} for {target}: {message}",
            status = intervention.status().as_label(),
            kind = intervention.kind().as_label(),
            requester = intervention.requested_by().as_str(),
            message = truncate(
                intervention.request().message(),
                MAX_RENDERED_CONTRIBUTION_LEN
            ),
        );
        render_intervention_details(
            &mut out,
            "request details",
            intervention.request().details(),
        );
        for response in intervention.responses() {
            let _ = writeln!(
                out,
                "  - {role} responded: {message}",
                role = response.role_id().as_str(),
                message = truncate(response.content().message(), MAX_RENDERED_CONTRIBUTION_LEN),
            );
            render_intervention_details(&mut out, "response details", response.content().details());
        }
    }
    out
}

fn render_intervention_details(out: &mut String, label: &str, details: &Attributes) {
    if details.as_map().is_empty() {
        return;
    }
    let rendered = serde_json::to_string(details.as_map()).unwrap_or_else(|_| "{}".to_owned());
    let _ = writeln!(
        out,
        "    {label}: {}",
        truncate(&rendered, MAX_RENDERED_CONTRIBUTION_LEN)
    );
}

/// Render the ceremony context as the mission brief shown to every
/// agent. Boolean entries (such as human-approval guard flags) are
/// skipped — only the caller-supplied inputs make it into the brief.
fn render_brief(attributes: &Attributes) -> String {
    let mut out = String::new();
    for (key, value) in attributes.as_map() {
        if value.is_boolean() {
            continue;
        }
        let rendered = match value.as_str() {
            Some(text) => text.to_owned(),
            None => value.to_string(),
        };
        let _ = writeln!(
            out,
            "- {key}: {}",
            truncate(&rendered, MAX_RENDERED_CONTRIBUTION_LEN)
        );
    }
    out
}

/// Cap a rendered prior turn so a single runaway contribution cannot
/// dominate the brief; appends an ellipsis when truncated.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn task_id_from(request: &CeremonyStepHandlerRequest) -> Result<TaskId, DomainError> {
    TaskId::new(format!(
        "ceremony-{}-{}-{}",
        request.instance_id().as_str(),
        request.step_id().as_str(),
        request.attempt().get()
    ))
}

fn task_attributes(
    request: &CeremonyStepHandlerRequest,
    config: &DeliberationStepConfig,
) -> BTreeMap<String, Value> {
    let mut attributes = request.context().attributes().as_map().clone();
    attributes.insert(
        "ceremony.instance_id".to_owned(),
        json!(request.instance_id().as_str()),
    );
    attributes.insert(
        "ceremony.definition_name".to_owned(),
        json!(request.definition_name().as_str()),
    );
    attributes.insert(
        "ceremony.definition_version".to_owned(),
        json!(request.definition_version().as_str()),
    );
    attributes.insert(
        "ceremony.current_state".to_owned(),
        json!(request.current_state().as_str()),
    );
    attributes.insert(
        "ceremony.step_id".to_owned(),
        json!(request.step_id().as_str()),
    );
    attributes.insert(
        "ceremony.handler_kind".to_owned(),
        json!(request.handler_kind().as_str()),
    );
    attributes.insert(
        "ceremony.deliberation_specialty".to_owned(),
        json!(config.specialty().as_str()),
    );
    attributes.insert(
        "ceremony.attempt".to_owned(),
        json!(request.attempt().get()),
    );
    attributes
}

fn output_attributes(
    request: &CeremonyStepHandlerRequest,
    winner_proposal_id: &choreo_core::value_objects::ProposalId,
    winner_content: &str,
    candidates_total: usize,
) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "task_id".to_owned(),
            json!(format!(
                "ceremony-{}-{}-{}",
                request.instance_id().as_str(),
                request.step_id().as_str(),
                request.attempt().get()
            )),
        ),
        (
            "winner_proposal_id".to_owned(),
            json!(winner_proposal_id.as_str()),
        ),
        ("winner_content".to_owned(), json!(winner_content)),
        (
            "candidates_total".to_owned(),
            json!(u64::try_from(candidates_total).unwrap_or(u64::MAX)),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use choreo_app::usecases::DeliberateUseCase;
    use choreo_core::entities::{CeremonyIntervention, Council};
    use choreo_core::ports::{ClockPort, CouncilRegistryPort, StatisticsPort};
    use choreo_core::value_objects::{
        AgentId, Attributes, CeremonyContext, CeremonyId, CeremonyInterventionContent,
        CeremonyInterventionId, CeremonyInterventionKind, CeremonyInterventionTarget, CeremonyName,
        CeremonyStepContribution, CeremonyTranscript, CeremonyVersion, CouncilId, RoleId,
        Specialty, StateId, StepAttempt, StepHandlerConfig, StepHandlerKind, StepId, StepOutput,
        StepStatus,
    };
    use serde_json::json;

    use crate::clock::SystemClock;
    use crate::memory::{
        InMemoryAgentRegistry, InMemoryCouncilRegistry, InMemoryDeliberationRepository,
        InMemoryStatistics,
    };
    use crate::noop::{NoopAgent, NoopMessaging};
    use crate::scoring::UniformScoring;
    use crate::validators::{
        ContentNonEmptyValidator, JsonObjectOutputValidator, RequiredFieldsValidator,
    };

    use super::*;

    /// The point of seating: the work goes to whoever this session
    /// seated, not to whoever the document names in general. Only the
    /// bound council exists here, so the step can succeed only by
    /// asking it.
    #[tokio::test]
    async fn a_seated_role_sends_its_work_to_the_council_the_session_bound() {
        let seated = Specialty::new("senior_sre_panel").unwrap();
        let agent_id = AgentId::new("agent-senior_sre_panel-0").unwrap();
        let agent_registry = Arc::new(InMemoryAgentRegistry::new());
        agent_registry
            .insert(Arc::new(NoopAgent::new(agent_id.clone(), seated.clone())))
            .await
            .unwrap();

        let council_registry = Arc::new(InMemoryCouncilRegistry::new());
        council_registry
            .register(
                Council::new(
                    CouncilId::new("council-senior_sre_panel").unwrap(),
                    seated.clone(),
                    vec![agent_id],
                    SystemClock::new().now(),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let deliberate = Arc::new(DeliberateUseCase::new(
            Arc::new(SystemClock::new()),
            council_registry,
            agent_registry,
            vec![Arc::new(ContentNonEmptyValidator::new())],
            Arc::new(UniformScoring::new()),
            Arc::new(InMemoryDeliberationRepository::new()),
            Arc::new(NoopMessaging::new()),
            Arc::new(InMemoryStatistics::new()),
            Arc::new(choreo_core::ports::NoopMetricsRecorder),
            "ceremony-step-handler-test",
        ));
        let handler = DeliberatingCeremonyStepHandler::new(deliberate);

        // The step still declares `facilitation_prompt`; no council
        // for it is registered.
        let request = request_with_prompt().with_bound_specialty(Some(seated.clone()));
        let result = handler.execute(request).await.unwrap();

        assert_eq!(result.status(), StepStatus::Completed);
        // Completing at all already proves the redirect — the step's
        // own specialty has no council. The answer naming the seated
        // agent proves who actually gave it.
        let winner = result
            .output()
            .attributes()
            .get("winner_content")
            .and_then(|value| value.as_str())
            .expect("a completed step should carry the winning content");
        assert!(
            winner.contains("agent-senior_sre_panel-0"),
            "the answer came from somewhere other than the seated panel: {winner}"
        );
    }

    #[tokio::test]
    async fn executes_step_by_running_deliberation_for_configured_specialty() {
        let specialty = Specialty::new("facilitation_prompt").unwrap();
        let agent_id = AgentId::new("agent-facilitation_prompt-0").unwrap();
        let agent_registry = Arc::new(InMemoryAgentRegistry::new());
        agent_registry
            .insert(Arc::new(NoopAgent::new(
                agent_id.clone(),
                specialty.clone(),
            )))
            .await
            .unwrap();

        let council_registry = Arc::new(InMemoryCouncilRegistry::new());
        council_registry
            .register(
                Council::new(
                    CouncilId::new("council-facilitation_prompt").unwrap(),
                    specialty.clone(),
                    vec![agent_id],
                    SystemClock::new().now(),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let statistics = Arc::new(InMemoryStatistics::new());
        let deliberate = Arc::new(DeliberateUseCase::new(
            Arc::new(SystemClock::new()),
            council_registry,
            agent_registry,
            vec![Arc::new(ContentNonEmptyValidator::new())],
            Arc::new(UniformScoring::new()),
            Arc::new(InMemoryDeliberationRepository::new()),
            Arc::new(NoopMessaging::new()),
            statistics.clone(),
            Arc::new(choreo_core::ports::NoopMetricsRecorder),
            "ceremony-step-handler-test",
        ));
        let handler = DeliberatingCeremonyStepHandler::new(deliberate);

        let result = handler.execute(request_with_prompt()).await.unwrap();

        assert_eq!(result.status(), StepStatus::Completed);
        let attributes = result.output().attributes();
        assert_eq!(
            attributes.get("task_id").and_then(|value| value.as_str()),
            Some("ceremony-ceremony-1-open_room-1")
        );
        assert!(attributes
            .get("winner_proposal_id")
            .and_then(|value| value.as_str())
            .is_some());
        assert!(attributes
            .get("winner_content")
            .and_then(|value| value.as_str())
            .is_some_and(|content| content.contains("Open the meeting")));
        assert_eq!(
            attributes
                .get("candidates_total")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            statistics
                .snapshot()
                .await
                .unwrap()
                .per_specialty()
                .get(&specialty)
                .copied(),
            Some(1)
        );
    }

    #[tokio::test]
    async fn step_with_output_contract_fails_as_no_valid_proposal_when_violated() {
        // The noop agent answers in plain prose, which cannot satisfy a
        // json_object contract — the deterministic gate must reject the
        // step naming the contract, not fall back to picking a winner.
        let specialty = Specialty::new("policy_gate").unwrap();
        let agent_id = AgentId::new("agent-policy_gate-0").unwrap();
        let agent_registry = Arc::new(InMemoryAgentRegistry::new());
        agent_registry
            .insert(Arc::new(NoopAgent::new(
                agent_id.clone(),
                specialty.clone(),
            )))
            .await
            .unwrap();

        let council_registry = Arc::new(InMemoryCouncilRegistry::new());
        council_registry
            .register(
                Council::new(
                    CouncilId::new("council-policy_gate").unwrap(),
                    specialty.clone(),
                    vec![agent_id],
                    SystemClock::new().now(),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let deliberate = Arc::new(DeliberateUseCase::new(
            Arc::new(SystemClock::new()),
            council_registry,
            agent_registry,
            vec![
                Arc::new(ContentNonEmptyValidator::new()),
                Arc::new(JsonObjectOutputValidator::new()),
                Arc::new(RequiredFieldsValidator::new()),
            ],
            Arc::new(UniformScoring::new()),
            Arc::new(InMemoryDeliberationRepository::new()),
            Arc::new(NoopMessaging::new()),
            Arc::new(InMemoryStatistics::new()),
            Arc::new(choreo_core::ports::NoopMetricsRecorder),
            "ceremony-step-handler-test",
        ));
        let handler = DeliberatingCeremonyStepHandler::new(deliberate);

        let request = CeremonyStepHandlerRequest::new(
            CeremonyId::new("ceremony-1").unwrap(),
            CeremonyName::new("editorial").unwrap(),
            CeremonyVersion::v1(),
            StateId::new("RULING").unwrap(),
            StepId::new("ruling").unwrap(),
            StepHandlerKind::new("policy_gate").unwrap(),
            StepHandlerConfig::new(
                Attributes::new(BTreeMap::from([
                    ("prompt".to_owned(), json!("Deliver the ruling")),
                    ("num_agents".to_owned(), json!(1)),
                    (
                        "output_contract".to_owned(),
                        json!({
                            "contract_id": "ruling-contract",
                            "required_fields": ["decision", "claims"],
                        }),
                    ),
                ]))
                .unwrap(),
            ),
            CeremonyContext::empty(),
            StepAttempt::FIRST,
        );

        let err = handler.execute(request).await.unwrap_err();

        assert!(matches!(
            err,
            choreo_core::error::DomainError::NoValidProposal { ref contract_id }
                if contract_id == "ruling-contract"
        ));
    }

    /// Stub agent that always answers with structured claims citing a
    /// reference that is NOT in the step's evidence pack — the exact
    /// failure mode the grounding gate exists for (a fluent, well-formed
    /// proposal built on fabricated evidence).
    #[derive(Debug)]
    struct FabricatingAgent {
        id: AgentId,
        specialty: Specialty,
    }

    #[async_trait::async_trait]
    impl choreo_core::ports::AgentPort for FabricatingAgent {
        fn id(&self) -> &AgentId {
            &self.id
        }

        fn specialty(&self) -> &Specialty {
            &self.specialty
        }

        async fn generate(
            &self,
            _request: choreo_core::ports::DraftRequest,
        ) -> Result<choreo_core::ports::Revision, DomainError> {
            Ok(choreo_core::ports::Revision {
                content: json!({
                    "claims": [
                        {
                            "text": "the pod spec sets privileged: true",
                            "evidence_refs": ["ev-fabricated"],
                        },
                    ],
                    "decision": "accept",
                })
                .to_string(),
            })
        }

        async fn critique(
            &self,
            _peer_content: &str,
            _constraints: &choreo_core::entities::TaskConstraints,
        ) -> Result<choreo_core::ports::Critique, DomainError> {
            Ok(choreo_core::ports::Critique {
                feedback: "looks fine".to_owned(),
            })
        }

        async fn revise(
            &self,
            own_content: &str,
            _critique: &choreo_core::ports::Critique,
        ) -> Result<choreo_core::ports::Revision, DomainError> {
            Ok(choreo_core::ports::Revision {
                content: own_content.to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn grounding_gate_rejects_fabricated_evidence_end_to_end() {
        let specialty = Specialty::new("evidence_gate").unwrap();
        let agent_id = AgentId::new("agent-evidence_gate-0").unwrap();
        let agent_registry = Arc::new(InMemoryAgentRegistry::new());
        agent_registry
            .insert(Arc::new(FabricatingAgent {
                id: agent_id.clone(),
                specialty: specialty.clone(),
            }))
            .await
            .unwrap();

        let council_registry = Arc::new(InMemoryCouncilRegistry::new());
        council_registry
            .register(
                Council::new(
                    CouncilId::new("council-evidence_gate").unwrap(),
                    specialty.clone(),
                    vec![agent_id],
                    SystemClock::new().now(),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let deliberate = Arc::new(DeliberateUseCase::new(
            Arc::new(SystemClock::new()),
            council_registry,
            agent_registry,
            vec![
                Arc::new(ContentNonEmptyValidator::new()),
                Arc::new(JsonObjectOutputValidator::new()),
                Arc::new(RequiredFieldsValidator::new()),
                Arc::new(crate::validators::ClaimsEvidenceGroundedValidator::new()),
            ],
            Arc::new(UniformScoring::new()),
            Arc::new(InMemoryDeliberationRepository::new()),
            Arc::new(NoopMessaging::new()),
            Arc::new(InMemoryStatistics::new()),
            Arc::new(choreo_core::ports::NoopMetricsRecorder),
            "ceremony-step-handler-test",
        ));
        let handler = DeliberatingCeremonyStepHandler::new(deliberate);

        // The evidence pack arrives through the ceremony context — the
        // deterministic collector's output — and the step contract
        // points its grounding rule at it.
        let request = CeremonyStepHandlerRequest::new(
            CeremonyId::new("ceremony-1").unwrap(),
            CeremonyName::new("evidence_review").unwrap(),
            CeremonyVersion::v1(),
            StateId::new("REVIEW").unwrap(),
            StepId::new("evidence_review").unwrap(),
            StepHandlerKind::new("evidence_gate").unwrap(),
            StepHandlerConfig::new(
                Attributes::new(BTreeMap::from([
                    ("prompt".to_owned(), json!("Review the change")),
                    ("num_agents".to_owned(), json!(1)),
                    (
                        "output_contract".to_owned(),
                        json!({
                            "contract_id": "evidence-bound-decision",
                            "required_fields": ["claims", "decision"],
                            "evidence": {
                                "allowed_refs_from_context": "evidence_pack",
                            },
                        }),
                    ),
                ]))
                .unwrap(),
            ),
            CeremonyContext::new(
                Attributes::new(BTreeMap::from([(
                    "evidence_pack".to_owned(),
                    json!([{"id": "ev-journal-1"}, {"id": "ev-trace-1"}]),
                )]))
                .unwrap(),
            ),
            StepAttempt::FIRST,
        );

        let err = handler.execute(request).await.unwrap_err();

        assert!(matches!(
            err,
            choreo_core::error::DomainError::NoValidProposal { ref contract_id }
                if contract_id == "evidence-bound-decision"
        ));
    }

    /// Support judge that refutes every claim — the failure mode the
    /// semantic gate exists for: a claim citing a *real* pack ref whose
    /// body does not say what the claim says.
    #[derive(Debug)]
    struct RefutingJudge;

    #[async_trait::async_trait]
    impl choreo_core::ports::EvidenceSupportJudgePort for RefutingJudge {
        async fn assess(
            &self,
            _claim_text: &str,
            _evidence: &[choreo_core::ports::EvidenceExcerpt],
        ) -> Result<choreo_core::ports::SupportVerdict, DomainError> {
            Ok(choreo_core::ports::SupportVerdict {
                supported: false,
                confidence: 95,
                rationale: "the cited excerpt does not state this".to_owned(),
            })
        }
    }

    /// Stub agent citing a *real* pack ref with a claim its body does
    /// not support: past the grounding gate, dead at the semantic gate.
    #[derive(Debug)]
    struct GroundedNonsenseAgent {
        id: AgentId,
        specialty: Specialty,
    }

    #[async_trait::async_trait]
    impl choreo_core::ports::AgentPort for GroundedNonsenseAgent {
        fn id(&self) -> &AgentId {
            &self.id
        }

        fn specialty(&self) -> &Specialty {
            &self.specialty
        }

        async fn generate(
            &self,
            _request: choreo_core::ports::DraftRequest,
        ) -> Result<choreo_core::ports::Revision, DomainError> {
            Ok(choreo_core::ports::Revision {
                content: json!({
                    "claims": [
                        {
                            "text": "the journal proves the pod ran privileged",
                            "evidence_refs": ["ev-journal-1"],
                        },
                    ],
                    "decision": "accept",
                })
                .to_string(),
            })
        }

        async fn critique(
            &self,
            _peer_content: &str,
            _constraints: &choreo_core::entities::TaskConstraints,
        ) -> Result<choreo_core::ports::Critique, DomainError> {
            Ok(choreo_core::ports::Critique {
                feedback: "looks fine".to_owned(),
            })
        }

        async fn revise(
            &self,
            own_content: &str,
            _critique: &choreo_core::ports::Critique,
        ) -> Result<choreo_core::ports::Revision, DomainError> {
            Ok(choreo_core::ports::Revision {
                content: own_content.to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn semantic_gate_rejects_grounded_but_unsupported_claims_end_to_end() {
        let specialty = Specialty::new("support_gate").unwrap();
        let agent_id = AgentId::new("agent-support_gate-0").unwrap();
        let agent_registry = Arc::new(InMemoryAgentRegistry::new());
        agent_registry
            .insert(Arc::new(GroundedNonsenseAgent {
                id: agent_id.clone(),
                specialty: specialty.clone(),
            }))
            .await
            .unwrap();

        let council_registry = Arc::new(InMemoryCouncilRegistry::new());
        council_registry
            .register(
                Council::new(
                    CouncilId::new("council-support_gate").unwrap(),
                    specialty.clone(),
                    vec![agent_id],
                    SystemClock::new().now(),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let deliberate = Arc::new(DeliberateUseCase::new(
            Arc::new(SystemClock::new()),
            council_registry,
            agent_registry,
            vec![
                Arc::new(ContentNonEmptyValidator::new()),
                Arc::new(JsonObjectOutputValidator::new()),
                Arc::new(RequiredFieldsValidator::new()),
                Arc::new(crate::validators::ClaimsEvidenceGroundedValidator::new()),
                Arc::new(crate::validators::ClaimsEvidenceSupportedValidator::new(
                    Some(Arc::new(RefutingJudge)),
                )),
            ],
            Arc::new(UniformScoring::new()),
            Arc::new(InMemoryDeliberationRepository::new()),
            Arc::new(NoopMessaging::new()),
            Arc::new(InMemoryStatistics::new()),
            Arc::new(choreo_core::ports::NoopMetricsRecorder),
            "ceremony-step-handler-test",
        ));
        let handler = DeliberatingCeremonyStepHandler::new(deliberate);

        let request = CeremonyStepHandlerRequest::new(
            CeremonyId::new("ceremony-1").unwrap(),
            CeremonyName::new("evidence_review").unwrap(),
            CeremonyVersion::v1(),
            StateId::new("REVIEW").unwrap(),
            StepId::new("evidence_review").unwrap(),
            StepHandlerKind::new("support_gate").unwrap(),
            StepHandlerConfig::new(
                Attributes::new(BTreeMap::from([
                    ("prompt".to_owned(), json!("Review the change")),
                    ("num_agents".to_owned(), json!(1)),
                    (
                        "output_contract".to_owned(),
                        json!({
                            "contract_id": "evidence-bound-decision",
                            "required_fields": ["claims", "decision"],
                            "evidence": {
                                "allowed_refs_from_context": "evidence_pack",
                                "semantic_support": { "min_confidence": 70 },
                            },
                        }),
                    ),
                ]))
                .unwrap(),
            ),
            CeremonyContext::new(
                Attributes::new(BTreeMap::from([(
                    "evidence_pack".to_owned(),
                    json!([
                        {"id": "ev-journal-1", "text": "typha (pid 4830) holds 0.0.0.0:5473"},
                        {"id": "ev-trace-1", "text": "trace shows a clean shutdown"},
                    ]),
                )]))
                .unwrap(),
            ),
            StepAttempt::FIRST,
        );

        let err = handler.execute(request).await.unwrap_err();

        // The claim is grounded (cites a real pack ref) yet the
        // refuting judge kills it — the proposal dies at the semantic
        // gate through the real deliberation pipeline.
        assert!(matches!(
            err,
            choreo_core::error::DomainError::NoValidProposal { ref contract_id }
                if contract_id == "evidence-bound-decision"
        ));
    }

    #[test]
    fn description_without_role_or_transcript_is_well_formed() {
        let request = request_with_prompt();
        let config = DeliberationStepConfig::from_request(&request).unwrap();

        let text = build_description(&request, &config).unwrap();
        let text = text.as_str();

        assert!(text.contains("participant in the \"editorial\" ceremony"));
        assert!(!text.contains("The meeting so far"));
        assert!(text.contains("Your task now: Open the meeting"));
    }

    #[test]
    fn description_frames_the_role_and_renders_prior_turns_as_prose() {
        let contribution = CeremonyStepContribution::new(
            StepId::new("open_room").unwrap(),
            RoleId::new("FACILITATOR").unwrap(),
            StepOutput::new(
                Attributes::new(BTreeMap::from([(
                    "winner_content".to_owned(),
                    json!("Restating the brief and inviting perspectives."),
                )]))
                .unwrap(),
            ),
        );
        let request = request_with_prompt()
            .with_role(RoleId::new("CUSTOMER_ADVOCATE").unwrap())
            .with_transcript(CeremonyTranscript::empty().appended(contribution));
        let config = DeliberationStepConfig::from_request(&request).unwrap();

        let text = build_description(&request, &config).unwrap();
        let text = text.as_str();

        assert!(text.contains("acting as CUSTOMER_ADVOCATE in the \"editorial\" ceremony"));
        assert!(text.contains("The meeting so far:"));
        assert!(text
            .contains("FACILITATOR (open_room): Restating the brief and inviting perspectives."));
        assert!(text.contains("Your task now: Open the meeting"));
    }

    #[test]
    fn description_renders_live_intervention_for_targeted_role() {
        let intervention = CeremonyIntervention::open(
            CeremonyInterventionId::new("inspect-queue").unwrap(),
            CeremonyInterventionKind::Investigation,
            RoleId::new("ENGINEER").unwrap(),
            CeremonyInterventionTarget::roles([RoleId::new("QUEUE_SPECIALIST").unwrap()]).unwrap(),
            CeremonyInterventionContent::new(
                "Inspect queue depth without consuming messages.",
                Attributes::new(BTreeMap::from([("queue".to_owned(), json!("orders"))])).unwrap(),
            )
            .unwrap(),
            time::OffsetDateTime::UNIX_EPOCH,
        );
        let request = request_with_prompt()
            .with_role(RoleId::new("QUEUE_SPECIALIST").unwrap())
            .with_interventions(vec![intervention]);
        let config = DeliberationStepConfig::from_request(&request).unwrap();

        let text = build_description(&request, &config).unwrap();
        let text = text.as_str();

        assert!(text.contains("Live participant requests:"));
        assert!(text.contains("[open] investigation requested by ENGINEER"));
        assert!(text.contains("Inspect queue depth without consuming messages."));
        assert!(text.contains("request details: {\"queue\":\"orders\"}"));
    }

    #[test]
    fn description_hides_scoped_intervention_from_unrelated_role() {
        let intervention = CeremonyIntervention::open(
            CeremonyInterventionId::new("inspect-database").unwrap(),
            CeremonyInterventionKind::Investigation,
            RoleId::new("ENGINEER").unwrap(),
            CeremonyInterventionTarget::roles([RoleId::new("DATABASE_SPECIALIST").unwrap()])
                .unwrap(),
            CeremonyInterventionContent::new("Inspect connection saturation.", Attributes::empty())
                .unwrap(),
            time::OffsetDateTime::UNIX_EPOCH,
        );
        let request = request_with_prompt()
            .with_role(RoleId::new("QUEUE_SPECIALIST").unwrap())
            .with_interventions(vec![intervention]);
        let config = DeliberationStepConfig::from_request(&request).unwrap();

        let text = build_description(&request, &config).unwrap();

        assert!(!text.as_str().contains("Inspect connection saturation."));
    }

    #[test]
    fn render_brief_lists_inputs_and_skips_boolean_flags() {
        let attributes = Attributes::new(BTreeMap::from([
            (
                "mission_brief".to_owned(),
                json!("Photograph trees that show signs of disease."),
            ),
            ("audience_notes".to_owned(), json!("Field foresters")),
            ("human_approved".to_owned(), json!(true)),
        ]))
        .unwrap();

        let brief = render_brief(&attributes);

        assert!(brief.contains("- mission_brief: Photograph trees that show signs of disease."));
        assert!(brief.contains("- audience_notes: Field foresters"));
        assert!(!brief.contains("human_approved"));
    }

    fn request_with_prompt() -> CeremonyStepHandlerRequest {
        CeremonyStepHandlerRequest::new(
            CeremonyId::new("ceremony-1").unwrap(),
            CeremonyName::new("editorial").unwrap(),
            CeremonyVersion::v1(),
            StateId::new("OPENING").unwrap(),
            StepId::new("open_room").unwrap(),
            StepHandlerKind::new("facilitation_prompt").unwrap(),
            StepHandlerConfig::new(
                Attributes::new(BTreeMap::from([
                    ("prompt".to_owned(), json!("Open the meeting")),
                    ("num_agents".to_owned(), json!(1)),
                ]))
                .unwrap(),
            ),
            CeremonyContext::empty(),
            StepAttempt::FIRST,
        )
    }
}
