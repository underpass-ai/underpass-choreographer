use crate::error::DomainError;

use super::{CeremonyValidationLocus, CeremonyValidationSeverity};

/// One defect found while analysing a ceremony definition.
///
/// The defect keeps the typed [`DomainError`] that describes it, so a
/// blocking finding can be surfaced as the exact error a caller would
/// have received from fail-fast construction.
///
/// Deliberately not serializable: it carries a [`DomainError`], and
/// serialization belongs to the adapter layer. An adapter renders the
/// finding into its own wire shape.
#[derive(Debug, Clone, PartialEq)]
pub struct CeremonyValidationFinding {
    severity: CeremonyValidationSeverity,
    locus: CeremonyValidationLocus,
    defect: DomainError,
}

impl CeremonyValidationFinding {
    #[must_use]
    pub fn error(locus: CeremonyValidationLocus, defect: DomainError) -> Self {
        Self {
            severity: CeremonyValidationSeverity::Error,
            locus,
            defect,
        }
    }

    #[must_use]
    pub fn warning(locus: CeremonyValidationLocus, defect: DomainError) -> Self {
        Self {
            severity: CeremonyValidationSeverity::Warning,
            locus,
            defect,
        }
    }

    #[must_use]
    pub fn severity(&self) -> CeremonyValidationSeverity {
        self.severity
    }

    #[must_use]
    pub fn locus(&self) -> &CeremonyValidationLocus {
        &self.locus
    }

    #[must_use]
    pub fn defect(&self) -> &DomainError {
        &self.defect
    }

    #[must_use]
    pub fn is_blocking(&self) -> bool {
        self.severity.is_error()
    }
}
