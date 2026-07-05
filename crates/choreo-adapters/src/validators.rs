//! Default validator adapters.
//!
//! These implementations cover use-case-agnostic sanity checks. They
//! are deliberately minimal; domain-specific validators (clinical
//! safety, policy compliance, fact checking, …) belong in the
//! integrating product, not in the Choreographer.

use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::entities::{TaskConstraints, ValidatorReport};
use choreo_core::error::DomainError;
use choreo_core::ports::{EvidenceExcerpt, EvidenceSupportJudgePort, ValidatorPort};
use choreo_core::value_objects::Attributes;
use serde_json::{json, Map, Value};

/// A validator that fails a proposal only when its content is empty
/// (after trimming). The thinnest possible "is there anything here"
/// check — useful as a default so operators who do not configure a
/// richer validator still get a meaningful ranking signal.
#[derive(Debug, Default, Clone, Copy)]
pub struct ContentNonEmptyValidator;

impl ContentNonEmptyValidator {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ValidatorPort for ContentNonEmptyValidator {
    fn kind(&self) -> &'static str {
        "content-non-empty"
    }

    async fn validate(
        &self,
        proposal_content: &str,
        _constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError> {
        let trimmed = proposal_content.trim();
        let passed = !trimmed.is_empty();
        let summary = if passed {
            format!("len={}", trimmed.len())
        } else {
            "content is empty after trimming".to_owned()
        };
        ValidatorReport::new(self.kind(), passed, summary, Attributes::empty())
    }
}

/// A validator that requires structured-output proposals to be valid
/// JSON objects at the root.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonObjectOutputValidator;

impl JsonObjectOutputValidator {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ValidatorPort for JsonObjectOutputValidator {
    fn kind(&self) -> &'static str {
        "output-json-object"
    }

    async fn validate(
        &self,
        proposal_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError> {
        if constraints.output_contract().is_none() {
            return ValidatorReport::new(
                self.kind(),
                true,
                "no structured output contract configured",
                Attributes::empty(),
            );
        }

        match parse_json_object(proposal_content) {
            Ok(_) => ValidatorReport::new(
                self.kind(),
                true,
                "proposal is a valid JSON object",
                Attributes::empty(),
            ),
            Err(summary) => ValidatorReport::new(
                self.kind(),
                false,
                summary,
                attributes(json!({ "expected_format": "json_object" }))?,
            ),
        }
    }
}

/// A validator that enforces required fields declared in the output
/// contract.
#[derive(Debug, Default, Clone, Copy)]
pub struct RequiredFieldsValidator;

impl RequiredFieldsValidator {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ValidatorPort for RequiredFieldsValidator {
    fn kind(&self) -> &'static str {
        "output-required-fields"
    }

    async fn validate(
        &self,
        proposal_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError> {
        let Some(contract) = constraints.output_contract() else {
            return ValidatorReport::new(
                self.kind(),
                true,
                "no structured output contract configured",
                Attributes::empty(),
            );
        };

        let object = match parse_json_object(proposal_content) {
            Ok(object) => object,
            Err(summary) => {
                return ValidatorReport::new(
                    self.kind(),
                    false,
                    summary,
                    attributes(json!({ "contract_id": contract.contract_id() }))?,
                );
            }
        };

        let missing_fields = contract
            .fields()
            .iter()
            .filter(|(_, rule)| rule.required())
            .filter(|(field_name, _)| !object.contains_key(*field_name))
            .map(|(field_name, _)| field_name.clone())
            .collect::<Vec<_>>();

        if missing_fields.is_empty() {
            ValidatorReport::new(
                self.kind(),
                true,
                "all required fields are present",
                Attributes::empty(),
            )
        } else {
            ValidatorReport::new(
                self.kind(),
                false,
                format!("missing required fields: {}", missing_fields.join(", ")),
                attributes(json!({
                    "contract_id": contract.contract_id(),
                    "missing_fields": missing_fields,
                }))?,
            )
        }
    }
}

/// A validator that enforces allowed string sets declared for one or
/// more fields in the output contract.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowedStringValuesValidator;

impl AllowedStringValuesValidator {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ValidatorPort for AllowedStringValuesValidator {
    fn kind(&self) -> &'static str {
        "output-allowed-string-values"
    }

    async fn validate(
        &self,
        proposal_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError> {
        let Some(contract) = constraints.output_contract() else {
            return ValidatorReport::new(
                self.kind(),
                true,
                "no structured output contract configured",
                Attributes::empty(),
            );
        };

        let object = match parse_json_object(proposal_content) {
            Ok(object) => object,
            Err(summary) => {
                return ValidatorReport::new(
                    self.kind(),
                    false,
                    summary,
                    attributes(json!({ "contract_id": contract.contract_id() }))?,
                );
            }
        };

        let mut violations = Vec::new();
        for (field_name, rule) in contract.fields() {
            if rule.allowed_string_values().is_empty() {
                continue;
            }
            let Some(value) = object.get(field_name) else {
                continue;
            };
            match value {
                Value::String(actual) if rule.allowed_string_values().contains(actual) => {}
                Value::String(actual) => violations.push(json!({
                    "field": field_name,
                    "actual": actual,
                    "allowed": rule.allowed_string_values(),
                })),
                other => violations.push(json!({
                    "field": field_name,
                    "actual_type": value_type_name(other),
                    "allowed": rule.allowed_string_values(),
                })),
            }
        }

        if violations.is_empty() {
            ValidatorReport::new(
                self.kind(),
                true,
                "all constrained string fields satisfy their allowed value sets",
                Attributes::empty(),
            )
        } else {
            ValidatorReport::new(
                self.kind(),
                false,
                "one or more constrained string fields violated the contract",
                attributes(json!({
                    "contract_id": contract.contract_id(),
                    "violations": violations,
                }))?,
            )
        }
    }
}

/// A validator that runs a full JSON Schema document (embedded in
/// `OutputContract::json_schema`) against the proposal output.
///
/// Subsumes both the "JSON Schema" and the "bounded event proposal
/// shape" deliverables from Epic 4 of the backlog: bounded shapes
/// (`maxLength`, `maxItems`, `additionalProperties: false`, …) are
/// expressed as JSON Schema constraints rather than a bespoke
/// validator.
///
/// Implementation notes:
///
/// - empty schema body → no-op (passes). Tasks without a schema keep
///   using the field-rule validators above.
/// - schema body must be a valid JSON document; malformed JSON or an
///   unsupported schema fails the proposal with a clear summary.
/// - compilation happens per-call. Schemas are small in practice and
///   compilation cost is dwarfed by an LLM-generated proposal — if
///   profiling later says otherwise, the validator can grow a
///   schema-text cache without changing the port surface.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonSchemaValidator;

impl JsonSchemaValidator {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ValidatorPort for JsonSchemaValidator {
    fn kind(&self) -> &'static str {
        "output-json-schema"
    }

    async fn validate(
        &self,
        proposal_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError> {
        // Cap on the number of violations reported per failed
        // proposal. A pathological schema can produce hundreds of
        // sub-errors; the caller can re-validate locally for the full
        // list if needed.
        const MAX_REPORTED_VIOLATIONS: usize = 16;

        let Some(contract) = constraints.output_contract() else {
            return ValidatorReport::new(
                self.kind(),
                true,
                "no structured output contract configured",
                Attributes::empty(),
            );
        };
        let schema_body = contract.json_schema();
        if schema_body.is_empty() {
            return ValidatorReport::new(
                self.kind(),
                true,
                "contract has no embedded JSON Schema",
                Attributes::empty(),
            );
        }

        let schema_value: Value = match serde_json::from_str(schema_body) {
            Ok(v) => v,
            Err(err) => {
                return ValidatorReport::new(
                    self.kind(),
                    false,
                    format!("output_contract.json_schema is not valid JSON: {err}"),
                    attributes(json!({ "contract_id": contract.contract_id() }))?,
                );
            }
        };

        let compiled = match jsonschema::JSONSchema::compile(&schema_value) {
            Ok(c) => c,
            Err(err) => {
                return ValidatorReport::new(
                    self.kind(),
                    false,
                    format!("output_contract.json_schema is not a valid JSON Schema: {err}"),
                    attributes(json!({ "contract_id": contract.contract_id() }))?,
                );
            }
        };

        let instance: Value = match serde_json::from_str(strip_markdown_fences(proposal_content)) {
            Ok(v) => v,
            Err(err) => {
                return ValidatorReport::new(
                    self.kind(),
                    false,
                    format!("proposal is not valid JSON: {err}"),
                    attributes(json!({ "contract_id": contract.contract_id() }))?,
                );
            }
        };

        // Collect violations into owned `Vec<Value>` immediately so
        // the iterator's borrow of `compiled` + `instance` ends before
        // we cross the function return.
        let violations: Vec<Value> = match compiled.validate(&instance) {
            Ok(()) => Vec::new(),
            Err(errors) => errors
                .take(MAX_REPORTED_VIOLATIONS)
                .map(|err| {
                    json!({
                        "instance_path": err.instance_path.to_string(),
                        "schema_path": err.schema_path.to_string(),
                        "reason": err.to_string(),
                    })
                })
                .collect(),
        };

        if violations.is_empty() {
            ValidatorReport::new(
                self.kind(),
                true,
                "output satisfies the embedded JSON Schema",
                Attributes::empty(),
            )
        } else {
            let summary = if let Some(first) = violations.first() {
                let path = first
                    .get("instance_path")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let reason = first.get("reason").and_then(Value::as_str).unwrap_or("");
                if path.is_empty() {
                    format!("schema violation: {reason}")
                } else {
                    format!("schema violation at `{path}`: {reason}")
                }
            } else {
                "schema validation failed".to_owned()
            };
            ValidatorReport::new(
                self.kind(),
                false,
                summary,
                attributes(json!({
                    "contract_id": contract.contract_id(),
                    "violations": violations,
                }))?,
            )
        }
    }
}

/// Defends downstream consumers against pathological structured
/// outputs: payloads that nest too deep, carry too many keys, or
/// blow up an array. A consumer that trusts the choreographer's
/// `OutputContract` chain should never have to second-guess these
/// limits; a `JsonSchema` can constrain shape but not raw size.
///
/// The validator runs **only** when the task has an `OutputContract`
/// — without one there is no structured-output expectation to bound.
/// In that case it short-circuits with a pass and a `not applicable`
/// note, mirroring the JsonSchemaValidator's posture.
///
/// Limits are conservative by default and tunable through the
/// `with_*` builder methods. Each one is an inclusive upper bound;
/// the validator counts at most one violation per dimension so a
/// huge payload that breaks several limits still produces a small,
/// human-readable report.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_field_names)] // every limit IS a "max_*" budget; renaming muddies intent
pub struct BoundedEventShapeValidator {
    /// Total UTF-8 byte length of the proposal content. Caps the
    /// envelope size a single deliberation can place on the bus.
    max_total_size_bytes: usize,
    /// Maximum nesting depth of objects + arrays combined.
    max_depth: usize,
    /// Maximum number of keys in any single object.
    max_object_keys: usize,
    /// Maximum number of elements in any single array.
    max_array_len: usize,
    /// Maximum UTF-8 byte length of any single string value.
    max_string_len: usize,
}

impl BoundedEventShapeValidator {
    /// Defaults chosen for use cases where the output is fed into a
    /// downstream event bus or audit log:
    ///
    /// - 256 KiB total — matches the `OutputContract.json_schema`
    ///   cap so the schema and the validated instance share a budget.
    /// - 32 levels of nesting — deeper than any realistic Report.
    /// - 256 object keys — Report fields plus generous extension room.
    /// - 1024 array elements — caps repeated-findings explosions.
    /// - 64 KiB per string — a single field cannot dominate the
    ///   envelope.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_total_size_bytes: 256 * 1024,
            max_depth: 32,
            max_object_keys: 256,
            max_array_len: 1024,
            max_string_len: 64 * 1024,
        }
    }

    #[must_use]
    pub const fn with_max_total_size_bytes(mut self, bytes: usize) -> Self {
        self.max_total_size_bytes = bytes;
        self
    }
    #[must_use]
    pub const fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }
    #[must_use]
    pub const fn with_max_object_keys(mut self, keys: usize) -> Self {
        self.max_object_keys = keys;
        self
    }
    #[must_use]
    pub const fn with_max_array_len(mut self, len: usize) -> Self {
        self.max_array_len = len;
        self
    }
    #[must_use]
    pub const fn with_max_string_len(mut self, len: usize) -> Self {
        self.max_string_len = len;
        self
    }
}

impl Default for BoundedEventShapeValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ValidatorPort for BoundedEventShapeValidator {
    fn kind(&self) -> &'static str {
        "output-bounded-event-shape"
    }

    async fn validate(
        &self,
        proposal_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError> {
        let Some(contract) = constraints.output_contract() else {
            return ValidatorReport::new(
                self.kind(),
                true,
                "no structured output contract configured",
                Attributes::empty(),
            );
        };

        let trimmed = proposal_content.trim();
        let total_bytes = trimmed.len();
        if total_bytes > self.max_total_size_bytes {
            return ValidatorReport::new(
                self.kind(),
                false,
                format!(
                    "proposal is {total_bytes} bytes; limit is {} bytes",
                    self.max_total_size_bytes
                ),
                attributes(json!({
                    "contract_id": contract.contract_id(),
                    "violation": "max_total_size_bytes",
                    "limit": self.max_total_size_bytes,
                    "actual": total_bytes,
                }))?,
            );
        }

        let instance: Value = match serde_json::from_str(strip_markdown_fences(trimmed)) {
            Ok(v) => v,
            Err(err) => {
                return ValidatorReport::new(
                    self.kind(),
                    false,
                    format!("proposal is not valid JSON: {err}"),
                    attributes(json!({ "contract_id": contract.contract_id() }))?,
                );
            }
        };

        if let Some(violation) = self.walk(&instance, "$", 0) {
            return ValidatorReport::new(
                self.kind(),
                false,
                violation.summary(),
                attributes(json!({
                    "contract_id": contract.contract_id(),
                    "violation": violation.kind,
                    "path": violation.path,
                    "limit": violation.limit,
                    "actual": violation.actual,
                }))?,
            );
        }

        ValidatorReport::new(
            self.kind(),
            true,
            "proposal satisfies the event-shape budget",
            attributes(json!({ "contract_id": contract.contract_id() }))?,
        )
    }
}

impl BoundedEventShapeValidator {
    fn walk(&self, value: &Value, path: &str, depth: usize) -> Option<ShapeViolation> {
        if depth > self.max_depth {
            return Some(ShapeViolation {
                kind: "max_depth",
                path: path.to_owned(),
                limit: self.max_depth,
                actual: depth,
            });
        }
        match value {
            Value::Null | Value::Bool(_) | Value::Number(_) => None,
            Value::String(s) => {
                if s.len() > self.max_string_len {
                    Some(ShapeViolation {
                        kind: "max_string_len",
                        path: path.to_owned(),
                        limit: self.max_string_len,
                        actual: s.len(),
                    })
                } else {
                    None
                }
            }
            Value::Array(items) => {
                if items.len() > self.max_array_len {
                    return Some(ShapeViolation {
                        kind: "max_array_len",
                        path: path.to_owned(),
                        limit: self.max_array_len,
                        actual: items.len(),
                    });
                }
                for (i, item) in items.iter().enumerate() {
                    if let Some(v) = self.walk(item, &format!("{path}[{i}]"), depth + 1) {
                        return Some(v);
                    }
                }
                None
            }
            Value::Object(map) => {
                if map.len() > self.max_object_keys {
                    return Some(ShapeViolation {
                        kind: "max_object_keys",
                        path: path.to_owned(),
                        limit: self.max_object_keys,
                        actual: map.len(),
                    });
                }
                for (key, child) in map {
                    if let Some(v) = self.walk(child, &format!("{path}.{key}"), depth + 1) {
                        return Some(v);
                    }
                }
                None
            }
        }
    }
}

#[derive(Debug)]
struct ShapeViolation {
    kind: &'static str,
    path: String,
    limit: usize,
    actual: usize,
}

impl ShapeViolation {
    fn summary(&self) -> String {
        format!(
            "{} at `{}`: {} exceeds limit {}",
            self.kind, self.path, self.actual, self.limit
        )
    }
}

/// A validator that enforces the evidence-grounding rule declared in
/// the output contract: every claim in the output must cite at least
/// one evidence reference, and every cited reference must exist in the
/// contract's allowed evidence pack.
///
/// Expected output shape (field names configurable per rule):
///
/// ```json
/// { "claims": [ { "text": "…", "evidence_refs": ["ev-1"] }, … ] }
/// ```
///
/// Semantics:
///
/// - no contract, or contract without a grounding rule → pass
///   (`"no evidence grounding configured"`, the sibling validators'
///   pattern).
/// - claims field missing or not an array → fail (a grounding-gated
///   step must produce inspectable claims).
/// - an empty claims array passes: grounding judges what is claimed,
///   not how much — pair with `required_fields`/`json_schema`
///   (`minItems`) to demand substance.
/// - each claim must be a JSON object whose refs field is a non-empty
///   array of strings, all present in the allowed pack. Violations
///   name the claim index, a text preview and the orphan refs — that
///   detail lands in spans/logs and becomes part of the decision
///   record.
#[derive(Debug, Default, Clone, Copy)]
pub struct ClaimsEvidenceGroundedValidator;

impl ClaimsEvidenceGroundedValidator {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

const CLAIM_TEXT_PREVIEW_LEN: usize = 80;

#[async_trait]
impl ValidatorPort for ClaimsEvidenceGroundedValidator {
    fn kind(&self) -> &'static str {
        "claims-evidence-grounded"
    }

    async fn validate(
        &self,
        proposal_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError> {
        let Some(contract) = constraints.output_contract() else {
            return ValidatorReport::new(
                self.kind(),
                true,
                "no evidence grounding configured",
                Attributes::empty(),
            );
        };
        let Some(rule) = contract.evidence_grounding() else {
            return ValidatorReport::new(
                self.kind(),
                true,
                "no evidence grounding configured",
                Attributes::empty(),
            );
        };

        let object = match parse_json_object(proposal_content) {
            Ok(object) => object,
            Err(summary) => {
                return ValidatorReport::new(
                    self.kind(),
                    false,
                    summary,
                    attributes(json!({ "contract_id": contract.contract_id() }))?,
                );
            }
        };

        let Some(claims) = object.get(rule.claims_field()).and_then(Value::as_array) else {
            return ValidatorReport::new(
                self.kind(),
                false,
                format!(
                    "claims field `{}` is missing or not an array",
                    rule.claims_field()
                ),
                attributes(json!({
                    "contract_id": contract.contract_id(),
                    "claims_field": rule.claims_field(),
                }))?,
            );
        };

        let violations: Vec<Value> = claims
            .iter()
            .enumerate()
            .flat_map(|(index, claim)| claim_violations(index, claim, rule))
            .collect();

        if violations.is_empty() {
            ValidatorReport::new(
                self.kind(),
                true,
                format!(
                    "all {} claims grounded in the evidence pack ({} allowed refs)",
                    claims.len(),
                    rule.allowed_refs().len()
                ),
                Attributes::empty(),
            )
        } else {
            ValidatorReport::new(
                self.kind(),
                false,
                format!(
                    "{} of {} claims lack grounded evidence",
                    violations.len(),
                    claims.len()
                ),
                attributes(json!({
                    "contract_id": contract.contract_id(),
                    "claims_field": rule.claims_field(),
                    "refs_field": rule.refs_field(),
                    "violations": violations,
                }))?,
            )
        }
    }
}

/// Grounding violations for one claim: shape problems, absent or empty
/// refs, non-string refs, and refs outside the allowed pack.
fn claim_violations(
    index: usize,
    claim: &Value,
    rule: &choreo_core::value_objects::EvidenceGroundingRule,
) -> Vec<Value> {
    let Some(claim_object) = claim.as_object() else {
        return vec![json!({
            "claim_index": index,
            "problem": "claim is not a JSON object",
            "actual_type": value_type_name(claim),
        })];
    };
    let preview = claim_preview(claim_object);
    let Some(refs) = claim_object
        .get(rule.refs_field())
        .and_then(Value::as_array)
    else {
        return vec![json!({
            "claim_index": index,
            "claim_preview": preview,
            "problem": format!(
                "refs field `{}` is missing or not an array",
                rule.refs_field()
            ),
        })];
    };
    if refs.is_empty() {
        return vec![json!({
            "claim_index": index,
            "claim_preview": preview,
            "problem": "claim cites no evidence",
        })];
    }

    let mut violations = Vec::new();
    let mut orphans = Vec::new();
    for reference in refs {
        match reference.as_str() {
            Some(id) if rule.allowed_refs().contains(id) => {}
            Some(id) => orphans.push(id.to_owned()),
            None => violations.push(json!({
                "claim_index": index,
                "claim_preview": preview,
                "problem": "evidence ref is not a string",
                "actual_type": value_type_name(reference),
            })),
        }
    }
    if !orphans.is_empty() {
        violations.push(json!({
            "claim_index": index,
            "claim_preview": preview,
            "problem": "evidence refs not present in the allowed pack",
            "orphan_refs": orphans,
        }));
    }
    violations
}

/// Short human preview of a claim for violation details: its `text`
/// field when present, otherwise the serialized object, truncated at a
/// char boundary.
fn claim_preview(claim: &Map<String, Value>) -> String {
    let raw = claim
        .get("text")
        .and_then(Value::as_str)
        .map_or_else(|| Value::Object(claim.clone()).to_string(), str::to_owned);
    if raw.len() > CLAIM_TEXT_PREVIEW_LEN {
        let mut end = CLAIM_TEXT_PREVIEW_LEN;
        while !raw.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &raw[..end])
    } else {
        raw
    }
}

/// Cap on the number of claims one proposal may put in front of the
/// support judge. Far above any real decision output; a proposal that
/// exceeds it fails the gate instead of burning a judge call per claim.
const MAX_JUDGED_CLAIMS: usize = 64;

/// A validator that enforces the semantic-support rule declared in the
/// output contract's evidence block: every claim's *cited* evidence
/// bodies must actually support what the claim says, as judged through
/// the wired [`EvidenceSupportJudgePort`].
///
/// This is the second gate behind [`ClaimsEvidenceGroundedValidator`]:
/// grounding proves the citation exists; this proves it holds. The
/// judgment is model-backed, but the *decision* stays deterministic —
/// a claim passes iff the verdict says `supported` with confidence at
/// or above the contract's `min_confidence` — and every verdict
/// (supported or not, with its rationale) is recorded in the report's
/// details, so the judge's opinion becomes part of the decision record
/// instead of an unrecorded opinion.
///
/// Semantics:
///
/// - no contract, or no grounding rule, or no semantic-support rule →
///   pass (`"no semantic support configured"`, the sibling validators'
///   pattern).
/// - semantic-support rule declared but no judge wired → **hard
///   error**: running the step unjudged would silently void the
///   policy, the same posture the grounding gate takes on an absent
///   evidence pack.
/// - a judge transport/parse failure is a hard error too (fail
///   closed): a gate that cannot judge must not wave proposals
///   through.
/// - claims citing refs the rule has no body for produce a violation
///   (those refs are outside the pack — the grounding gate names them;
///   this gate refuses to judge on nothing).
pub struct ClaimsEvidenceSupportedValidator {
    judge: Option<Arc<dyn EvidenceSupportJudgePort>>,
}

impl std::fmt::Debug for ClaimsEvidenceSupportedValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaimsEvidenceSupportedValidator")
            .field("judge_wired", &self.judge.is_some())
            .finish()
    }
}

impl ClaimsEvidenceSupportedValidator {
    /// Build the validator. `judge` is `None` when the deployment did
    /// not wire a support judge; the validator stays a no-op unless a
    /// contract demands semantic support, in which case it fails
    /// loudly.
    #[must_use]
    pub fn new(judge: Option<Arc<dyn EvidenceSupportJudgePort>>) -> Self {
        Self { judge }
    }
}

#[async_trait]
impl ValidatorPort for ClaimsEvidenceSupportedValidator {
    fn kind(&self) -> &'static str {
        "claims-evidence-supported"
    }

    async fn validate(
        &self,
        proposal_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError> {
        let Some(rule) = constraints
            .output_contract()
            .and_then(|contract| contract.evidence_grounding())
        else {
            return ValidatorReport::new(
                self.kind(),
                true,
                "no semantic support configured",
                Attributes::empty(),
            );
        };
        let Some(support) = rule.semantic_support() else {
            return ValidatorReport::new(
                self.kind(),
                true,
                "no semantic support configured",
                Attributes::empty(),
            );
        };
        let Some(judge) = self.judge.as_ref() else {
            // Fail the step, not the proposal: a contract demanding a
            // judgment the deployment cannot produce is a wiring gap,
            // and waving the proposal through would void the policy.
            return Err(DomainError::InvariantViolated {
                reason: "semantic support demanded by the contract but no \
                         evidence-support judge is wired (set CHOREO_SUPPORT_JUDGE_ENABLED)",
            });
        };
        // `unwrap` is safe: the two `let Some` above prove the contract exists.
        let contract_id = constraints
            .output_contract()
            .map(choreo_core::value_objects::OutputContract::contract_id)
            .unwrap_or_default()
            .to_owned();

        let object = match parse_json_object(proposal_content) {
            Ok(object) => object,
            Err(summary) => {
                return ValidatorReport::new(
                    self.kind(),
                    false,
                    summary,
                    attributes(json!({ "contract_id": contract_id }))?,
                );
            }
        };
        let Some(claims) = object.get(rule.claims_field()).and_then(Value::as_array) else {
            return ValidatorReport::new(
                self.kind(),
                false,
                format!(
                    "claims field `{}` is missing or not an array",
                    rule.claims_field()
                ),
                attributes(json!({
                    "contract_id": contract_id,
                    "claims_field": rule.claims_field(),
                }))?,
            );
        };
        if claims.len() > MAX_JUDGED_CLAIMS {
            return ValidatorReport::new(
                self.kind(),
                false,
                format!(
                    "{} claims exceed the judgeable limit of {MAX_JUDGED_CLAIMS}",
                    claims.len()
                ),
                attributes(json!({ "contract_id": contract_id }))?,
            );
        }

        let (verdicts, violations) =
            judge_claims(judge.as_ref(), claims, rule.refs_field(), support).await?;

        let details = attributes(json!({
            "contract_id": contract_id,
            "min_confidence": support.min_confidence(),
            "verdicts": verdicts,
            "violations": violations,
        }))?;
        if violations.is_empty() {
            ValidatorReport::new(
                self.kind(),
                true,
                format!(
                    "all {} claims semantically supported by their cited evidence \
                     (min_confidence {})",
                    claims.len(),
                    support.min_confidence()
                ),
                details,
            )
        } else {
            ValidatorReport::new(
                self.kind(),
                false,
                format!(
                    "{} of {} claims lack semantic support from their cited evidence",
                    violations.len(),
                    claims.len()
                ),
                details,
            )
        }
    }
}

/// Judge every claim and collect the verdicts (all of them, so the
/// judge's opinion rides the decision record) plus the violations (the
/// claims the deterministic rule rejects). A judge failure propagates:
/// the gate fails closed.
async fn judge_claims(
    judge: &dyn EvidenceSupportJudgePort,
    claims: &[Value],
    refs_field: &str,
    support: &choreo_core::value_objects::SemanticSupportRule,
) -> Result<(Vec<Value>, Vec<Value>), DomainError> {
    let mut verdicts = Vec::with_capacity(claims.len());
    let mut violations = Vec::new();
    for (index, claim) in claims.iter().enumerate() {
        let Some(claim_object) = claim.as_object() else {
            violations.push(json!({
                "claim_index": index,
                "problem": "claim is not a JSON object",
                "actual_type": value_type_name(claim),
            }));
            continue;
        };
        let preview = claim_preview(claim_object);
        let excerpts = cited_excerpts(claim_object, refs_field, support);
        if excerpts.is_empty() {
            violations.push(json!({
                "claim_index": index,
                "claim_preview": preview,
                "problem": "claim cites no evidence with a judgeable body",
            }));
            continue;
        }
        let claim_text = claim_object
            .get("text")
            .and_then(Value::as_str)
            .map_or_else(
                || Value::Object(claim_object.clone()).to_string(),
                str::to_owned,
            );

        let verdict = judge.assess(&claim_text, &excerpts).await?;
        let accepted = verdict.supported && verdict.confidence >= support.min_confidence();
        verdicts.push(json!({
            "claim_index": index,
            "claim_preview": preview,
            "refs": excerpts
                .iter()
                .map(|excerpt| excerpt.reference.clone())
                .collect::<Vec<_>>(),
            "supported": verdict.supported,
            "confidence": verdict.confidence,
            "rationale": verdict.rationale,
        }));
        if !accepted {
            violations.push(json!({
                "claim_index": index,
                "claim_preview": preview,
                "problem": if verdict.supported {
                    format!(
                        "supported but confidence {} is below min_confidence {}",
                        verdict.confidence,
                        support.min_confidence()
                    )
                } else {
                    "cited evidence does not support the claim".to_owned()
                },
            }));
        }
    }
    Ok((verdicts, violations))
}

/// The evidence excerpts a claim actually cited, resolved to bodies.
/// Refs without a body in the rule are skipped — they are outside the
/// pack, which the grounding gate reports by name; this gate only
/// judges on real text.
fn cited_excerpts(
    claim: &Map<String, Value>,
    refs_field: &str,
    support: &choreo_core::value_objects::SemanticSupportRule,
) -> Vec<EvidenceExcerpt> {
    claim
        .get(refs_field)
        .and_then(Value::as_array)
        .map(|refs| {
            refs.iter()
                .filter_map(Value::as_str)
                .filter_map(|reference| {
                    support.body(reference).map(|body| EvidenceExcerpt {
                        reference: reference.to_owned(),
                        body: body.to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_json_object(proposal_content: &str) -> Result<Map<String, Value>, String> {
    let trimmed = strip_markdown_fences(proposal_content);
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|err| format!("proposal is not valid JSON: {err}"))?;
    match value {
        Value::Object(object) => Ok(object),
        other => Err(format!(
            "proposal root must be a JSON object, got {}",
            value_type_name(&other)
        )),
    }
}

/// Contracts govern content, not transport cosmetics: models routinely wrap
/// an otherwise-valid JSON payload in Markdown code fences (```json … ```)
/// even when told not to. When the whole trimmed payload is a single fenced
/// block, unwrap it before parsing; anything else (prose around the fence,
/// multiple blocks) is returned untouched and will fail JSON parsing as
/// before — this is deliberately narrow so the gate never "finds" JSON
/// buried inside surrounding text the model also emitted.
fn strip_markdown_fences(content: &str) -> &str {
    let trimmed = content.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let Some(inner) = rest.strip_suffix("```") else {
        return trimmed;
    };
    // Drop the info string (e.g. `json`) on the opening fence line, if any.
    match inner.split_once('\n') {
        Some((info, body)) if !info.trim().contains(' ') => body.trim(),
        _ => inner.trim(),
    }
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn attributes(value: Value) -> Result<Attributes, DomainError> {
    let Value::Object(object) = value else {
        return Err(DomainError::InvariantViolated {
            reason: "validator details must be JSON objects",
        });
    };
    Attributes::new(object.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{OutputContract, OutputFieldRule};
    use std::collections::BTreeMap;

    fn structured_constraints() -> TaskConstraints {
        TaskConstraints::default().with_output_contract(
            OutputContract::json_object(
                "decision-contract",
                BTreeMap::from([
                    (
                        "decision".to_owned(),
                        OutputFieldRule::new(true, ["emit_event", "escalate"]).unwrap(),
                    ),
                    (
                        "reason".to_owned(),
                        OutputFieldRule::new(true, std::iter::empty::<&str>()).unwrap(),
                    ),
                ]),
            )
            .unwrap(),
        )
    }

    fn grounded_constraints() -> TaskConstraints {
        TaskConstraints::default().with_output_contract(
            OutputContract::json_object("evidence-contract", BTreeMap::new())
                .unwrap()
                .with_evidence_grounding(
                    choreo_core::value_objects::EvidenceGroundingRule::new(
                        "claims",
                        "evidence_refs",
                        ["ev-1", "ev-2"],
                    )
                    .unwrap(),
                ),
        )
    }

    #[tokio::test]
    async fn grounding_passes_without_configuration() {
        let v = ClaimsEvidenceGroundedValidator::new();
        let r = v
            .validate("free prose", &TaskConstraints::default())
            .await
            .unwrap();
        assert!(r.passed());
        assert_eq!(r.kind(), "claims-evidence-grounded");
        assert!(r.summary().contains("no evidence grounding configured"));

        // A contract without a grounding rule is also a no-op.
        let r = v
            .validate("free prose", &structured_constraints())
            .await
            .unwrap();
        assert!(r.passed());
    }

    #[test]
    fn strip_markdown_fences_unwraps_pure_fenced_block() {
        assert_eq!(
            strip_markdown_fences("```json\n{\"a\": 1}\n```"),
            "{\"a\": 1}"
        );
        assert_eq!(strip_markdown_fences("```\n{\"a\": 1}\n```"), "{\"a\": 1}");
        // Sin fences: intacto.
        assert_eq!(strip_markdown_fences("  {\"a\": 1} "), "{\"a\": 1}");
        // Prosa alrededor del fence: intacto (fallara el parse, a proposito).
        let mixed = "look: ```json\n{}\n```";
        assert_eq!(strip_markdown_fences(mixed), mixed.trim());
        // Fence sin cierre: intacto.
        assert_eq!(strip_markdown_fences("```json\n{"), "```json\n{");
    }

    #[tokio::test]
    async fn grounded_claims_pass_inside_markdown_fences() {
        let v = ClaimsEvidenceGroundedValidator::new();
        let content = "```json\n{\"claims\":[{\"text\":\"typha holds the port\",\"evidence_refs\":[\"ev-1\"]}]}\n```";
        let r = v.validate(content, &grounded_constraints()).await.unwrap();
        assert!(r.passed(), "summary: {}", r.summary());
    }

    #[tokio::test]
    async fn grounded_claims_pass() {
        let v = ClaimsEvidenceGroundedValidator::new();
        let content = r#"{"claims":[
            {"text":"typha holds the port","evidence_refs":["ev-1"]},
            {"text":"crun state is per root","evidence_refs":["ev-1","ev-2"]}
        ]}"#;
        let r = v.validate(content, &grounded_constraints()).await.unwrap();
        assert!(r.passed(), "summary: {}", r.summary());
        assert!(r.summary().contains("all 2 claims grounded"));
    }

    #[tokio::test]
    async fn claim_without_refs_fails() {
        let v = ClaimsEvidenceGroundedValidator::new();
        let content = r#"{"claims":[
            {"text":"grounded","evidence_refs":["ev-1"]},
            {"text":"vibes only"}
        ]}"#;
        let r = v.validate(content, &grounded_constraints()).await.unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("1 of 2 claims"));
    }

    #[tokio::test]
    async fn claim_with_empty_refs_fails() {
        let v = ClaimsEvidenceGroundedValidator::new();
        let content = r#"{"claims":[{"text":"empty-handed","evidence_refs":[]}]}"#;
        let r = v.validate(content, &grounded_constraints()).await.unwrap();
        assert!(!r.passed());
    }

    #[tokio::test]
    async fn orphan_ref_fails_and_is_named() {
        let v = ClaimsEvidenceGroundedValidator::new();
        let content = r#"{"claims":[{"text":"invented","evidence_refs":["ev-999"]}]}"#;
        let r = v.validate(content, &grounded_constraints()).await.unwrap();
        assert!(!r.passed());
        let details = serde_json::to_string(r.details()).unwrap();
        assert!(details.contains("ev-999"));
    }

    #[tokio::test]
    async fn missing_claims_field_fails() {
        let v = ClaimsEvidenceGroundedValidator::new();
        let r = v
            .validate(r#"{"decision":"accept"}"#, &grounded_constraints())
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("claims"));
    }

    #[tokio::test]
    async fn empty_claims_array_passes() {
        let v = ClaimsEvidenceGroundedValidator::new();
        let r = v
            .validate(r#"{"claims":[]}"#, &grounded_constraints())
            .await
            .unwrap();
        assert!(r.passed());
    }

    // ---- ClaimsEvidenceSupportedValidator ------------------------------

    use choreo_core::ports::SupportVerdict;
    use choreo_core::value_objects::SemanticSupportRule;

    /// Scripted judge: answers by looking the claim text up in a
    /// verdict table; records what it was asked so tests can assert the
    /// excerpts a claim put in front of it.
    #[derive(Default)]
    struct ScriptedJudge {
        verdicts: std::collections::BTreeMap<String, SupportVerdict>,
        asked: std::sync::Mutex<Vec<(String, Vec<String>)>>,
    }

    #[async_trait]
    impl EvidenceSupportJudgePort for ScriptedJudge {
        async fn assess(
            &self,
            claim_text: &str,
            evidence: &[EvidenceExcerpt],
        ) -> Result<SupportVerdict, DomainError> {
            self.asked.lock().unwrap().push((
                claim_text.to_owned(),
                evidence
                    .iter()
                    .map(|excerpt| excerpt.reference.clone())
                    .collect(),
            ));
            self.verdicts
                .get(claim_text)
                .cloned()
                .ok_or(DomainError::InvariantViolated {
                    reason: "scripted judge: unexpected claim",
                })
        }
    }

    fn verdict(supported: bool, confidence: u8) -> SupportVerdict {
        SupportVerdict {
            supported,
            confidence,
            rationale: "scripted".to_owned(),
        }
    }

    fn supported_constraints(min_confidence: u8) -> TaskConstraints {
        let rule = choreo_core::value_objects::EvidenceGroundingRule::new(
            "claims",
            "evidence_refs",
            ["ev-1", "ev-2"],
        )
        .unwrap()
        .with_semantic_support(
            SemanticSupportRule::new(
                min_confidence,
                [
                    ("ev-1", "journalctl: typha (pid 4830) holds 0.0.0.0:5473"),
                    ("ev-2", "crun state dir is scoped per runtime root"),
                ],
            )
            .unwrap(),
        )
        .unwrap();
        TaskConstraints::default().with_output_contract(
            OutputContract::json_object("support-contract", BTreeMap::new())
                .unwrap()
                .with_evidence_grounding(rule),
        )
    }

    #[tokio::test]
    async fn support_validator_passes_without_configuration() {
        let v = ClaimsEvidenceSupportedValidator::new(None);
        // No contract at all.
        let r = v
            .validate("free prose", &TaskConstraints::default())
            .await
            .unwrap();
        assert!(r.passed());
        assert_eq!(r.kind(), "claims-evidence-supported");
        assert!(r.summary().contains("no semantic support configured"));
        // A grounding rule without a semantic-support rule is also a no-op.
        let r = v
            .validate("free prose", &grounded_constraints())
            .await
            .unwrap();
        assert!(r.passed());
    }

    #[tokio::test]
    async fn support_demanded_without_a_judge_fails_the_step_loudly() {
        let v = ClaimsEvidenceSupportedValidator::new(None);
        let err = v
            .validate(r#"{"claims":[]}"#, &supported_constraints(70))
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { reason }
            if reason.contains("no evidence-support judge is wired")));
    }

    #[tokio::test]
    async fn supported_claims_pass_and_verdicts_ride_the_details() {
        let judge = ScriptedJudge {
            verdicts: BTreeMap::from([("typha holds the port".to_owned(), verdict(true, 92))]),
            ..Default::default()
        };
        let v = ClaimsEvidenceSupportedValidator::new(Some(Arc::new(judge)));
        let content = r#"{"claims":[{"text":"typha holds the port","evidence_refs":["ev-1"]}]}"#;
        let r = v
            .validate(content, &supported_constraints(70))
            .await
            .unwrap();
        assert!(r.passed(), "summary: {}", r.summary());
        let details = serde_json::to_string(r.details()).unwrap();
        assert!(details.contains("\"supported\":true"));
        assert!(details.contains("\"confidence\":92"));
        assert!(details.contains("scripted"));
    }

    #[tokio::test]
    async fn unsupported_claim_fails_and_is_named() {
        let judge = ScriptedJudge {
            verdicts: BTreeMap::from([
                ("grounded and true".to_owned(), verdict(true, 90)),
                (
                    "cites real refs, says nonsense".to_owned(),
                    verdict(false, 88),
                ),
            ]),
            ..Default::default()
        };
        let v = ClaimsEvidenceSupportedValidator::new(Some(Arc::new(judge)));
        let content = r#"{"claims":[
            {"text":"grounded and true","evidence_refs":["ev-1"]},
            {"text":"cites real refs, says nonsense","evidence_refs":["ev-1","ev-2"]}
        ]}"#;
        let r = v
            .validate(content, &supported_constraints(70))
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("1 of 2 claims"));
        let details = serde_json::to_string(r.details()).unwrap();
        assert!(details.contains("does not support the claim"));
    }

    #[tokio::test]
    async fn low_confidence_support_fails_under_the_threshold() {
        let judge = ScriptedJudge {
            verdicts: BTreeMap::from([("maybe".to_owned(), verdict(true, 55))]),
            ..Default::default()
        };
        let v = ClaimsEvidenceSupportedValidator::new(Some(Arc::new(judge)));
        let content = r#"{"claims":[{"text":"maybe","evidence_refs":["ev-1"]}]}"#;
        let r = v
            .validate(content, &supported_constraints(70))
            .await
            .unwrap();
        assert!(!r.passed());
        let details = serde_json::to_string(r.details()).unwrap();
        assert!(details.contains("below min_confidence"));
    }

    #[tokio::test]
    async fn judge_only_sees_the_evidence_the_claim_cited() {
        let judge = Arc::new(ScriptedJudge {
            verdicts: BTreeMap::from([("narrow".to_owned(), verdict(true, 95))]),
            ..Default::default()
        });
        let v = ClaimsEvidenceSupportedValidator::new(Some(judge.clone()));
        let content = r#"{"claims":[{"text":"narrow","evidence_refs":["ev-2"]}]}"#;
        v.validate(content, &supported_constraints(70))
            .await
            .unwrap();
        let asked = judge.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].1, vec!["ev-2".to_owned()]);
    }

    #[tokio::test]
    async fn claim_citing_only_orphan_refs_fails_without_a_judge_call() {
        let judge = Arc::new(ScriptedJudge::default());
        let v = ClaimsEvidenceSupportedValidator::new(Some(judge.clone()));
        let content = r#"{"claims":[{"text":"invented","evidence_refs":["ev-999"]}]}"#;
        let r = v
            .validate(content, &supported_constraints(70))
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(judge.asked.lock().unwrap().is_empty());
        let details = serde_json::to_string(r.details()).unwrap();
        assert!(details.contains("no evidence with a judgeable body"));
    }

    #[tokio::test]
    async fn judge_failure_fails_the_step_closed() {
        // Empty verdict table: any claim makes the scripted judge error.
        let judge = Arc::new(ScriptedJudge::default());
        let v = ClaimsEvidenceSupportedValidator::new(Some(judge));
        let content = r#"{"claims":[{"text":"anything","evidence_refs":["ev-1"]}]}"#;
        let err = v
            .validate(content, &supported_constraints(70))
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    #[tokio::test]
    async fn empty_claims_array_passes_the_support_gate() {
        let v = ClaimsEvidenceSupportedValidator::new(Some(Arc::new(ScriptedJudge::default())));
        let r = v
            .validate(r#"{"claims":[]}"#, &supported_constraints(70))
            .await
            .unwrap();
        assert!(r.passed());
    }

    #[tokio::test]
    async fn missing_claims_field_fails_the_support_gate() {
        let v = ClaimsEvidenceSupportedValidator::new(Some(Arc::new(ScriptedJudge::default())));
        let r = v
            .validate(r#"{"decision":"accept"}"#, &supported_constraints(70))
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("claims"));
    }

    #[tokio::test]
    async fn non_empty_content_passes() {
        let v = ContentNonEmptyValidator::new();
        let r = v
            .validate("hello", &TaskConstraints::default())
            .await
            .unwrap();
        assert!(r.passed());
        assert_eq!(r.kind(), "content-non-empty");
    }

    #[tokio::test]
    async fn whitespace_only_content_fails() {
        let v = ContentNonEmptyValidator::new();
        let r = v
            .validate("   \n\t ", &TaskConstraints::default())
            .await
            .unwrap();
        assert!(!r.passed());
    }

    #[tokio::test]
    async fn empty_string_fails() {
        let v = ContentNonEmptyValidator::new();
        let r = v.validate("", &TaskConstraints::default()).await.unwrap();
        assert!(!r.passed());
    }

    #[tokio::test]
    async fn json_object_validator_accepts_valid_object() {
        let v = JsonObjectOutputValidator::new();
        let r = v
            .validate(r#"{"decision":"emit_event"}"#, &structured_constraints())
            .await
            .unwrap();
        assert!(r.passed());
    }

    #[tokio::test]
    async fn json_object_validator_rejects_non_json() {
        let v = JsonObjectOutputValidator::new();
        let r = v
            .validate("not json", &structured_constraints())
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("not valid JSON"));
    }

    #[tokio::test]
    async fn required_fields_validator_rejects_missing_fields() {
        let v = RequiredFieldsValidator::new();
        let r = v
            .validate(r#"{"decision":"emit_event"}"#, &structured_constraints())
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("reason"));
    }

    #[tokio::test]
    async fn allowed_string_values_validator_rejects_unknown_value() {
        let v = AllowedStringValuesValidator::new();
        let r = v
            .validate(
                r#"{"decision":"drop_everything","reason":"missing evidence"}"#,
                &structured_constraints(),
            )
            .await
            .unwrap();
        assert!(!r.passed());
        assert_eq!(r.kind(), "output-allowed-string-values");
    }

    #[tokio::test]
    async fn contract_validators_are_noops_without_contract() {
        let constraints = TaskConstraints::default();
        for report in [
            JsonObjectOutputValidator::new()
                .validate("plain text", &constraints)
                .await
                .unwrap(),
            RequiredFieldsValidator::new()
                .validate("plain text", &constraints)
                .await
                .unwrap(),
            AllowedStringValuesValidator::new()
                .validate("plain text", &constraints)
                .await
                .unwrap(),
            JsonSchemaValidator::new()
                .validate("plain text", &constraints)
                .await
                .unwrap(),
        ] {
            assert!(report.passed());
        }
    }

    fn schema_constraints(schema_body: &str) -> TaskConstraints {
        TaskConstraints::default().with_output_contract(
            OutputContract::new_with_schema(
                "schema-contract",
                choreo_core::value_objects::OutputFormat::JsonObject,
                BTreeMap::new(),
                schema_body,
            )
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn json_schema_validator_is_noop_without_embedded_schema() {
        // structured_constraints has no JSON Schema attached.
        let v = JsonSchemaValidator::new();
        let r = v
            .validate(r#"{"any": "shape"}"#, &structured_constraints())
            .await
            .unwrap();
        assert!(r.passed());
        assert_eq!(r.kind(), "output-json-schema");
        assert!(r.summary().contains("no embedded JSON Schema"));
    }

    #[tokio::test]
    async fn json_schema_validator_accepts_satisfying_output() {
        let schema = r#"{
            "type": "object",
            "required": ["decision", "reason"],
            "properties": {
                "decision": { "type": "string", "enum": ["emit_event", "escalate"] },
                "reason": { "type": "string", "minLength": 1 }
            },
            "additionalProperties": false
        }"#;
        let v = JsonSchemaValidator::new();
        let r = v
            .validate(
                r#"{"decision":"emit_event","reason":"clear signal"}"#,
                &schema_constraints(schema),
            )
            .await
            .unwrap();
        assert!(r.passed(), "summary: {}", r.summary());
    }

    #[tokio::test]
    async fn json_schema_validator_rejects_missing_required_field() {
        let schema = r#"{
            "type": "object",
            "required": ["decision", "reason"],
            "properties": {
                "decision": { "type": "string" },
                "reason": { "type": "string" }
            }
        }"#;
        let v = JsonSchemaValidator::new();
        let r = v
            .validate(r#"{"decision":"emit_event"}"#, &schema_constraints(schema))
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("schema violation"));
    }

    #[tokio::test]
    async fn json_schema_validator_rejects_violation_of_max_items() {
        // bounded shape: maxItems on findings. Subsumes the
        // "bounded event proposal shape" deliverable.
        let schema = r#"{
            "type": "object",
            "properties": {
                "findings": {
                    "type": "array",
                    "maxItems": 2,
                    "items": { "type": "string", "maxLength": 64 }
                }
            }
        }"#;
        let v = JsonSchemaValidator::new();
        let r = v
            .validate(r#"{"findings":["a","b","c"]}"#, &schema_constraints(schema))
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("schema violation"));
    }

    #[tokio::test]
    async fn json_schema_validator_reports_malformed_schema() {
        let v = JsonSchemaValidator::new();
        let r = v
            .validate(
                r#"{"decision":"emit_event"}"#,
                &schema_constraints("not a json schema"),
            )
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("not valid JSON"));
    }

    #[tokio::test]
    async fn json_schema_validator_reports_malformed_proposal() {
        let v = JsonSchemaValidator::new();
        let r = v
            .validate(
                "not json at all",
                &schema_constraints(r#"{"type":"object"}"#),
            )
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("proposal is not valid JSON"));
    }

    // ---- BoundedEventShapeValidator -----------------------------------

    fn unconstrained() -> TaskConstraints {
        TaskConstraints::default()
    }

    #[tokio::test]
    async fn bounded_event_shape_is_noop_without_contract() {
        let v = BoundedEventShapeValidator::new();
        let r = v.validate(r#"{"k":"v"}"#, &unconstrained()).await.unwrap();
        assert!(r.passed());
        assert!(r.summary().contains("no structured output contract"));
    }

    #[tokio::test]
    async fn bounded_event_shape_accepts_modest_payloads() {
        let v = BoundedEventShapeValidator::new();
        let r = v
            .validate(
                r#"{"decision":"emit_event","reason":"clear"}"#,
                &structured_constraints(),
            )
            .await
            .unwrap();
        assert!(r.passed(), "summary: {}", r.summary());
        assert_eq!(r.kind(), "output-bounded-event-shape");
    }

    #[tokio::test]
    async fn bounded_event_shape_rejects_oversized_payloads() {
        let mut bloated = String::from("{\"k\":\"");
        bloated.push_str(&"a".repeat(2048));
        bloated.push_str("\"}");
        let v = BoundedEventShapeValidator::new().with_max_total_size_bytes(512);
        let r = v
            .validate(&bloated, &structured_constraints())
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("limit is 512"));
    }

    #[tokio::test]
    async fn bounded_event_shape_rejects_too_many_object_keys() {
        use std::fmt::Write as _;
        let mut payload = String::from("{");
        for i in 0..40 {
            if i > 0 {
                payload.push(',');
            }
            write!(payload, r#""k{i}":{i}"#).unwrap();
        }
        payload.push('}');
        let v = BoundedEventShapeValidator::new().with_max_object_keys(32);
        let r = v
            .validate(&payload, &structured_constraints())
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("max_object_keys"));
    }

    #[tokio::test]
    async fn bounded_event_shape_rejects_too_deep_nesting() {
        // 6 levels of nesting around a literal.
        let payload = r#"{"a":{"b":{"c":{"d":{"e":{"f":1}}}}}}"#;
        let v = BoundedEventShapeValidator::new().with_max_depth(4);
        let r = v
            .validate(payload, &structured_constraints())
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("max_depth"));
    }

    #[tokio::test]
    async fn bounded_event_shape_rejects_long_strings() {
        let payload = format!(r#"{{"reason":"{}"}}"#, "x".repeat(200));
        let v = BoundedEventShapeValidator::new().with_max_string_len(64);
        let r = v
            .validate(&payload, &structured_constraints())
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("max_string_len"));
    }

    #[tokio::test]
    async fn bounded_event_shape_rejects_huge_arrays() {
        let nums = (0..50).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        let payload = format!(r#"{{"items":[{nums}]}}"#);
        let v = BoundedEventShapeValidator::new().with_max_array_len(10);
        let r = v
            .validate(&payload, &structured_constraints())
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("max_array_len"));
    }

    #[tokio::test]
    async fn bounded_event_shape_rejects_invalid_json() {
        let v = BoundedEventShapeValidator::new();
        let r = v
            .validate("not json", &structured_constraints())
            .await
            .unwrap();
        assert!(!r.passed());
        assert!(r.summary().contains("not valid JSON"));
    }
}
