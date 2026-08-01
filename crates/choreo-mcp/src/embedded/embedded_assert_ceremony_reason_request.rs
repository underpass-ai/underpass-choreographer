//! Saying why one thing a session produced led to another, over stdio.

use choreo_app::usecases::AssertCeremonyReasonInput;
use choreo_core::value_objects::{
    CeremonyId, CeremonyInterventionId, CeremonyReasonKind, CeremonyRecordRef, GuardName,
    MemoryConfidence, RoleId, StepId,
};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use super::embedded_request_fields::required_string;

pub(super) struct EmbeddedAssertCeremonyReasonRequest {
    input: AssertCeremonyReasonInput,
    ceremony_id: CeremonyId,
}

impl EmbeddedAssertCeremonyReasonRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        choreographer
            .assert_reason(self.input)
            .await
            .map_err(|error| format!("failed to record why: {error}"))?;
        Ok(self.ceremony_id)
    }
}

/// One end of a reason. Only the field the kind names is read.
fn record_ref(value: Option<&Value>, field: &str) -> Result<CeremonyRecordRef, String> {
    let object = value
        .and_then(Value::as_object)
        .ok_or_else(|| format!("`{field}` must be an object"))?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{field}.kind` is required"))?;
    let text = |key: &str| -> Result<String, String> {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| format!("`{field}.{key}` is required for kind `{kind}`"))
    };
    let ordinal = || -> Result<u32, String> {
        object
            .get("ordinal")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| format!("`{field}.ordinal` is required for kind `{kind}`"))
    };

    Ok(match kind {
        "step" => CeremonyRecordRef::step(
            StepId::new(text("step_id")?).map_err(|error| stringify(&error))?,
        ),
        "agenda_item" => CeremonyRecordRef::agenda_item(
            CeremonyInterventionId::new(text("agenda_item")?).map_err(|error| stringify(&error))?,
        ),
        "contribution" => CeremonyRecordRef::contribution(
            CeremonyInterventionId::new(text("agenda_item")?).map_err(|error| stringify(&error))?,
            ordinal()?,
        ),
        "guard_decision" => CeremonyRecordRef::guard_decision(
            GuardName::new(text("guard_name")?).map_err(|error| stringify(&error))?,
        ),
        "transition" => CeremonyRecordRef::transition(ordinal()?),
        other => return Err(format!("`{field}.kind` does not name anything: {other}")),
    })
}

fn reason_kind(raw: &str) -> Result<CeremonyReasonKind, String> {
    Ok(match raw {
        "chosen_because" => CeremonyReasonKind::ChosenBecause,
        "achieved_by" => CeremonyReasonKind::AchievedBy,
        "follows_from" => CeremonyReasonKind::FollowsFrom,
        "satisfies_constraint" => CeremonyReasonKind::SatisfiesConstraint,
        "violates_constraint" => CeremonyReasonKind::ViolatesConstraint,
        "supersedes" => CeremonyReasonKind::Supersedes,
        "contradicts" => CeremonyReasonKind::Contradicts,
        "answers" => {
            return Err(
                "`answers` states the shape of the session rather than anyone's judgement, \
                 and only the engine asserts it"
                    .to_owned(),
            )
        }
        other => return Err(format!("`kind` does not name a reason: {other}")),
    })
}

fn confidence(raw: &str) -> Result<MemoryConfidence, String> {
    Ok(match raw {
        "high" => MemoryConfidence::High,
        "medium" => MemoryConfidence::Medium,
        "low" => MemoryConfidence::Low,
        other => {
            return Err(format!(
                "`confidence` must be high, medium or low, not {other}"
            ))
        }
    })
}

fn stringify(error: &choreo_core::error::DomainError) -> String {
    error.to_string()
}

impl TryFrom<&Value> for EmbeddedAssertCeremonyReasonRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        let ceremony_id = CeremonyId::new(required_string(object, "ceremony_id")?)
            .map_err(|error| stringify(&error))?;
        Ok(Self {
            input: AssertCeremonyReasonInput::new(
                ceremony_id.clone(),
                RoleId::new(required_string(object, "role_id")?)
                    .map_err(|error| stringify(&error))?,
                record_ref(object.get("from"), "from")?,
                record_ref(object.get("to"), "to")?,
                reason_kind(&required_string(object, "kind")?)?,
                required_string(object, "why")?,
                confidence(&required_string(object, "confidence")?)?,
            ),
            ceremony_id,
        })
    }
}
