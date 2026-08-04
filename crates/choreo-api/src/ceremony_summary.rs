use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::InterventionView;

/// One seat at a ceremony, as a consumer sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyParticipant {
    pub role_id: String,
    pub specialty: String,
    pub bound_at_millis: i64,
}

/// One ceremony instance, as a consumer sees it.
///
/// A projection, never the aggregate. The instance inside the engine gains
/// fields as the domain needs them; a consumer that read it directly would
/// inherit each one as a contract. Everything here is plain data a consumer can
/// hold, log or map into its own vocabulary without importing the domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonySummary {
    pub ceremony_id: String,
    pub definition_name: String,
    pub definition_version: String,
    /// The digest of the published definition this instance was bound to, hex
    /// encoded — or absent for an instance started from an unpublished draft.
    /// Present, it makes "this exact procedure ran" provable rather than a
    /// promise about a name.
    pub definition_digest: Option<String>,
    pub current_state: String,
    pub participants: Vec<CeremonyParticipant>,
    /// The table's conversation: every intervention raised, with its answers.
    pub interventions: Vec<InterventionView>,
    /// The context the instance was started with. This is where a consuming
    /// product keeps its own reference to its own aggregate — the engine
    /// carries the keys without knowing what they mean.
    pub context: BTreeMap<String, serde_json::Value>,
    pub created_at_millis: i64,
    pub updated_at_millis: i64,
    pub completed_at_millis: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_survives_the_wire() {
        let summary = CeremonySummary {
            ceremony_id: "c-1".to_owned(),
            definition_name: "scope_discovery".to_owned(),
            definition_version: "1.0".to_owned(),
            definition_digest: Some("abc123".to_owned()),
            current_state: "STARTED".to_owned(),
            participants: vec![CeremonyParticipant {
                role_id: "FACILITATOR".to_owned(),
                specialty: "coordination".to_owned(),
                bound_at_millis: 1_700_000_000_000,
            }],
            interventions: Vec::new(),
            context: BTreeMap::from([(
                "requested_by".to_owned(),
                serde_json::Value::String("consumer-1".to_owned()),
            )]),
            created_at_millis: 1_700_000_000_000,
            updated_at_millis: 1_700_000_000_000,
            completed_at_millis: None,
        };
        let bytes = serde_json::to_vec(&summary).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<CeremonySummary>(&bytes).expect("deserializes"),
            summary
        );
    }

    #[test]
    fn an_unbound_instance_has_no_digest_rather_than_a_placeholder() {
        let summary = CeremonySummary {
            ceremony_id: "c-1".to_owned(),
            definition_name: "draft".to_owned(),
            definition_version: "1.0".to_owned(),
            definition_digest: None,
            current_state: "STARTED".to_owned(),
            participants: Vec::new(),
            interventions: Vec::new(),
            context: BTreeMap::new(),
            created_at_millis: 1,
            updated_at_millis: 1,
            completed_at_millis: None,
        };
        assert!(
            summary.definition_digest.is_none(),
            "a placeholder digest would let an unpublished draft read as provable"
        );
    }
}
