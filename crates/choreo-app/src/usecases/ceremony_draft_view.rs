//! [`CeremonyDraftView`] — what a draft is and what is wrong with it.
//!
//! Shared by every distribution for the same reason the instance view
//! is: an author asking "is this publishable, and why not" must get
//! the same answer whichever way they reached the engine. Counting a
//! draft's parts and explaining its defects are decisions about what
//! the answer *is*, not about how to encode it, so they belong here
//! and the encoding belongs to each transport.

use choreo_core::entities::CeremonyDefinitionDraft;
use choreo_core::value_objects::{
    CeremonyName, CeremonyValidationFinding, CeremonyValidationReport, CeremonyVersion,
};

/// How many of each element a draft declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CeremonyDraftSummary {
    pub states: usize,
    pub initial_states: usize,
    pub terminal_states: usize,
    pub transitions: usize,
    pub steps: usize,
    pub guards: usize,
    pub roles: usize,
}

pub struct CeremonyDraftView<'a> {
    draft: &'a CeremonyDefinitionDraft,
    report: &'a CeremonyValidationReport,
    summary: CeremonyDraftSummary,
    narrative: Vec<String>,
}

impl std::fmt::Debug for CeremonyDraftView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CeremonyDraftView")
            .field("summary", &self.summary)
            .finish_non_exhaustive()
    }
}

impl<'a> CeremonyDraftView<'a> {
    #[must_use]
    pub fn project(
        draft: &'a CeremonyDefinitionDraft,
        report: &'a CeremonyValidationReport,
    ) -> Self {
        let summary = CeremonyDraftSummary {
            states: draft.states().len(),
            initial_states: draft
                .states()
                .iter()
                .filter(|state| state.is_initial())
                .count(),
            terminal_states: draft
                .states()
                .iter()
                .filter(|state| state.is_terminal())
                .count(),
            transitions: draft.transitions().len(),
            steps: draft.steps().len(),
            guards: draft.guards().len(),
            roles: draft.roles().len(),
        };
        let narrative = narrate(draft, report, &summary);
        Self {
            draft,
            report,
            summary,
            narrative,
        }
    }

    #[must_use]
    pub fn name(&self) -> &CeremonyName {
        self.draft.name()
    }

    #[must_use]
    pub fn version(&self) -> &CeremonyVersion {
        self.draft.version()
    }

    /// Whether anything blocks publication. A draft with warnings and
    /// no errors is publishable and still worth reading about.
    #[must_use]
    pub fn is_publishable(&self) -> bool {
        self.report.is_valid()
    }

    #[must_use]
    pub fn error_count(&self) -> usize {
        self.report.errors().count()
    }

    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.report.warnings().count()
    }

    #[must_use]
    pub fn findings(&self) -> &[CeremonyValidationFinding] {
        self.report.findings()
    }

    #[must_use]
    pub const fn summary(&self) -> CeremonyDraftSummary {
        self.summary
    }

    /// The explanation, in the order it should be read: what the draft
    /// is, then what stops it, then what merely troubles it.
    #[must_use]
    pub fn narrative(&self) -> &[String] {
        &self.narrative
    }
}

fn narrate(
    draft: &CeremonyDefinitionDraft,
    report: &CeremonyValidationReport,
    summary: &CeremonyDraftSummary,
) -> Vec<String> {
    let mut lines = vec![format!(
        "`{}` declares {} states, {} transitions, {} steps, {} guards and {} roles.",
        draft.name().as_str(),
        summary.states,
        summary.transitions,
        summary.steps,
        summary.guards,
        summary.roles,
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
    format!("{} — at {}", finding.defect(), finding.locus())
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{
        CeremonyState, CeremonyTransition, StateId, TransitionTrigger,
    };

    fn state(id: &str) -> StateId {
        StateId::new(id).unwrap()
    }

    /// A draft whose only transition leads somewhere it never declared.
    fn broken_draft() -> CeremonyDefinitionDraft {
        CeremonyDefinitionDraft::new(
            CeremonyName::new("broken_ceremony").unwrap(),
            CeremonyVersion::v1(),
            None,
            Vec::new(),
            Vec::new(),
            vec![
                CeremonyState::initial(state("DRAFTING")),
                CeremonyState::terminal(state("DONE")),
            ],
            vec![CeremonyTransition::new(
                state("DRAFTING"),
                state("NOWHERE"),
                TransitionTrigger::new("finish").unwrap(),
                Vec::new(),
            )
            .unwrap()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn the_first_line_says_what_the_draft_is() {
        let draft = broken_draft();
        let report = draft.analyze();
        let view = CeremonyDraftView::project(&draft, &report);

        assert_eq!(
            view.narrative()[0],
            "`broken_ceremony` declares 2 states, 1 transitions, 0 steps, 0 guards and 0 roles."
        );
        assert_eq!(view.summary().states, 2);
        assert_eq!(view.summary().initial_states, 1);
        assert_eq!(view.summary().terminal_states, 1);
    }

    #[test]
    fn a_blocked_draft_says_so_and_names_each_element() {
        let draft = broken_draft();
        let report = draft.analyze();
        let view = CeremonyDraftView::project(&draft, &report);

        assert!(!view.is_publishable());
        assert!(view.error_count() > 0);
        assert!(view
            .narrative()
            .iter()
            .any(|line| line.contains("block publication")));
        // Each defect points at an element in words, not as a
        // serialized object quoted into the middle of a sentence.
        assert!(view
            .narrative()
            .iter()
            .any(|line| line.contains(" — at ") && !line.contains('{')));
    }
}
