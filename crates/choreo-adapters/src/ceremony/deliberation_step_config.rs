use choreo_core::error::DomainError;
use choreo_core::ports::CeremonyStepHandlerRequest;
use choreo_core::value_objects::{
    EvidenceGroundingRule, NumAgents, OutputContract, Rounds, SemanticSupportRule, Specialty,
    TaskDescription, DEFAULT_SUPPORT_MIN_CONFIDENCE,
};
use serde_json::Value;

use super::CeremonyStepConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliberationStepConfig {
    specialty: Specialty,
    task_description: TaskDescription,
    rounds: Rounds,
    num_agents: Option<NumAgents>,
    see_prior: bool,
    output_contract: Option<OutputContract>,
}

impl DeliberationStepConfig {
    pub fn from_request(request: &CeremonyStepHandlerRequest) -> Result<Self, DomainError> {
        let config = CeremonyStepConfig::new(
            request.handler_config().attributes(),
            request.handler_kind(),
        );

        let mut output_contract = config.output_contract()?;
        // Resolve the evidence-grounding declaration here: the spec may
        // point at a ceremony-context key, and only the transport layer
        // holds the context. The resolved rule rides on the contract so
        // the grounding validator sees it through `TaskConstraints`.
        if let Some(spec) = config.evidence_grounding_spec()? {
            let contract = output_contract.take().ok_or(DomainError::EmptyField {
                // `evidence` nests inside `output_contract`; reaching
                // here without a contract is a parse invariant breach.
                field: "ceremony_step.config.output_contract",
            })?;
            let mut refs = spec.static_refs;
            if let Some(key) = spec.context_key.as_deref() {
                refs.extend(context_refs(request, key)?);
            }
            let mut rule = EvidenceGroundingRule::new(spec.claims_field, spec.refs_field, refs)?;
            // Resolve the semantic-support declaration the same way:
            // the bodies live in the ceremony context, and a support
            // gate whose bodies do not resolve fails loudly at wiring
            // time instead of judging on nothing.
            if let Some(semantic) = spec.semantic {
                let bodies_key = semantic.bodies_context_key.or(spec.context_key).ok_or(
                    DomainError::EmptyField {
                        field: SEMANTIC_BODIES_FIELD,
                    },
                )?;
                let bodies = context_bodies(request, &bodies_key)?;
                let support = SemanticSupportRule::new(
                    semantic
                        .min_confidence
                        .unwrap_or(DEFAULT_SUPPORT_MIN_CONFIDENCE),
                    bodies,
                )?;
                rule = rule.with_semantic_support(support)?;
            }
            output_contract = Some(contract.with_evidence_grounding(rule));
        }

        Ok(Self {
            task_description: config.prompt()?,
            specialty: config.specialty()?,
            rounds: config.rounds()?,
            num_agents: config.num_agents()?,
            see_prior: config.see_prior_steps()?,
            output_contract,
        })
    }

    #[must_use]
    pub fn specialty(&self) -> &Specialty {
        &self.specialty
    }

    #[must_use]
    pub fn task_description(&self) -> &TaskDescription {
        &self.task_description
    }

    #[must_use]
    pub fn rounds(&self) -> Rounds {
        self.rounds
    }

    #[must_use]
    pub fn num_agents(&self) -> Option<NumAgents> {
        self.num_agents
    }

    /// Whether the step deliberates with the prior ceremony transcript in
    /// view (defaults to `true`).
    #[must_use]
    pub fn see_prior(&self) -> bool {
        self.see_prior
    }

    /// Structured output contract the step's proposals must satisfy, if
    /// the step declared one.
    #[must_use]
    pub fn output_contract(&self) -> Option<&OutputContract> {
        self.output_contract.as_ref()
    }
}

/// Stable error field for a malformed or missing context evidence pack.
const CONTEXT_REFS_FIELD: &str =
    "ceremony_step.config.output_contract.evidence.allowed_refs_from_context";

/// Stable error field for a malformed or missing context bodies pack.
const SEMANTIC_BODIES_FIELD: &str =
    "ceremony_step.config.output_contract.evidence.semantic_support.bodies_from_context";

/// Resolve an evidence pack from the ceremony context. The entry must
/// be an array of strings, or of objects each carrying a string `id`
/// (the natural shape of an evidence pack: `[{id, kind, uri, …}, …]`).
/// A grounding gate pointing at an absent or malformed pack fails
/// loudly — running the step ungrounded would silently void the
/// policy.
fn context_refs(
    request: &CeremonyStepHandlerRequest,
    key: &str,
) -> Result<Vec<String>, DomainError> {
    let Some(value) = request.context().attributes().get(key) else {
        return Err(DomainError::EmptyField {
            field: CONTEXT_REFS_FIELD,
        });
    };
    let Some(items) = value.as_array() else {
        return Err(DomainError::InvalidCharacters {
            field: CONTEXT_REFS_FIELD,
        });
    };
    items
        .iter()
        .map(|item| match item {
            Value::String(reference) => Ok(reference.clone()),
            Value::Object(entry) => entry
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(DomainError::InvalidCharacters {
                    field: CONTEXT_REFS_FIELD,
                }),
            _ => Err(DomainError::InvalidCharacters {
                field: CONTEXT_REFS_FIELD,
            }),
        })
        .collect()
}

/// Resolve evidence bodies from the ceremony context. The entry must
/// be an array of objects each carrying a string `id` and a string
/// `text` (the same pack shape [`context_refs`] reads ids from —
/// plain-string entries carry no body and make a semantic-support
/// gate fail loudly, because a gate that cannot read its evidence
/// would be judging on nothing).
fn context_bodies(
    request: &CeremonyStepHandlerRequest,
    key: &str,
) -> Result<Vec<(String, String)>, DomainError> {
    let Some(value) = request.context().attributes().get(key) else {
        return Err(DomainError::EmptyField {
            field: SEMANTIC_BODIES_FIELD,
        });
    };
    let Some(items) = value.as_array() else {
        return Err(DomainError::InvalidCharacters {
            field: SEMANTIC_BODIES_FIELD,
        });
    };
    items
        .iter()
        .map(|item| {
            let entry = item.as_object().ok_or(DomainError::InvalidCharacters {
                field: SEMANTIC_BODIES_FIELD,
            })?;
            let id =
                entry
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or(DomainError::InvalidCharacters {
                        field: SEMANTIC_BODIES_FIELD,
                    })?;
            let text = entry.get("text").and_then(Value::as_str).ok_or(
                DomainError::InvalidCharacters {
                    field: SEMANTIC_BODIES_FIELD,
                },
            )?;
            Ok((id.to_owned(), text.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use choreo_core::value_objects::{
        Attributes, CeremonyContext, CeremonyId, CeremonyName, CeremonyVersion, StateId,
        StepAttempt, StepHandlerConfig, StepHandlerKind, StepId,
    };
    use serde_json::{json, Value};

    use super::*;

    fn request(config: BTreeMap<String, Value>) -> CeremonyStepHandlerRequest {
        CeremonyStepHandlerRequest::new(
            CeremonyId::new("ceremony-1").unwrap(),
            CeremonyName::new("editorial").unwrap(),
            CeremonyVersion::v1(),
            StateId::new("OPENING").unwrap(),
            StepId::new("open_room").unwrap(),
            StepHandlerKind::new("facilitation_prompt").unwrap(),
            StepHandlerConfig::new(Attributes::new(config).unwrap()),
            CeremonyContext::empty(),
            StepAttempt::FIRST,
        )
    }

    #[test]
    fn defaults_specialty_to_handler_kind() {
        let config = DeliberationStepConfig::from_request(&request(BTreeMap::from([(
            "prompt".to_owned(),
            json!("Open the meeting"),
        )])))
        .unwrap();

        assert_eq!(config.specialty().as_str(), "facilitation_prompt");
        assert_eq!(config.task_description().as_str(), "Open the meeting");
        assert_eq!(config.rounds().get(), 1);
        assert!(config.num_agents().is_none());
    }

    #[test]
    fn accepts_explicit_specialty_and_bounds() {
        let config = DeliberationStepConfig::from_request(&request(BTreeMap::from([
            ("prompt".to_owned(), json!("Open the meeting")),
            ("specialty".to_owned(), json!("facilitator")),
            ("rounds".to_owned(), json!(0)),
            ("num_agents".to_owned(), json!(1)),
        ])))
        .unwrap();

        assert_eq!(config.specialty().as_str(), "facilitator");
        assert_eq!(config.rounds().get(), 0);
        assert_eq!(config.num_agents().unwrap().get(), 1);
    }

    #[test]
    fn rejects_missing_prompt() {
        let err = DeliberationStepConfig::from_request(&request(BTreeMap::new())).unwrap_err();

        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "ceremony_step.config.prompt"
            }
        ));
    }

    #[test]
    fn carries_declared_output_contract() {
        let config = DeliberationStepConfig::from_request(&request(BTreeMap::from([
            ("prompt".to_owned(), json!("Decide")),
            (
                "output_contract".to_owned(),
                json!({
                    "contract_id": "evidence-bound-decision",
                    "required_fields": ["decision"],
                }),
            ),
        ])))
        .unwrap();

        let contract = config.output_contract().unwrap();
        assert_eq!(contract.contract_id(), "evidence-bound-decision");
        assert!(contract.fields()["decision"].required());
    }

    fn request_with_context(
        config: BTreeMap<String, Value>,
        context: BTreeMap<String, Value>,
    ) -> CeremonyStepHandlerRequest {
        CeremonyStepHandlerRequest::new(
            CeremonyId::new("ceremony-1").unwrap(),
            CeremonyName::new("editorial").unwrap(),
            CeremonyVersion::v1(),
            StateId::new("OPENING").unwrap(),
            StepId::new("open_room").unwrap(),
            StepHandlerKind::new("facilitation_prompt").unwrap(),
            StepHandlerConfig::new(Attributes::new(config).unwrap()),
            CeremonyContext::new(Attributes::new(context).unwrap()),
            StepAttempt::FIRST,
        )
    }

    fn evidence_contract_config(evidence: &Value) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("prompt".to_owned(), json!("Decide with evidence")),
            (
                "output_contract".to_owned(),
                json!({
                    "contract_id": "evidence-bound-decision",
                    "required_fields": ["claims"],
                    "evidence": evidence,
                }),
            ),
        ])
    }

    #[test]
    fn resolves_evidence_pack_from_ceremony_context() {
        let config = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "allowed_refs_from_context": "evidence_pack",
            })),
            BTreeMap::from([(
                "evidence_pack".to_owned(),
                json!([
                    {"id": "ev-trace-1", "kind": "trace"},
                    {"id": "ev-metric-1", "kind": "metric"},
                ]),
            )]),
        ))
        .unwrap();

        let rule = config
            .output_contract()
            .unwrap()
            .evidence_grounding()
            .unwrap();
        assert_eq!(rule.claims_field(), "claims");
        assert_eq!(rule.refs_field(), "evidence_refs");
        assert!(rule.allowed_refs().contains("ev-trace-1"));
        assert!(rule.allowed_refs().contains("ev-metric-1"));
    }

    #[test]
    fn merges_static_refs_with_context_refs_and_custom_fields() {
        let config = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "claims_field": "findings",
                "refs_field": "sources",
                "allowed_refs": ["ev-static-1"],
                "allowed_refs_from_context": "evidence_pack",
            })),
            BTreeMap::from([("evidence_pack".to_owned(), json!(["ev-ctx-1"]))]),
        ))
        .unwrap();

        let rule = config
            .output_contract()
            .unwrap()
            .evidence_grounding()
            .unwrap();
        assert_eq!(rule.claims_field(), "findings");
        assert_eq!(rule.refs_field(), "sources");
        assert!(rule.allowed_refs().contains("ev-static-1"));
        assert!(rule.allowed_refs().contains("ev-ctx-1"));
    }

    #[test]
    fn resolves_semantic_support_bodies_from_ceremony_context() {
        let config = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "allowed_refs_from_context": "evidence_pack",
                "semantic_support": { "min_confidence": 80 },
            })),
            BTreeMap::from([(
                "evidence_pack".to_owned(),
                json!([
                    {"id": "ev-trace-1", "kind": "trace", "text": "typha holds 0.0.0.0:5473"},
                    {"id": "ev-metric-1", "kind": "metric", "text": "gauge went to zero"},
                ]),
            )]),
        ))
        .unwrap();

        let support = config
            .output_contract()
            .unwrap()
            .evidence_grounding()
            .unwrap()
            .semantic_support()
            .unwrap();
        assert_eq!(support.min_confidence(), 80);
        assert_eq!(support.body("ev-trace-1"), Some("typha holds 0.0.0.0:5473"));
        assert_eq!(support.body("ev-metric-1"), Some("gauge went to zero"));
    }

    #[test]
    fn semantic_support_defaults_min_confidence() {
        let config = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "allowed_refs_from_context": "evidence_pack",
                "semantic_support": {},
            })),
            BTreeMap::from([(
                "evidence_pack".to_owned(),
                json!([{"id": "ev-1", "text": "body"}]),
            )]),
        ))
        .unwrap();

        let support = config
            .output_contract()
            .unwrap()
            .evidence_grounding()
            .unwrap()
            .semantic_support()
            .unwrap();
        assert_eq!(support.min_confidence(), DEFAULT_SUPPORT_MIN_CONFIDENCE);
    }

    #[test]
    fn semantic_support_fails_loudly_when_entries_carry_no_text() {
        let err = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "allowed_refs_from_context": "evidence_pack",
                "semantic_support": {},
            })),
            BTreeMap::from([(
                "evidence_pack".to_owned(),
                json!([{"id": "ev-1", "kind": "trace"}]),
            )]),
        ))
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::InvalidCharacters {
                field: "ceremony_step.config.output_contract.evidence.semantic_support.bodies_from_context"
            }
        ));
    }

    #[test]
    fn semantic_support_fails_loudly_when_a_static_ref_has_no_body() {
        // Static refs have no context entry to read a body from — the
        // coverage check in the domain rule must reject the wiring.
        let err = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "allowed_refs": ["ev-static-1"],
                "allowed_refs_from_context": "evidence_pack",
                "semantic_support": {},
            })),
            BTreeMap::from([(
                "evidence_pack".to_owned(),
                json!([{"id": "ev-1", "text": "body"}]),
            )]),
        ))
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "output_contract.evidence.semantic_support.bodies"
            }
        ));
    }

    #[test]
    fn semantic_support_with_unknown_key_is_rejected() {
        let err = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "allowed_refs_from_context": "evidence_pack",
                "semantic_support": { "min_confidnce": 80 },
            })),
            BTreeMap::from([(
                "evidence_pack".to_owned(),
                json!([{"id": "ev-1", "text": "body"}]),
            )]),
        ))
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::InvalidCharacters {
                field: "ceremony_step.config.output_contract.evidence.semantic_support"
            }
        ));
    }

    #[test]
    fn semantic_support_reads_bodies_from_a_dedicated_context_key() {
        let config = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "allowed_refs": ["ev-1"],
                "semantic_support": { "bodies_from_context": "evidence_bodies" },
            })),
            BTreeMap::from([(
                "evidence_bodies".to_owned(),
                json!([{"id": "ev-1", "text": "the excerpt"}]),
            )]),
        ))
        .unwrap();

        let support = config
            .output_contract()
            .unwrap()
            .evidence_grounding()
            .unwrap()
            .semantic_support()
            .unwrap();
        assert_eq!(support.body("ev-1"), Some("the excerpt"));
    }

    #[test]
    fn grounding_gate_fails_loudly_when_context_pack_is_absent() {
        let err = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "allowed_refs_from_context": "evidence_pack",
            })),
            BTreeMap::new(),
        ))
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "ceremony_step.config.output_contract.evidence.allowed_refs_from_context"
            }
        ));
    }

    #[test]
    fn evidence_block_without_any_source_is_rejected() {
        let err = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({ "claims_field": "claims" })),
            BTreeMap::new(),
        ))
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "ceremony_step.config.output_contract.evidence.allowed_refs"
            }
        ));
    }

    #[test]
    fn evidence_block_with_unknown_key_is_rejected() {
        let err = DeliberationStepConfig::from_request(&request_with_context(
            evidence_contract_config(&json!({
                "allowed_refs": ["ev-1"],
                "allowd_refs": ["typo"],
            })),
            BTreeMap::new(),
        ))
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::InvalidCharacters {
                field: "ceremony_step.config.output_contract.evidence"
            }
        ));
    }

    #[test]
    fn output_contract_defaults_to_none() {
        let config = DeliberationStepConfig::from_request(&request(BTreeMap::from([(
            "prompt".to_owned(),
            json!("Decide"),
        )])))
        .unwrap();

        assert!(config.output_contract().is_none());
    }
}
