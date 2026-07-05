//! [`CeremonyStepConfig`] — typed accessor over a ceremony step's handler
//! configuration.
//!
//! Ceremony step handlers are configured through the free-form
//! [`Attributes`] bag carried by `StepHandlerConfig`. Reading that bag
//! with scattered string literals is primitive-obsessed and error-prone,
//! so this view owns the configuration key schema in exactly one place
//! and exposes a validated, fail-fast typed value for each field. Both
//! the deliberation step handler and the participant planner read the
//! step configuration through it, which keeps the schema — including the
//! canonical-vs-legacy agent-kind key — consistent across the adapter.

use std::collections::BTreeMap;

use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    AgentKind, Attributes, NumAgents, OutputContract, OutputFieldRule, OutputFormat, Rounds,
    Specialty, StepHandlerKind, TaskDescription,
};
use serde_json::Value;

/// Agent kind assumed when a step does not declare one.
const DEFAULT_AGENT_KIND: &str = "noop";

/// Configuration keys recognised on a ceremony step's handler config.
mod key {
    pub(super) const PROMPT: &str = "prompt";
    pub(super) const SPECIALTY: &str = "specialty";
    pub(super) const ROUNDS: &str = "rounds";
    pub(super) const NUM_AGENTS: &str = "num_agents";
    /// Canonical agent-kind key.
    pub(super) const AGENT_KIND: &str = "agent_kind";
    /// Legacy agent-kind key, still accepted for backwards compatibility.
    pub(super) const AGENT_KIND_LEGACY: &str = "agent.kind";
    pub(super) const PARTICIPANTS: &str = "participants";
    /// Whether the step deliberates with the prior transcript in view.
    pub(super) const SEE_PRIOR: &str = "see_prior";
    /// Structured output contract enforced on the step's proposals.
    pub(super) const OUTPUT_CONTRACT: &str = "output_contract";
}

/// Keys recognised inside the `output_contract` block.
mod contract_key {
    pub(super) const CONTRACT_ID: &str = "contract_id";
    pub(super) const FORMAT: &str = "format";
    pub(super) const REQUIRED_FIELDS: &str = "required_fields";
    pub(super) const ALLOWED_VALUES: &str = "allowed_values";
    pub(super) const JSON_SCHEMA: &str = "json_schema";
    pub(super) const EVIDENCE: &str = "evidence";
}

/// Keys recognised inside the `output_contract.evidence` block.
mod evidence_key {
    pub(super) const CLAIMS_FIELD: &str = "claims_field";
    pub(super) const REFS_FIELD: &str = "refs_field";
    pub(super) const ALLOWED_REFS: &str = "allowed_refs";
    pub(super) const ALLOWED_REFS_FROM_CONTEXT: &str = "allowed_refs_from_context";
    pub(super) const SEMANTIC_SUPPORT: &str = "semantic_support";
}

/// Keys recognised inside the `output_contract.evidence.semantic_support`
/// block.
mod semantic_key {
    pub(super) const MIN_CONFIDENCE: &str = "min_confidence";
    pub(super) const BODIES_FROM_CONTEXT: &str = "bodies_from_context";
}

/// Stable `DomainError` field names surfaced when a value is malformed.
mod field {
    pub(super) const PROMPT: &str = "ceremony_step.config.prompt";
    pub(super) const ROUNDS: &str = "ceremony_step.config.rounds";
    pub(super) const NUM_AGENTS: &str = "ceremony_step.config.num_agents";
    pub(super) const PARTICIPANTS: &str = "ceremony_step.config.participants";
    pub(super) const SEE_PRIOR: &str = "ceremony_step.config.see_prior";
    pub(super) const OUTPUT_CONTRACT: &str = "ceremony_step.config.output_contract";
    pub(super) const CONTRACT_ID: &str = "ceremony_step.config.output_contract.contract_id";
    pub(super) const CONTRACT_FORMAT: &str = "ceremony_step.config.output_contract.format";
    pub(super) const CONTRACT_REQUIRED_FIELDS: &str =
        "ceremony_step.config.output_contract.required_fields";
    pub(super) const CONTRACT_ALLOWED_VALUES: &str =
        "ceremony_step.config.output_contract.allowed_values";
    pub(super) const CONTRACT_JSON_SCHEMA: &str =
        "ceremony_step.config.output_contract.json_schema";
    pub(super) const CONTRACT_EVIDENCE: &str = "ceremony_step.config.output_contract.evidence";
    pub(super) const EVIDENCE_CLAIMS_FIELD: &str =
        "ceremony_step.config.output_contract.evidence.claims_field";
    pub(super) const EVIDENCE_REFS_FIELD: &str =
        "ceremony_step.config.output_contract.evidence.refs_field";
    pub(super) const EVIDENCE_ALLOWED_REFS: &str =
        "ceremony_step.config.output_contract.evidence.allowed_refs";
    pub(super) const EVIDENCE_CONTEXT_KEY: &str =
        "ceremony_step.config.output_contract.evidence.allowed_refs_from_context";
    pub(super) const EVIDENCE_SEMANTIC: &str =
        "ceremony_step.config.output_contract.evidence.semantic_support";
    pub(super) const SEMANTIC_MIN_CONFIDENCE: &str =
        "ceremony_step.config.output_contract.evidence.semantic_support.min_confidence";
    pub(super) const SEMANTIC_BODIES_KEY: &str =
        "ceremony_step.config.output_contract.evidence.semantic_support.bodies_from_context";
}

/// Default output field carrying the claims array.
const DEFAULT_CLAIMS_FIELD: &str = "claims";
/// Default per-claim field carrying the evidence references.
const DEFAULT_REFS_FIELD: &str = "evidence_refs";

/// Declared evidence-grounding configuration for a step, before the
/// context-borne refs are resolved. The step config owns the schema;
/// the transport layer — which holds the ceremony context — resolves
/// `context_key` into concrete refs and builds the domain rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvidenceGroundingSpec {
    pub(crate) claims_field: String,
    pub(crate) refs_field: String,
    pub(crate) static_refs: Vec<String>,
    pub(crate) context_key: Option<String>,
    pub(crate) semantic: Option<SemanticSupportSpec>,
}

/// Declared semantic-support configuration for a step, before the
/// context-borne evidence bodies are resolved. `bodies_context_key`
/// defaults to the grounding spec's `context_key` when absent — the
/// pack that names the refs usually also carries their text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticSupportSpec {
    pub(crate) min_confidence: Option<u8>,
    pub(crate) bodies_context_key: Option<String>,
}

/// A validated, typed view over one ceremony step's handler configuration.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CeremonyStepConfig<'a> {
    attributes: &'a Attributes,
    handler_kind: &'a StepHandlerKind,
}

impl<'a> CeremonyStepConfig<'a> {
    /// Wrap a step's handler-config attributes together with the handler
    /// kind that backs the specialty default.
    pub(crate) fn new(attributes: &'a Attributes, handler_kind: &'a StepHandlerKind) -> Self {
        Self {
            attributes,
            handler_kind,
        }
    }

    /// Required free-text prompt the step deliberates over.
    pub(crate) fn prompt(&self) -> Result<TaskDescription, DomainError> {
        TaskDescription::new(required_string(
            self.attributes.get(key::PROMPT),
            field::PROMPT,
        )?)
    }

    /// Specialty the step deliberates under, defaulting to the handler kind.
    pub(crate) fn specialty(&self) -> Result<Specialty, DomainError> {
        Specialty::new(
            optional_string(self.attributes.get(key::SPECIALTY))
                .unwrap_or_else(|| self.handler_kind.as_str()),
        )
    }

    /// Number of deliberation rounds; the domain default when unset.
    pub(crate) fn rounds(&self) -> Result<Rounds, DomainError> {
        match optional_u32(self.attributes.get(key::ROUNDS), field::ROUNDS)? {
            Some(value) => Rounds::new(value),
            None => Ok(Rounds::default()),
        }
    }

    /// Explicit agent count, if the step declares one.
    pub(crate) fn num_agents(&self) -> Result<Option<NumAgents>, DomainError> {
        optional_u32(self.attributes.get(key::NUM_AGENTS), field::NUM_AGENTS)?
            .map(NumAgents::new)
            .transpose()
    }

    /// Agent kind, resolving the canonical `agent_kind` first, then the
    /// legacy `agent.kind`, and defaulting to noop.
    pub(crate) fn agent_kind(&self) -> Result<AgentKind, DomainError> {
        AgentKind::new(
            optional_string(self.attributes.get(key::AGENT_KIND))
                .or_else(|| optional_string(self.attributes.get(key::AGENT_KIND_LEGACY)))
                .unwrap_or(DEFAULT_AGENT_KIND),
        )
    }

    /// Participant labels declared on the step, in order (possibly empty).
    pub(crate) fn participant_labels(&self) -> Result<Vec<String>, DomainError> {
        let Some(value) = self.attributes.get(key::PARTICIPANTS) else {
            return Ok(Vec::new());
        };
        if value.is_null() {
            return Ok(Vec::new());
        }
        let Some(items) = value.as_array() else {
            return Err(DomainError::InvalidCharacters {
                field: field::PARTICIPANTS,
            });
        };
        items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::trim)
                    .filter(|label| !label.is_empty())
                    .map(str::to_owned)
                    .ok_or(DomainError::InvalidCharacters {
                        field: field::PARTICIPANTS,
                    })
            })
            .collect()
    }

    /// Whether the step should deliberate with the prior transcript in
    /// view. Defaults to `true`: a ceremony is a conversation, so a step
    /// builds on what came before unless it is explicitly made blind
    /// (for example, independent estimates before a reveal).
    pub(crate) fn see_prior_steps(&self) -> Result<bool, DomainError> {
        Ok(optional_bool(self.attributes.get(key::SEE_PRIOR), field::SEE_PRIOR)?.unwrap_or(true))
    }

    /// Structured output contract declared on the step, if any.
    ///
    /// Shape (all under the step's `config`):
    ///
    /// ```yaml
    /// output_contract:
    ///   contract_id: evidence-bound-decision   # required
    ///   format: json_object                    # optional; only value today
    ///   required_fields: [claims, decision]    # optional
    ///   allowed_values:                        # optional, per field
    ///     decision: [accept, reject, request_changes, request_more_evidence]
    ///   json_schema: '{"type":"object"}'       # optional, inline body
    /// ```
    ///
    /// `required_fields` and `allowed_values` merge into per-field rules
    /// (a field named only under `allowed_values` is constrained but not
    /// required). Unknown keys inside the block are rejected: a typo in a
    /// policy gate must fail the parse, not silently weaken the contract.
    /// Enforcement itself is the existing deliberation pipeline — the
    /// contract validators run per proposal, and when no proposal
    /// satisfies the contract the step fails with
    /// `NoValidProposal { contract_id }` (the ceremony retries per its
    /// retry policy and otherwise stops at the guard).
    pub(crate) fn output_contract(&self) -> Result<Option<OutputContract>, DomainError> {
        let Some(value) = self.attributes.get(key::OUTPUT_CONTRACT) else {
            return Ok(None);
        };
        if value.is_null() {
            return Ok(None);
        }
        let Some(block) = value.as_object() else {
            return Err(DomainError::InvalidCharacters {
                field: field::OUTPUT_CONTRACT,
            });
        };

        let known = [
            contract_key::CONTRACT_ID,
            contract_key::FORMAT,
            contract_key::REQUIRED_FIELDS,
            contract_key::ALLOWED_VALUES,
            contract_key::JSON_SCHEMA,
            contract_key::EVIDENCE,
        ];
        if block.keys().any(|key| !known.contains(&key.as_str())) {
            return Err(DomainError::InvalidCharacters {
                field: field::OUTPUT_CONTRACT,
            });
        }

        let contract_id =
            required_string(block.get(contract_key::CONTRACT_ID), field::CONTRACT_ID)?;

        let format = match optional_string(block.get(contract_key::FORMAT)) {
            None | Some("json_object") => OutputFormat::JsonObject,
            Some(_) => {
                return Err(DomainError::InvalidCharacters {
                    field: field::CONTRACT_FORMAT,
                })
            }
        };

        let required = string_array(
            block.get(contract_key::REQUIRED_FIELDS),
            field::CONTRACT_REQUIRED_FIELDS,
        )?;

        let mut allowed: BTreeMap<String, Vec<String>> = BTreeMap::new();
        if let Some(value) = block.get(contract_key::ALLOWED_VALUES) {
            if !value.is_null() {
                let Some(map) = value.as_object() else {
                    return Err(DomainError::InvalidCharacters {
                        field: field::CONTRACT_ALLOWED_VALUES,
                    });
                };
                for (field_name, values) in map {
                    allowed.insert(
                        field_name.clone(),
                        string_array(Some(values), field::CONTRACT_ALLOWED_VALUES)?,
                    );
                }
            }
        }

        let mut fields: BTreeMap<String, OutputFieldRule> = BTreeMap::new();
        for name in &required {
            let values = allowed.remove(name).unwrap_or_default();
            fields.insert(name.clone(), OutputFieldRule::new(true, values)?);
        }
        for (name, values) in allowed {
            fields.insert(name, OutputFieldRule::new(false, values)?);
        }

        let json_schema = optional_string(block.get(contract_key::JSON_SCHEMA)).unwrap_or_default();
        if let Some(value) = block.get(contract_key::JSON_SCHEMA) {
            if !value.is_null() && !value.is_string() {
                return Err(DomainError::InvalidCharacters {
                    field: field::CONTRACT_JSON_SCHEMA,
                });
            }
        }

        Ok(Some(OutputContract::new_with_schema(
            contract_id,
            format,
            fields,
            json_schema,
        )?))
    }

    /// Evidence-grounding declaration inside the step's contract, if
    /// any.
    ///
    /// Shape (inside the `output_contract` block):
    ///
    /// ```yaml
    /// output_contract:
    ///   contract_id: evidence-bound-decision
    ///   evidence:
    ///     claims_field: claims                    # optional, default "claims"
    ///     refs_field: evidence_refs               # optional, default "evidence_refs"
    ///     allowed_refs: [ev-static-1]             # optional, static pack entries
    ///     allowed_refs_from_context: evidence_pack # optional, ceremony-context key
    ///     semantic_support:                        # optional, second gate
    ///       min_confidence: 70                     # optional, percent 0-100
    ///       bodies_from_context: evidence_pack     # optional, ceremony-context key
    /// ```
    ///
    /// At least one of `allowed_refs` / `allowed_refs_from_context` is
    /// required, unknown keys are rejected (same reasoning as the
    /// contract block: a typo must not silently weaken a policy gate).
    /// The context key is resolved by the transport layer at request
    /// time — the context entry must be an array of strings, or of
    /// objects each carrying a string `id` (the natural shape of an
    /// evidence pack).
    ///
    /// `semantic_support` (presence turns the gate on, `{}` is valid)
    /// additionally demands that every claim's cited evidence
    /// *supports* the claim, judged through the deployment's
    /// evidence-support judge. Its bodies resolve from
    /// `bodies_from_context` — defaulting to
    /// `allowed_refs_from_context` — whose entries must be objects
    /// carrying `id` and `text`.
    pub(crate) fn evidence_grounding_spec(
        &self,
    ) -> Result<Option<EvidenceGroundingSpec>, DomainError> {
        let Some(contract) = self.attributes.get(key::OUTPUT_CONTRACT) else {
            return Ok(None);
        };
        let Some(block) = contract.as_object() else {
            // output_contract() already rejects this shape; stay quiet here.
            return Ok(None);
        };
        let Some(value) = block.get(contract_key::EVIDENCE) else {
            return Ok(None);
        };
        if value.is_null() {
            return Ok(None);
        }
        let Some(evidence) = value.as_object() else {
            return Err(DomainError::InvalidCharacters {
                field: field::CONTRACT_EVIDENCE,
            });
        };

        let known = [
            evidence_key::CLAIMS_FIELD,
            evidence_key::REFS_FIELD,
            evidence_key::ALLOWED_REFS,
            evidence_key::ALLOWED_REFS_FROM_CONTEXT,
            evidence_key::SEMANTIC_SUPPORT,
        ];
        if evidence.keys().any(|key| !known.contains(&key.as_str())) {
            return Err(DomainError::InvalidCharacters {
                field: field::CONTRACT_EVIDENCE,
            });
        }

        if let Some(value) = evidence.get(evidence_key::CLAIMS_FIELD) {
            if !value.is_null() && !value.is_string() {
                return Err(DomainError::InvalidCharacters {
                    field: field::EVIDENCE_CLAIMS_FIELD,
                });
            }
        }
        if let Some(value) = evidence.get(evidence_key::REFS_FIELD) {
            if !value.is_null() && !value.is_string() {
                return Err(DomainError::InvalidCharacters {
                    field: field::EVIDENCE_REFS_FIELD,
                });
            }
        }
        let claims_field = optional_string(evidence.get(evidence_key::CLAIMS_FIELD))
            .unwrap_or(DEFAULT_CLAIMS_FIELD)
            .to_owned();
        let refs_field = optional_string(evidence.get(evidence_key::REFS_FIELD))
            .unwrap_or(DEFAULT_REFS_FIELD)
            .to_owned();

        let static_refs = string_array(
            evidence.get(evidence_key::ALLOWED_REFS),
            field::EVIDENCE_ALLOWED_REFS,
        )?;
        if let Some(value) = evidence.get(evidence_key::ALLOWED_REFS_FROM_CONTEXT) {
            if !value.is_null() && !value.is_string() {
                return Err(DomainError::InvalidCharacters {
                    field: field::EVIDENCE_CONTEXT_KEY,
                });
            }
        }
        let context_key = optional_string(evidence.get(evidence_key::ALLOWED_REFS_FROM_CONTEXT))
            .map(str::to_owned);

        if static_refs.is_empty() && context_key.is_none() {
            return Err(DomainError::EmptyField {
                field: field::EVIDENCE_ALLOWED_REFS,
            });
        }

        let semantic = semantic_support_spec(evidence.get(evidence_key::SEMANTIC_SUPPORT))?;

        Ok(Some(EvidenceGroundingSpec {
            claims_field,
            refs_field,
            static_refs,
            context_key,
            semantic,
        }))
    }
}

/// Parse the optional `semantic_support` block. Absent/null → `None`;
/// anything but an object with the recognised keys is rejected.
fn semantic_support_spec(
    value: Option<&Value>,
) -> Result<Option<SemanticSupportSpec>, DomainError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(block) = value.as_object() else {
        return Err(DomainError::InvalidCharacters {
            field: field::EVIDENCE_SEMANTIC,
        });
    };

    let known = [
        semantic_key::MIN_CONFIDENCE,
        semantic_key::BODIES_FROM_CONTEXT,
    ];
    if block.keys().any(|key| !known.contains(&key.as_str())) {
        return Err(DomainError::InvalidCharacters {
            field: field::EVIDENCE_SEMANTIC,
        });
    }

    let min_confidence = match block.get(semantic_key::MIN_CONFIDENCE) {
        None => None,
        Some(value) if value.is_null() => None,
        Some(value) => {
            let raw = value.as_u64().ok_or(DomainError::InvalidCharacters {
                field: field::SEMANTIC_MIN_CONFIDENCE,
            })?;
            let percent = u8::try_from(raw).map_err(|_| DomainError::OutOfRange {
                field: field::SEMANTIC_MIN_CONFIDENCE,
                value: raw as f64,
                min: 0.0,
                max: 100.0,
            })?;
            if percent > 100 {
                return Err(DomainError::OutOfRange {
                    field: field::SEMANTIC_MIN_CONFIDENCE,
                    value: f64::from(percent),
                    min: 0.0,
                    max: 100.0,
                });
            }
            Some(percent)
        }
    };

    if let Some(value) = block.get(semantic_key::BODIES_FROM_CONTEXT) {
        if !value.is_null() && !value.is_string() {
            return Err(DomainError::InvalidCharacters {
                field: field::SEMANTIC_BODIES_KEY,
            });
        }
    }
    let bodies_context_key =
        optional_string(block.get(semantic_key::BODIES_FROM_CONTEXT)).map(str::to_owned);

    Ok(Some(SemanticSupportSpec {
        min_confidence,
        bodies_context_key,
    }))
}

/// Parse a JSON value as a non-empty-string array (absent/null → empty).
fn string_array(value: Option<&Value>, field: &'static str) -> Result<Vec<String>, DomainError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let Some(items) = value.as_array() else {
        return Err(DomainError::InvalidCharacters { field });
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|raw| !raw.is_empty())
                .map(str::to_owned)
                .ok_or(DomainError::InvalidCharacters { field })
        })
        .collect()
}

fn required_string(value: Option<&Value>, field: &'static str) -> Result<String, DomainError> {
    let Some(value) = value else {
        return Err(DomainError::EmptyField { field });
    };
    let Some(raw) = value.as_str() else {
        return Err(DomainError::InvalidCharacters { field });
    };
    if raw.trim().is_empty() {
        return Err(DomainError::EmptyField { field });
    }
    Ok(raw.to_owned())
}

fn optional_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
}

fn optional_bool(value: Option<&Value>, field: &'static str) -> Result<Option<bool>, DomainError> {
    match value {
        None => Ok(None),
        Some(value) if value.is_null() => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or(DomainError::InvalidCharacters { field }),
    }
}

fn optional_u32(value: Option<&Value>, field: &'static str) -> Result<Option<u32>, DomainError> {
    let Some(value) = value else { return Ok(None) };
    if value.is_null() {
        return Ok(None);
    }
    let Some(raw) = value.as_u64() else {
        return Err(DomainError::InvalidCharacters { field });
    };
    u32::try_from(raw)
        .map(Some)
        .map_err(|_| DomainError::OutOfRange {
            field,
            value: raw as f64,
            min: 0.0,
            max: f64::from(u32::MAX),
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;

    fn config(map: BTreeMap<String, Value>) -> (Attributes, StepHandlerKind) {
        (
            Attributes::new(map).unwrap(),
            StepHandlerKind::new("facilitation_prompt").unwrap(),
        )
    }

    #[test]
    fn specialty_defaults_to_handler_kind() {
        let (attributes, handler_kind) = config(BTreeMap::new());
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert_eq!(step.specialty().unwrap().as_str(), "facilitation_prompt");
    }

    #[test]
    fn explicit_specialty_wins() {
        let (attributes, handler_kind) = config(BTreeMap::from([(
            "specialty".to_owned(),
            json!("facilitator"),
        )]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert_eq!(step.specialty().unwrap().as_str(), "facilitator");
    }

    #[test]
    fn rounds_default_is_one_when_unset() {
        let (attributes, handler_kind) = config(BTreeMap::new());
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert_eq!(step.rounds().unwrap().get(), 1);
    }

    #[test]
    fn agent_kind_prefers_canonical_key() {
        let (attributes, handler_kind) = config(BTreeMap::from([
            ("agent_kind".to_owned(), json!("vllm")),
            ("agent.kind".to_owned(), json!("openai")),
        ]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert_eq!(step.agent_kind().unwrap().as_str(), "vllm");
    }

    #[test]
    fn agent_kind_falls_back_to_legacy_key() {
        let (attributes, handler_kind) =
            config(BTreeMap::from([("agent.kind".to_owned(), json!("openai"))]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert_eq!(step.agent_kind().unwrap().as_str(), "openai");
    }

    #[test]
    fn agent_kind_defaults_to_noop() {
        let (attributes, handler_kind) = config(BTreeMap::new());
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert_eq!(step.agent_kind().unwrap().as_str(), "noop");
    }

    #[test]
    fn missing_prompt_is_rejected() {
        let (attributes, handler_kind) = config(BTreeMap::new());
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(matches!(
            step.prompt().unwrap_err(),
            DomainError::EmptyField {
                field: "ceremony_step.config.prompt"
            }
        ));
    }

    #[test]
    fn non_array_participants_are_rejected() {
        let (attributes, handler_kind) = config(BTreeMap::from([(
            "participants".to_owned(),
            json!("facilitator"),
        )]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(matches!(
            step.participant_labels().unwrap_err(),
            DomainError::InvalidCharacters {
                field: "ceremony_step.config.participants"
            }
        ));
    }

    #[test]
    fn see_prior_defaults_to_true() {
        let (attributes, handler_kind) = config(BTreeMap::new());
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(step.see_prior_steps().unwrap());
    }

    #[test]
    fn see_prior_can_be_disabled() {
        let (attributes, handler_kind) =
            config(BTreeMap::from([("see_prior".to_owned(), json!(false))]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(!step.see_prior_steps().unwrap());
    }

    #[test]
    fn non_bool_see_prior_is_rejected() {
        let (attributes, handler_kind) =
            config(BTreeMap::from([("see_prior".to_owned(), json!("yes"))]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(matches!(
            step.see_prior_steps().unwrap_err(),
            DomainError::InvalidCharacters {
                field: "ceremony_step.config.see_prior"
            }
        ));
    }

    #[test]
    fn output_contract_defaults_to_none() {
        let (attributes, handler_kind) = config(BTreeMap::new());
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(step.output_contract().unwrap().is_none());
    }

    #[test]
    fn output_contract_parses_full_shape() {
        let (attributes, handler_kind) = config(BTreeMap::from([(
            "output_contract".to_owned(),
            json!({
                "contract_id": "evidence-bound-decision",
                "format": "json_object",
                "required_fields": ["claims", "decision"],
                "allowed_values": {
                    "decision": ["accept", "reject", "request_changes"],
                    "confidence": ["high", "medium", "low"],
                },
                "json_schema": "{\"type\":\"object\"}",
            }),
        )]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        let contract = step.output_contract().unwrap().unwrap();

        assert_eq!(contract.contract_id(), "evidence-bound-decision");
        assert!(contract.fields()["claims"].required());
        assert!(contract.fields()["decision"].required());
        assert!(contract.fields()["decision"]
            .allowed_string_values()
            .contains("request_changes"));
        // Constrained but not required: named only under allowed_values.
        assert!(!contract.fields()["confidence"].required());
        assert_eq!(contract.json_schema(), "{\"type\":\"object\"}");
    }

    #[test]
    fn output_contract_requires_contract_id() {
        let (attributes, handler_kind) = config(BTreeMap::from([(
            "output_contract".to_owned(),
            json!({ "required_fields": ["decision"] }),
        )]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(matches!(
            step.output_contract().unwrap_err(),
            DomainError::EmptyField {
                field: "ceremony_step.config.output_contract.contract_id"
            }
        ));
    }

    #[test]
    fn output_contract_rejects_unknown_keys() {
        let (attributes, handler_kind) = config(BTreeMap::from([(
            "output_contract".to_owned(),
            json!({ "contract_id": "c1", "require_fields": ["decision"] }),
        )]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(matches!(
            step.output_contract().unwrap_err(),
            DomainError::InvalidCharacters {
                field: "ceremony_step.config.output_contract"
            }
        ));
    }

    #[test]
    fn output_contract_rejects_unknown_format() {
        let (attributes, handler_kind) = config(BTreeMap::from([(
            "output_contract".to_owned(),
            json!({ "contract_id": "c1", "format": "yaml" }),
        )]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(matches!(
            step.output_contract().unwrap_err(),
            DomainError::InvalidCharacters {
                field: "ceremony_step.config.output_contract.format"
            }
        ));
    }

    #[test]
    fn output_contract_rejects_non_object_block() {
        let (attributes, handler_kind) = config(BTreeMap::from([(
            "output_contract".to_owned(),
            json!("evidence-bound-decision"),
        )]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(matches!(
            step.output_contract().unwrap_err(),
            DomainError::InvalidCharacters {
                field: "ceremony_step.config.output_contract"
            }
        ));
    }

    #[test]
    fn output_contract_rejects_non_array_allowed_values() {
        let (attributes, handler_kind) = config(BTreeMap::from([(
            "output_contract".to_owned(),
            json!({ "contract_id": "c1", "allowed_values": { "decision": "accept" } }),
        )]));
        let step = CeremonyStepConfig::new(&attributes, &handler_kind);

        assert!(matches!(
            step.output_contract().unwrap_err(),
            DomainError::InvalidCharacters {
                field: "ceremony_step.config.output_contract.allowed_values"
            }
        ));
    }
}
