use super::CeremonyValidationFinding;

/// The complete outcome of analysing a ceremony definition.
///
/// Analysis collects every defect instead of stopping at the first one.
/// A boolean, or a single error, does not carry enough information for
/// an author to correct a draft in one pass.
///
/// Findings preserve the order in which the checks run, so the first
/// blocking finding is the error fail-fast construction would raise.
///
/// Deliberately not serializable, for the same reason as
/// [`CeremonyValidationFinding`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CeremonyValidationReport {
    findings: Vec<CeremonyValidationFinding>,
}

impl CeremonyValidationReport {
    #[must_use]
    pub fn new(findings: impl IntoIterator<Item = CeremonyValidationFinding>) -> Self {
        Self {
            findings: findings.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn findings(&self) -> &[CeremonyValidationFinding] {
        &self.findings
    }

    pub fn errors(&self) -> impl Iterator<Item = &CeremonyValidationFinding> {
        self.findings.iter().filter(|finding| finding.is_blocking())
    }

    pub fn warnings(&self) -> impl Iterator<Item = &CeremonyValidationFinding> {
        self.findings
            .iter()
            .filter(|finding| !finding.is_blocking())
    }

    /// The first blocking finding, in check order.
    #[must_use]
    pub fn first_error(&self) -> Option<&CeremonyValidationFinding> {
        self.findings.iter().find(|finding| finding.is_blocking())
    }

    /// A definition is publishable only when no finding blocks it.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.first_error().is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DomainError;
    use crate::value_objects::{CeremonyValidationLocus, CeremonyValidationSeverity, StateId};

    fn state_id(value: &str) -> StateId {
        StateId::new(value).expect("valid state id")
    }

    fn blocking() -> CeremonyValidationFinding {
        CeremonyValidationFinding::error(
            CeremonyValidationLocus::state(state_id("draft")),
            DomainError::NotFound {
                what: "ceremony_transition.to_state",
            },
        )
    }

    fn advisory() -> CeremonyValidationFinding {
        CeremonyValidationFinding::warning(
            CeremonyValidationLocus::Definition,
            DomainError::InvariantViolated {
                reason: "advisory only",
            },
        )
    }

    #[test]
    fn an_empty_report_is_valid() {
        let report = CeremonyValidationReport::default();

        assert!(report.is_valid());
        assert!(report.findings().is_empty());
        assert!(report.first_error().is_none());
    }

    #[test]
    fn warnings_alone_do_not_block() {
        let report = CeremonyValidationReport::new([advisory()]);

        assert!(report.is_valid());
        assert_eq!(report.warnings().count(), 1);
        assert_eq!(report.errors().count(), 0);
    }

    #[test]
    fn any_error_blocks_the_definition() {
        let report = CeremonyValidationReport::new([advisory(), blocking()]);

        assert!(!report.is_valid());
        assert_eq!(report.errors().count(), 1);
    }

    #[test]
    fn first_error_skips_preceding_warnings() {
        let report = CeremonyValidationReport::new([advisory(), blocking(), advisory()]);
        let first = report.first_error().expect("a blocking finding");

        assert_eq!(first.severity(), CeremonyValidationSeverity::Error);
        assert_eq!(
            first.defect(),
            &DomainError::NotFound {
                what: "ceremony_transition.to_state",
            }
        );
    }

    #[test]
    fn findings_preserve_check_order() {
        let report = CeremonyValidationReport::new([blocking(), advisory()]);

        assert_eq!(report.findings().len(), 2);
        assert!(report.findings()[0].is_blocking());
        assert!(!report.findings()[1].is_blocking());
    }
}
