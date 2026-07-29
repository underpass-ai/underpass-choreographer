//! [`AuditChain`] — verification of a ceremony's journal.
//!
//! The verifier depends on nothing but the records. It does not know
//! which store produced them, so a host cannot satisfy it by asserting
//! its own integrity: the answer comes from the bytes that were
//! written, which is the point of chaining them in the first place.

use crate::value_objects::{AuditChainDefect, AuditChainVerdict, AuditSequence};

use super::AuditRecord;

/// Stateless verification of an ordered journal.
#[derive(Debug)]
pub struct AuditChain;

impl AuditChain {
    /// Verify records given in the order the journal returned them.
    ///
    /// An empty journal is intact: nothing was written, so nothing was
    /// tampered with. Verification stops at the first defect — past it,
    /// no statement about the remaining records would be sound.
    #[must_use]
    pub fn verify(records: &[AuditRecord]) -> AuditChainVerdict {
        let Some((first, rest)) = records.split_first() else {
            return AuditChainVerdict::Intact;
        };

        if !first.sequence().is_first() {
            return AuditChainVerdict::Broken(AuditChainDefect::DoesNotStartAtTheBeginning {
                found: first.sequence(),
            });
        }
        if first.previous_record_hash().is_some() {
            return AuditChainVerdict::Broken(AuditChainDefect::UnexpectedRoot {
                at: first.sequence(),
            });
        }
        if let Some(verdict) = digest_defect(first) {
            return verdict;
        }

        let mut previous = first;
        for record in rest {
            if record.ceremony_id() != previous.ceremony_id() {
                return AuditChainVerdict::Broken(AuditChainDefect::ForeignCeremony {
                    at: record.sequence(),
                });
            }
            if !record.sequence().follows(previous.sequence()) {
                return AuditChainVerdict::Broken(AuditChainDefect::SequenceBroken {
                    expected: previous.sequence().next(),
                    found: record.sequence(),
                });
            }
            match record.previous_record_hash() {
                None => {
                    return AuditChainVerdict::Broken(AuditChainDefect::UnexpectedRoot {
                        at: record.sequence(),
                    })
                }
                Some(hash) if hash != previous.record_hash() => {
                    return AuditChainVerdict::Broken(AuditChainDefect::LinkBroken {
                        at: record.sequence(),
                    })
                }
                Some(_) => {}
            }
            if let Some(verdict) = digest_defect(record) {
                return verdict;
            }
            previous = record;
        }

        AuditChainVerdict::Intact
    }

    /// The position the next record must occupy.
    #[must_use]
    pub fn next_sequence(records: &[AuditRecord]) -> AuditSequence {
        records
            .last()
            .map_or(AuditSequence::FIRST, |record| record.sequence().next())
    }
}

/// A record whose digest cannot even be recomputed is treated as
/// altered: an unrenderable timestamp means the stored bytes are not
/// what this implementation would have written.
fn digest_defect(record: &AuditRecord) -> Option<AuditChainVerdict> {
    match record.digest_is_intact() {
        Ok(true) => None,
        Ok(false) | Err(_) => Some(AuditChainVerdict::Broken(AuditChainDefect::DigestAltered {
            at: record.sequence(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::AuditFact;
    use crate::value_objects::{
        AuditActor, AuditActorKind, AuditEventType, CeremonyId, CeremonyName, CeremonyVersion,
        EventId,
    };
    use serde_json::Value;
    use time::macros::datetime;

    fn fact(event_id: &str, ceremony: &str) -> AuditFact {
        AuditFact {
            event_id: EventId::new(event_id).unwrap(),
            event_type: AuditEventType::StepCompleted,
            ceremony_id: CeremonyId::new(ceremony).unwrap(),
            definition_name: CeremonyName::new("planning_ceremony").unwrap(),
            definition_version: CeremonyVersion::v1(),
            occurred_at: datetime!(2026-07-29 09:00:00 UTC),
            actor: AuditActor::new("engineer-1", AuditActorKind::Human, None).unwrap(),
            correlation_id: None,
            causation_id: None,
            trace: None,
        }
    }

    fn chain() -> Vec<AuditRecord> {
        let first = AuditRecord::first(fact("e1", "ceremony-1")).unwrap();
        let second = AuditRecord::following(fact("e2", "ceremony-1"), &first).unwrap();
        let third = AuditRecord::following(fact("e3", "ceremony-1"), &second).unwrap();
        vec![first, second, third]
    }

    fn tampered(record: &AuditRecord, mutate: impl FnOnce(&mut Value)) -> AuditRecord {
        let mut json = serde_json::to_value(record).unwrap();
        mutate(&mut json);
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn an_empty_journal_is_intact() {
        assert!(AuditChain::verify(&[]).is_intact());
        assert_eq!(AuditChain::next_sequence(&[]), AuditSequence::FIRST);
    }

    #[test]
    fn a_well_formed_journal_is_intact() {
        let records = chain();

        assert!(AuditChain::verify(&records).is_intact());
        assert_eq!(AuditChain::next_sequence(&records).value(), 4);
    }

    #[test]
    fn a_journal_missing_its_opening_records_is_detected() {
        let records = chain();

        assert_eq!(
            AuditChain::verify(&records[1..]).defect(),
            Some(AuditChainDefect::DoesNotStartAtTheBeginning {
                found: AuditSequence::new(2).unwrap()
            })
        );
    }

    #[test]
    fn a_record_removed_from_the_middle_is_detected() {
        let records = chain();
        let gapped = vec![records[0].clone(), records[2].clone()];

        assert_eq!(
            AuditChain::verify(&gapped).defect(),
            Some(AuditChainDefect::SequenceBroken {
                expected: AuditSequence::new(2).unwrap(),
                found: AuditSequence::new(3).unwrap(),
            })
        );
    }

    #[test]
    fn an_altered_record_is_detected_at_its_own_position() {
        let mut records = chain();
        records[1] = tampered(&records[1], |json| {
            json["actor"]["actor_id"] = "someone-else".into();
        });

        assert_eq!(
            AuditChain::verify(&records).defect(),
            Some(AuditChainDefect::DigestAltered {
                at: AuditSequence::new(2).unwrap()
            })
        );
    }

    #[test]
    fn a_substituted_record_breaks_the_link_of_the_one_after_it() {
        let records = chain();
        let forged = AuditRecord::following(fact("forged", "ceremony-1"), &records[0]).unwrap();
        let substituted = vec![records[0].clone(), forged, records[2].clone()];

        // The forgery is internally sound — it is the third record,
        // still naming the digest of the record it really followed,
        // that exposes the substitution.
        assert_eq!(
            AuditChain::verify(&substituted).defect(),
            Some(AuditChainDefect::LinkBroken {
                at: AuditSequence::new(3).unwrap()
            })
        );
    }

    #[test]
    fn a_grafted_foreign_record_is_detected() {
        let records = chain();
        let foreign = AuditRecord::first(fact("f1", "ceremony-2")).unwrap();
        let grafted = vec![records[0].clone(), foreign];

        assert_eq!(
            AuditChain::verify(&grafted).defect(),
            Some(AuditChainDefect::ForeignCeremony {
                at: AuditSequence::FIRST
            })
        );
    }

    #[test]
    fn an_opening_record_that_claims_a_predecessor_is_detected() {
        let records = chain();
        let rooted = tampered(&records[0], |json| {
            json["previous_record_hash"] = serde_json::to_value([9_u8; 32]).unwrap();
        });

        assert_eq!(
            AuditChain::verify(&[rooted]).defect(),
            Some(AuditChainDefect::UnexpectedRoot {
                at: AuditSequence::FIRST
            })
        );
    }

    #[test]
    fn a_later_record_that_claims_no_predecessor_is_detected() {
        let mut records = chain();
        records[1] = tampered(&records[1], |json| {
            json["previous_record_hash"] = Value::Null;
        });

        assert_eq!(
            AuditChain::verify(&records).defect(),
            Some(AuditChainDefect::UnexpectedRoot {
                at: AuditSequence::new(2).unwrap()
            })
        );
    }

    #[test]
    fn verification_stops_at_the_first_defect() {
        let mut records = chain();
        records[1] = tampered(&records[1], |json| {
            json["event_type"] = "step_failed".into();
        });
        records[2] = tampered(&records[2], |json| {
            json["event_type"] = "step_failed".into();
        });

        // Both are altered; only the earlier one is reported, because
        // past a break nothing further can be trusted.
        assert_eq!(
            AuditChain::verify(&records)
                .defect()
                .map(AuditChainDefect::at),
            Some(AuditSequence::new(2).unwrap())
        );
    }
}
