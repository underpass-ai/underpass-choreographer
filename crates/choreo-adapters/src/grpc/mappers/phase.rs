//! [`DeliberationPhase`] ↔ proto enum conversion.
//!
//! The domain FSM is 5-state (Proposing, Revising, Validating,
//! Scoring, Completed); the proto enum matches it after the
//! API-alignment slice.

use choreo_core::entities::DeliberationPhase;
use choreo_proto::v1 as pb;

/// Not wired today: the non-streaming handlers do not emit
/// `DeliberationPhase` on the wire. `StreamDeliberation` (future) and
/// diagnostics (future) will call this. Marked `allow(dead_code)` so
/// the mapping stays centralised until then.
#[allow(dead_code)]
#[must_use]
pub fn proto_phase_from_domain(phase: DeliberationPhase) -> pb::DeliberationPhase {
    match phase {
        DeliberationPhase::Proposing => pb::DeliberationPhase::Proposing,
        DeliberationPhase::Revising => pb::DeliberationPhase::Revising,
        DeliberationPhase::Validating => pb::DeliberationPhase::Validating,
        DeliberationPhase::Scoring => pb::DeliberationPhase::Scoring,
        DeliberationPhase::Completed => pb::DeliberationPhase::Completed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_domain_phase_has_a_distinct_proto_value() {
        use std::collections::HashSet;
        let mapped: HashSet<i32> = [
            DeliberationPhase::Proposing,
            DeliberationPhase::Revising,
            DeliberationPhase::Validating,
            DeliberationPhase::Scoring,
            DeliberationPhase::Completed,
        ]
        .iter()
        .map(|p| proto_phase_from_domain(*p) as i32)
        .collect();
        assert_eq!(mapped.len(), 5);
    }
}
