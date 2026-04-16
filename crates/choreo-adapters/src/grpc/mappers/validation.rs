//! Validation: domain → proto.

use choreo_core::entities::{ValidationOutcome, ValidatorReport};
use choreo_proto::v1 as pb;

use super::attributes::attributes_to_struct;

#[must_use]
pub fn validator_report_to_proto(r: &ValidatorReport) -> pb::ValidatorReport {
    pb::ValidatorReport {
        kind: r.kind().to_owned(),
        passed: r.passed(),
        summary: r.summary().to_owned(),
        details: Some(attributes_to_struct(r.details())),
    }
}

/// Project a domain [`ValidationOutcome`] onto the proto message.
/// The domain does not carry a top-level `passed` boolean; we derive
/// it here from `reports` since the proto removed that field too.
/// (`passed` stays conceptually available to clients as
/// `reports.all(|r| r.passed)`.)
#[must_use]
pub fn validation_outcome_to_proto(o: &ValidationOutcome) -> pb::ValidationOutcome {
    pb::ValidationOutcome {
        score: o.score().get(),
        reports: o.reports().iter().map(validator_report_to_proto).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{Attributes, Score};

    fn report(kind: &str, passed: bool) -> ValidatorReport {
        ValidatorReport::new(kind, passed, "summary", Attributes::empty()).unwrap()
    }

    #[test]
    fn validator_report_fields_copy_through() {
        let r = report("lint", true);
        let pb = validator_report_to_proto(&r);
        assert_eq!(pb.kind, "lint");
        assert!(pb.passed);
        assert_eq!(pb.summary, "summary");
        assert!(pb.details.is_some());
    }

    #[test]
    fn outcome_score_and_reports_copy_through() {
        let o = ValidationOutcome::new(
            Score::new(0.75).unwrap(),
            vec![report("a", true), report("b", false)],
        );
        let pb = validation_outcome_to_proto(&o);
        assert_eq!(pb.score, 0.75);
        assert_eq!(pb.reports.len(), 2);
        assert!(pb.reports[0].passed);
        assert!(!pb.reports[1].passed);
    }
}
