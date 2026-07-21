//! Immutable evidence returned by one host-provided ceremony source.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::DomainError;
use crate::value_objects::{Attributes, CeremonyEvidenceSourceId, CeremonyInterventionContent};

use super::ExternalContextBundle;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyEvidencePack {
    source_id: CeremonyEvidenceSourceId,
    bundle: ExternalContextBundle,
    #[serde(with = "time::serde::rfc3339")]
    collected_at: OffsetDateTime,
}

impl CeremonyEvidencePack {
    pub fn new(
        source_id: CeremonyEvidenceSourceId,
        bundle: ExternalContextBundle,
        collected_at: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        if bundle.summary().is_none() {
            return Err(DomainError::EmptyField {
                field: "ceremony_evidence_pack.summary",
            });
        }
        if bundle.items().is_empty() && bundle.references().is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "ceremony_evidence_pack.evidence",
            });
        }
        Ok(Self {
            source_id,
            bundle,
            collected_at,
        })
    }

    #[must_use]
    pub fn source_id(&self) -> &CeremonyEvidenceSourceId {
        &self.source_id
    }

    #[must_use]
    pub fn bundle(&self) -> &ExternalContextBundle {
        &self.bundle
    }

    #[must_use]
    pub fn collected_at(&self) -> OffsetDateTime {
        self.collected_at
    }

    pub fn intervention_content(&self) -> Result<CeremonyInterventionContent, DomainError> {
        let summary = self
            .bundle
            .summary()
            .ok_or(DomainError::InvariantViolated {
                reason: "ceremony evidence pack must retain its summary",
            })?;
        let serialized =
            serde_json::to_value(self).map_err(|_| DomainError::InvariantViolated {
                reason: "ceremony evidence pack could not be represented as attributes",
            })?;
        let details = Attributes::new(BTreeMap::from([("evidence_pack".to_owned(), serialized)]))?;
        CeremonyInterventionContent::new(summary.text(), details)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{ContextItem, ContextSummary};
    use time::macros::datetime;

    fn bundle(items: Vec<ContextItem>) -> ExternalContextBundle {
        ExternalContextBundle::new(
            "observability-1",
            "1.0",
            Some(ContextSummary::new("Error rate increased.", Attributes::empty()).unwrap()),
            items,
            Vec::new(),
            Attributes::empty(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_an_empty_pack_even_when_it_has_a_summary() {
        let error = CeremonyEvidencePack::new(
            CeremonyEvidenceSourceId::new("observability").unwrap(),
            bundle(Vec::new()),
            datetime!(2026-07-21 18:00:00 UTC),
        )
        .unwrap_err();

        assert!(matches!(error, DomainError::EmptyCollection { .. }));
    }

    #[test]
    fn builds_intervention_content_with_the_typed_pack() {
        let item = ContextItem::new(
            "error-rate",
            "metric",
            "Checkout error rate",
            Some("Error rate is 18%.".to_owned()),
            Attributes::empty(),
            Vec::new(),
        )
        .unwrap();
        let pack = CeremonyEvidencePack::new(
            CeremonyEvidenceSourceId::new("observability").unwrap(),
            bundle(vec![item]),
            datetime!(2026-07-21 18:00:00 UTC),
        )
        .unwrap();

        let content = pack.intervention_content().unwrap();

        assert_eq!(content.message(), "Error rate increased.");
        assert_eq!(
            content.details().as_map()["evidence_pack"]["source_id"],
            "observability"
        );
    }
}
