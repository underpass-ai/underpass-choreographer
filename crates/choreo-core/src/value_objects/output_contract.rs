//! Structured output contract for a council invocation.
//!
//! This is intentionally generic and domain-agnostic. It does not know
//! what a "decision", "report", or "event" means; it only describes
//! the shape that a proposal must satisfy when a caller requires a
//! structured output instead of free-form text.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_CONTRACT_ID_LEN: usize = 128;
const MAX_FIELDS: usize = 128;
const MAX_FIELD_NAME_LEN: usize = 128;
const MAX_ALLOWED_VALUES_PER_FIELD: usize = 128;
const MAX_ALLOWED_VALUE_LEN: usize = 256;
/// Cap on the embedded JSON Schema body. 256 KiB is enough for an
/// elaborate Report-shape schema with nested objects and several
/// dozen enums; anything larger should live behind a `$ref` and be
/// fetched by the validator if/when remote schemas are supported.
const MAX_JSON_SCHEMA_LEN: usize = 256 * 1024;
/// Cap on the evidence pack an evidence-grounding rule may carry. An
/// evidence pack is a curated set of reference ids for one
/// deliberation, not a corpus; anything larger belongs in an external
/// store that the pack entries reference.
const MAX_ALLOWED_EVIDENCE_REFS: usize = 1024;
/// Cap on one evidence body a semantic-support rule may carry. Bodies
/// are curated excerpts a judge reads per claim, not documents; a
/// larger source belongs in an external store, with the excerpt that
/// actually supports the claim quoted here.
const MAX_EVIDENCE_BODY_LEN: usize = 16 * 1024;
/// Default minimum confidence (percent) a support verdict must reach
/// before a claim counts as semantically supported.
pub const DEFAULT_SUPPORT_MIN_CONFIDENCE: u8 = 70;

/// Wire- and storage-stable structured output format selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputFormat {
    /// A single JSON object at the root.
    #[default]
    JsonObject,
}

impl OutputFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JsonObject => "json_object",
        }
    }
}

/// Validation rules for one named field in a structured output object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OutputFieldRule {
    required: bool,
    #[serde(default)]
    allowed_string_values: BTreeSet<String>,
}

impl OutputFieldRule {
    pub fn new(
        required: bool,
        allowed_string_values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, DomainError> {
        let values = allowed_string_values
            .into_iter()
            .map(|value| {
                let value = value.into();
                validate_text(
                    &value,
                    "output_contract.field.allowed_value",
                    MAX_ALLOWED_VALUE_LEN,
                )
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if values.len() > MAX_ALLOWED_VALUES_PER_FIELD {
            return Err(DomainError::OutOfRange {
                field: "output_contract.field.allowed_values",
                value: values.len() as f64,
                min: 0.0,
                max: MAX_ALLOWED_VALUES_PER_FIELD as f64,
            });
        }
        Ok(Self {
            required,
            allowed_string_values: values,
        })
    }

    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }

    #[must_use]
    pub fn allowed_string_values(&self) -> &BTreeSet<String> {
        &self.allowed_string_values
    }
}

/// Semantic-support rule for one invocation: the evidence *bodies*
/// (ref id → excerpt text) a support judge reads to decide whether a
/// claim's cited evidence actually supports what the claim says, and
/// the minimum confidence (percent, 0–100) a verdict must reach.
///
/// This is the second gate behind [`EvidenceGroundingRule`]: grounding
/// checks that the citation *exists*; semantic support checks that the
/// citation *holds*. The judgment itself comes from a wired
/// `EvidenceSupportJudgePort` implementation — the rule only carries
/// what the judge needs and the deterministic acceptance threshold, so
/// the decision stays a rule even when the signal comes from a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticSupportRule {
    min_confidence: u8,
    bodies: BTreeMap<String, String>,
}

impl SemanticSupportRule {
    /// Build a semantic-support rule. `bodies` must be non-empty: a
    /// support gate with nothing to read is a configuration error, not
    /// a stricter gate (mirroring the grounding rule's posture on an
    /// empty pack).
    pub fn new(
        min_confidence: u8,
        bodies: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Result<Self, DomainError> {
        if min_confidence > 100 {
            return Err(DomainError::OutOfRange {
                field: "output_contract.evidence.semantic_support.min_confidence",
                value: f64::from(min_confidence),
                min: 0.0,
                max: 100.0,
            });
        }
        let bodies = bodies
            .into_iter()
            .map(|(reference, body)| {
                let reference = validate_text(
                    &reference.into(),
                    "output_contract.evidence.semantic_support.body_ref",
                    MAX_ALLOWED_VALUE_LEN,
                )?;
                let body = validate_text(
                    &body.into(),
                    "output_contract.evidence.semantic_support.body",
                    MAX_EVIDENCE_BODY_LEN,
                )?;
                Ok::<_, DomainError>((reference, body))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        if bodies.is_empty() {
            return Err(DomainError::EmptyField {
                field: "output_contract.evidence.semantic_support.bodies",
            });
        }
        if bodies.len() > MAX_ALLOWED_EVIDENCE_REFS {
            return Err(DomainError::OutOfRange {
                field: "output_contract.evidence.semantic_support.bodies",
                value: bodies.len() as f64,
                min: 1.0,
                max: MAX_ALLOWED_EVIDENCE_REFS as f64,
            });
        }
        Ok(Self {
            min_confidence,
            bodies,
        })
    }

    /// Minimum confidence (percent, 0–100) a support verdict must
    /// reach for the claim to count as supported.
    #[must_use]
    pub const fn min_confidence(&self) -> u8 {
        self.min_confidence
    }

    /// Evidence bodies by reference id.
    #[must_use]
    pub fn bodies(&self) -> &BTreeMap<String, String> {
        &self.bodies
    }

    /// The body for one evidence reference, when the rule carries it.
    #[must_use]
    pub fn body(&self, reference: &str) -> Option<&str> {
        self.bodies.get(reference).map(String::as_str)
    }
}

/// Evidence-grounding rule for one invocation: which output field
/// carries the claims, which per-claim field carries the evidence
/// references, and the closed set of reference ids that count as real
/// evidence for this deliberation (the "evidence pack").
///
/// The rule is deliberately shape-only: the core does not know what an
/// evidence ref points at (a document, a trace, a metric snapshot) —
/// only that a claim citing a ref outside the pack is ungrounded. When
/// a [`SemanticSupportRule`] is attached the contract additionally
/// demands that every claim's cited evidence *supports* the claim, as
/// judged through the `EvidenceSupportJudgePort`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceGroundingRule {
    claims_field: String,
    refs_field: String,
    allowed_refs: BTreeSet<String>,
    /// Optional second gate: semantic support of each claim by its
    /// cited evidence bodies. `None` keeps the historical
    /// citation-existence semantics (and the historical wire shape).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    semantic_support: Option<SemanticSupportRule>,
}

impl EvidenceGroundingRule {
    /// Build a grounding rule. `allowed_refs` must be non-empty: an
    /// evidence-bound deliberation with an empty pack is a
    /// configuration error, not a stricter gate.
    pub fn new(
        claims_field: impl Into<String>,
        refs_field: impl Into<String>,
        allowed_refs: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, DomainError> {
        let claims_field = validate_text(
            &claims_field.into(),
            "output_contract.evidence.claims_field",
            MAX_FIELD_NAME_LEN,
        )?;
        let refs_field = validate_text(
            &refs_field.into(),
            "output_contract.evidence.refs_field",
            MAX_FIELD_NAME_LEN,
        )?;
        let allowed_refs = allowed_refs
            .into_iter()
            .map(|reference| {
                let reference = reference.into();
                validate_text(
                    &reference,
                    "output_contract.evidence.allowed_ref",
                    MAX_ALLOWED_VALUE_LEN,
                )
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if allowed_refs.is_empty() {
            return Err(DomainError::EmptyField {
                field: "output_contract.evidence.allowed_refs",
            });
        }
        if allowed_refs.len() > MAX_ALLOWED_EVIDENCE_REFS {
            return Err(DomainError::OutOfRange {
                field: "output_contract.evidence.allowed_refs",
                value: allowed_refs.len() as f64,
                min: 1.0,
                max: MAX_ALLOWED_EVIDENCE_REFS as f64,
            });
        }
        Ok(Self {
            claims_field,
            refs_field,
            allowed_refs,
            semantic_support: None,
        })
    }

    /// Attach a semantic-support rule. Every allowed reference must
    /// carry a body: a pack entry the judge cannot read would make the
    /// gate's outcome depend on which ref a proposal happens to cite —
    /// a config gap must fail loudly at wiring time, not at judgment
    /// time.
    pub fn with_semantic_support(mut self, rule: SemanticSupportRule) -> Result<Self, DomainError> {
        if self
            .allowed_refs
            .iter()
            .any(|reference| !rule.bodies.contains_key(reference))
        {
            return Err(DomainError::EmptyField {
                field: "output_contract.evidence.semantic_support.bodies",
            });
        }
        self.semantic_support = Some(rule);
        Ok(self)
    }

    #[must_use]
    pub fn claims_field(&self) -> &str {
        &self.claims_field
    }

    #[must_use]
    pub fn refs_field(&self) -> &str {
        &self.refs_field
    }

    #[must_use]
    pub fn allowed_refs(&self) -> &BTreeSet<String> {
        &self.allowed_refs
    }

    /// Semantic-support rule, when the contract demands one. `None`
    /// means the support validator is a no-op for this invocation.
    #[must_use]
    pub fn semantic_support(&self) -> Option<&SemanticSupportRule> {
        self.semantic_support.as_ref()
    }
}

/// Typed structured-output contract attached to one invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputContract {
    contract_id: String,
    format: OutputFormat,
    #[serde(default)]
    fields: BTreeMap<String, OutputFieldRule>,
    /// Optional embedded JSON Schema. When non-empty, the adapter
    /// JSON-schema validator parses it once and validates every
    /// proposal output against it in addition to the field-level
    /// rules. Kept as a `String` here so the core stays free of any
    /// schema-engine dependency.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    json_schema: String,
    /// Optional evidence-grounding rule. When present, the adapter
    /// grounding validator rejects proposals whose claims do not cite
    /// evidence from the allowed pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evidence_grounding: Option<EvidenceGroundingRule>,
}

impl OutputContract {
    pub fn new(
        contract_id: impl Into<String>,
        format: OutputFormat,
        fields: BTreeMap<String, OutputFieldRule>,
    ) -> Result<Self, DomainError> {
        Self::new_with_schema(contract_id, format, fields, String::new())
    }

    /// Build a contract that also carries an embedded JSON Schema
    /// body. The schema text is whitespace-trimmed and length-bounded
    /// (`MAX_JSON_SCHEMA_LEN = 256 KiB`); validation that the body is
    /// itself well-formed JSON / a valid JSON Schema document happens
    /// at adapter wiring time (the core does not pull a schema
    /// engine in).
    pub fn new_with_schema(
        contract_id: impl Into<String>,
        format: OutputFormat,
        fields: BTreeMap<String, OutputFieldRule>,
        json_schema: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let contract_id = contract_id.into();
        let contract_id = validate_text(
            &contract_id,
            "output_contract.contract_id",
            MAX_CONTRACT_ID_LEN,
        )?;
        if fields.len() > MAX_FIELDS {
            return Err(DomainError::OutOfRange {
                field: "output_contract.fields",
                value: fields.len() as f64,
                min: 0.0,
                max: MAX_FIELDS as f64,
            });
        }

        let mut normalized = BTreeMap::new();
        for (name, rule) in fields {
            let field_name =
                validate_text(&name, "output_contract.field.name", MAX_FIELD_NAME_LEN)?;
            normalized.insert(field_name, rule);
        }

        let json_schema = normalize_optional_schema(&json_schema.into())?;

        Ok(Self {
            contract_id,
            format,
            fields: normalized,
            json_schema,
            evidence_grounding: None,
        })
    }

    pub fn json_object(
        contract_id: impl Into<String>,
        fields: BTreeMap<String, OutputFieldRule>,
    ) -> Result<Self, DomainError> {
        Self::new(contract_id, OutputFormat::JsonObject, fields)
    }

    #[must_use]
    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }

    #[must_use]
    pub const fn format(&self) -> OutputFormat {
        self.format
    }

    #[must_use]
    pub fn fields(&self) -> &BTreeMap<String, OutputFieldRule> {
        &self.fields
    }

    /// Embedded JSON Schema body. Empty string means "no schema —
    /// only field-level rules apply"; the JSON Schema validator
    /// adapter treats empty as a no-op.
    #[must_use]
    pub fn json_schema(&self) -> &str {
        &self.json_schema
    }

    /// Attach an evidence-grounding rule to this contract.
    #[must_use]
    pub fn with_evidence_grounding(mut self, rule: EvidenceGroundingRule) -> Self {
        self.evidence_grounding = Some(rule);
        self
    }

    /// Evidence-grounding rule, when the contract declares one. `None`
    /// means the grounding validator is a no-op for this invocation.
    #[must_use]
    pub fn evidence_grounding(&self) -> Option<&EvidenceGroundingRule> {
        self.evidence_grounding.as_ref()
    }
}

fn normalize_optional_schema(raw: &str) -> Result<String, DomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.len() > MAX_JSON_SCHEMA_LEN {
        return Err(DomainError::FieldTooLong {
            field: "output_contract.json_schema",
            actual: trimmed.len(),
            max: MAX_JSON_SCHEMA_LEN,
        });
    }
    Ok(trimmed.to_owned())
}

fn validate_text(value: &str, field: &'static str, max_len: usize) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::EmptyField { field });
    }
    if trimmed.len() > max_len {
        return Err(DomainError::FieldTooLong {
            field,
            actual: trimmed.len(),
            max: max_len,
        });
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rule() -> OutputFieldRule {
        OutputFieldRule::new(true, ["emit_event", "escalate"]).unwrap()
    }

    #[test]
    fn json_object_contract_keeps_fields() {
        let contract = OutputContract::json_object(
            "decision-contract",
            BTreeMap::from([("decision".to_owned(), sample_rule())]),
        )
        .unwrap();

        assert_eq!(contract.contract_id(), "decision-contract");
        assert_eq!(contract.format(), OutputFormat::JsonObject);
        assert!(contract.fields()["decision"].required());
        assert!(contract.fields()["decision"]
            .allowed_string_values()
            .contains("emit_event"));
    }

    #[test]
    fn blank_contract_id_is_rejected() {
        let err = OutputContract::json_object("   ", BTreeMap::new()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "output_contract.contract_id"
            }
        ));
    }

    #[test]
    fn blank_field_name_is_rejected() {
        let err = OutputContract::json_object(
            "c1",
            BTreeMap::from([("   ".to_owned(), OutputFieldRule::default())]),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "output_contract.field.name"
            }
        ));
    }

    #[test]
    fn blank_allowed_value_is_rejected() {
        let err = OutputFieldRule::new(false, [" "]).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "output_contract.field.allowed_value"
            }
        ));
    }

    #[test]
    fn serde_roundtrip_is_stable() {
        let contract = OutputContract::json_object(
            "decision-contract",
            BTreeMap::from([("decision".to_owned(), sample_rule())]),
        )
        .unwrap();
        let serialized = serde_json::to_string(&contract).unwrap();
        let back: OutputContract = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, contract);
    }

    #[test]
    fn json_schema_is_empty_by_default() {
        let contract = OutputContract::json_object("c1", BTreeMap::new()).unwrap();
        assert!(contract.json_schema().is_empty());
    }

    #[test]
    fn new_with_schema_carries_trimmed_body() {
        let raw = "  { \"type\": \"object\" }  ";
        let contract = OutputContract::new_with_schema(
            "decision-contract",
            OutputFormat::JsonObject,
            BTreeMap::new(),
            raw,
        )
        .unwrap();
        assert_eq!(contract.json_schema(), "{ \"type\": \"object\" }");
    }

    #[test]
    fn overlong_schema_is_rejected() {
        let body = "x".repeat(MAX_JSON_SCHEMA_LEN + 1);
        let err =
            OutputContract::new_with_schema("c1", OutputFormat::JsonObject, BTreeMap::new(), body)
                .unwrap_err();
        assert!(matches!(
            err,
            DomainError::FieldTooLong {
                field: "output_contract.json_schema",
                ..
            }
        ));
    }

    #[test]
    fn evidence_grounding_rule_keeps_fields_and_refs() {
        let rule = EvidenceGroundingRule::new("claims", "evidence_refs", ["ev-1", "ev-2"]).unwrap();
        assert_eq!(rule.claims_field(), "claims");
        assert_eq!(rule.refs_field(), "evidence_refs");
        assert!(rule.allowed_refs().contains("ev-1"));
        assert_eq!(rule.allowed_refs().len(), 2);
    }

    #[test]
    fn evidence_grounding_rule_rejects_empty_pack() {
        let err = EvidenceGroundingRule::new("claims", "evidence_refs", Vec::<String>::new())
            .unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "output_contract.evidence.allowed_refs"
            }
        ));
    }

    #[test]
    fn evidence_grounding_rule_rejects_blank_ref() {
        let err = EvidenceGroundingRule::new("claims", "evidence_refs", ["  "]).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "output_contract.evidence.allowed_ref"
            }
        ));
    }

    #[test]
    fn contract_with_evidence_grounding_roundtrips() {
        let contract = OutputContract::json_object("c1", BTreeMap::new())
            .unwrap()
            .with_evidence_grounding(
                EvidenceGroundingRule::new("claims", "evidence_refs", ["ev-1"]).unwrap(),
            );
        let serialized = serde_json::to_string(&contract).unwrap();
        let back: OutputContract = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, contract);
        assert_eq!(back.evidence_grounding().unwrap().claims_field(), "claims");
    }

    #[test]
    fn semantic_support_rule_keeps_bodies_and_threshold() {
        let rule =
            SemanticSupportRule::new(80, [("ev-1", "typha held port 5473"), ("ev-2", "crun log")])
                .unwrap();
        assert_eq!(rule.min_confidence(), 80);
        assert_eq!(rule.body("ev-1"), Some("typha held port 5473"));
        assert_eq!(rule.bodies().len(), 2);
    }

    #[test]
    fn semantic_support_rule_rejects_out_of_range_confidence() {
        let err = SemanticSupportRule::new(101, [("ev-1", "body")]).unwrap_err();
        assert!(matches!(
            err,
            DomainError::OutOfRange {
                field: "output_contract.evidence.semantic_support.min_confidence",
                ..
            }
        ));
    }

    #[test]
    fn semantic_support_rule_rejects_empty_bodies() {
        let err = SemanticSupportRule::new(70, Vec::<(String, String)>::new()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "output_contract.evidence.semantic_support.bodies"
            }
        ));
    }

    #[test]
    fn semantic_support_rule_rejects_blank_body() {
        let err = SemanticSupportRule::new(70, [("ev-1", "   ")]).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "output_contract.evidence.semantic_support.body"
            }
        ));
    }

    #[test]
    fn semantic_support_requires_a_body_for_every_allowed_ref() {
        let grounding =
            EvidenceGroundingRule::new("claims", "evidence_refs", ["ev-1", "ev-2"]).unwrap();
        let partial = SemanticSupportRule::new(70, [("ev-1", "only one body")]).unwrap();
        let err = grounding.with_semantic_support(partial).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "output_contract.evidence.semantic_support.bodies"
            }
        ));
    }

    #[test]
    fn grounding_with_semantic_support_roundtrips() {
        let rule = EvidenceGroundingRule::new("claims", "evidence_refs", ["ev-1"])
            .unwrap()
            .with_semantic_support(SemanticSupportRule::new(70, [("ev-1", "body")]).unwrap())
            .unwrap();
        let contract = OutputContract::json_object("c1", BTreeMap::new())
            .unwrap()
            .with_evidence_grounding(rule);
        let serialized = serde_json::to_string(&contract).unwrap();
        let back: OutputContract = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, contract);
        let support = back
            .evidence_grounding()
            .unwrap()
            .semantic_support()
            .unwrap();
        assert_eq!(support.min_confidence(), 70);
        assert_eq!(support.body("ev-1"), Some("body"));
    }

    #[test]
    fn grounding_without_semantic_support_deserializes_from_legacy_wire_shape() {
        // Grounding rules serialized before the semantic-support field
        // existed must keep deserializing.
        let legacy =
            r#"{"claims_field":"claims","refs_field":"evidence_refs","allowed_refs":["ev-1"]}"#;
        let back: EvidenceGroundingRule = serde_json::from_str(legacy).unwrap();
        assert!(back.semantic_support().is_none());
    }

    #[test]
    fn contract_without_grounding_deserializes_from_legacy_wire_shape() {
        // Contracts serialized before the grounding field existed must
        // keep deserializing (registry/persistence compatibility).
        let legacy = r#"{"contract_id":"c1","format":"JsonObject","fields":{}}"#;
        let back: OutputContract = serde_json::from_str(legacy).unwrap();
        assert!(back.evidence_grounding().is_none());
    }

    #[test]
    fn schema_serde_roundtrip_preserves_body() {
        let contract = OutputContract::new_with_schema(
            "c1",
            OutputFormat::JsonObject,
            BTreeMap::new(),
            "{\"type\":\"object\"}",
        )
        .unwrap();
        let serialized = serde_json::to_string(&contract).unwrap();
        let back: OutputContract = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, contract);
        assert_eq!(back.json_schema(), "{\"type\":\"object\"}");
    }
}
