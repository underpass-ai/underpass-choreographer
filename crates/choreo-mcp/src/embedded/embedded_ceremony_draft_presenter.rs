use choreo_core::entities::CeremonyDefinitionDraft;
use choreo_core::value_objects::{CeremonyValidationFinding, CeremonyValidationReport};
use serde_json::{json, Value};

/// Render a draft analysis for machine and human consumers.
///
/// The domain report is deliberately not serializable, so mapping it to
/// a wire shape happens here, at the adapter boundary.
pub(crate) struct EmbeddedCeremonyDraftPresenter;

impl EmbeddedCeremonyDraftPresenter {
    pub(crate) fn present_validation(
        draft: &CeremonyDefinitionDraft,
        report: &CeremonyValidationReport,
    ) -> Value {
        json!({
            "ceremony": draft.name().as_str(),
            "version": draft.version().as_str(),
            "publishable": report.is_valid(),
            "error_count": report.errors().count(),
            "warning_count": report.warnings().count(),
            "findings": report.findings().iter().map(present_finding).collect::<Vec<_>>(),
        })
    }

    pub(crate) fn present_explanation(
        draft: &CeremonyDefinitionDraft,
        report: &CeremonyValidationReport,
    ) -> Value {
        json!({
            "ceremony": draft.name().as_str(),
            "version": draft.version().as_str(),
            "publishable": report.is_valid(),
            "summary": summarize(draft),
            "narrative": narrate(draft, report),
        })
    }
}

fn present_finding(finding: &CeremonyValidationFinding) -> Value {
    json!({
        "severity": if finding.is_blocking() { "error" } else { "warning" },
        "locus": serde_json::to_value(finding.locus()).unwrap_or(Value::Null),
        "message": finding.defect().to_string(),
    })
}

fn summarize(draft: &CeremonyDefinitionDraft) -> Value {
    json!({
        "states": draft.states().len(),
        "initial_states": draft
            .states()
            .iter()
            .filter(|state| state.is_initial())
            .count(),
        "terminal_states": draft
            .states()
            .iter()
            .filter(|state| state.is_terminal())
            .count(),
        "transitions": draft.transitions().len(),
        "steps": draft.steps().len(),
        "guards": draft.guards().len(),
        "roles": draft.roles().len(),
    })
}

fn narrate(draft: &CeremonyDefinitionDraft, report: &CeremonyValidationReport) -> Vec<String> {
    let mut lines = vec![format!(
        "`{}` declares {} states, {} transitions, {} steps, {} guards and {} roles.",
        draft.name().as_str(),
        draft.states().len(),
        draft.transitions().len(),
        draft.steps().len(),
        draft.guards().len(),
        draft.roles().len(),
    )];

    let errors = report.errors().collect::<Vec<_>>();
    if errors.is_empty() {
        lines.push("Nothing blocks publication.".to_owned());
    } else {
        lines.push(format!(
            "{} defect(s) block publication; the draft cannot be published or executed until every one is fixed.",
            errors.len()
        ));
        lines.extend(errors.into_iter().map(describe));
    }

    let warnings = report.warnings().collect::<Vec<_>>();
    if !warnings.is_empty() {
        lines.push(format!(
            "{} warning(s) do not block publication but describe a ceremony that can stall.",
            warnings.len()
        ));
        lines.extend(warnings.into_iter().map(describe));
    }

    lines
}

fn describe(finding: &CeremonyValidationFinding) -> String {
    match serde_json::to_value(finding.locus()) {
        Ok(locus) => format!("{} — at {locus}", finding.defect()),
        Err(_) => finding.defect().to_string(),
    }
}
