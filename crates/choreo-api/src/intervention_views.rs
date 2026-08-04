use serde::{Deserialize, Serialize};

/// One answer given at the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterventionResponseView {
    pub role_id: String,
    pub content: String,
    pub responded_at_millis: i64,
}

/// One intervention — a question, investigation or proposed action put to the
/// table — as a consumer sees it.
///
/// The table's conversational unit. A projection, never the entity: responses
/// come with who answered and when, the request with who asked, and `open`
/// with whether the table still owes an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterventionView {
    pub intervention_id: String,
    /// `opinion`, `investigation` or `action`.
    pub kind: String,
    pub requested_by: String,
    /// The seats asked. Empty means the whole table.
    pub target_role_ids: Vec<String>,
    pub request: String,
    pub open: bool,
    pub responses: Vec<InterventionResponseView>,
    pub created_at_millis: i64,
    pub closed_at_millis: Option<i64>,
}

/// Put a question, investigation or proposed action to the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaiseInterventionRequest {
    pub ceremony_id: String,
    /// Chosen by the caller so a retried request raises the same intervention,
    /// not a second one.
    pub intervention_id: String,
    /// The seat speaking. A seat, not a person: the engine sees roles and
    /// cannot see what fills them.
    pub role_id: String,
    /// One of `human`, `agent`, `service`, `engine`.
    pub role_kind: String,
    /// One of `opinion`, `investigation`, `action`.
    pub kind: String,
    /// The seats asked. Empty asks the whole table.
    pub target_role_ids: Vec<String>,
    pub request: String,
}

/// Answer an open intervention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RespondToInterventionRequest {
    pub ceremony_id: String,
    pub intervention_id: String,
    pub role_id: String,
    pub role_kind: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_intervention_survives_the_wire() {
        let view = InterventionView {
            intervention_id: "iv-1".to_owned(),
            kind: "opinion".to_owned(),
            requested_by: "FACILITATOR".to_owned(),
            target_role_ids: vec!["REVIEWER".to_owned()],
            request: "Recommend the safest cleanup.".to_owned(),
            open: true,
            responses: vec![InterventionResponseView {
                role_id: "REVIEWER".to_owned(),
                content: "Rotate the certificate first.".to_owned(),
                responded_at_millis: 1_700_000_000_000,
            }],
            created_at_millis: 1_700_000_000_000,
            closed_at_millis: None,
        };
        let bytes = serde_json::to_vec(&view).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<InterventionView>(&bytes).expect("deserializes"),
            view
        );
    }

    #[test]
    fn an_empty_target_means_the_whole_table() {
        let request = RaiseInterventionRequest {
            ceremony_id: "c-1".to_owned(),
            intervention_id: "iv-1".to_owned(),
            role_id: "FACILITATOR".to_owned(),
            role_kind: "human".to_owned(),
            kind: "opinion".to_owned(),
            target_role_ids: Vec::new(),
            request: "Thoughts?".to_owned(),
        };
        assert!(
            request.target_role_ids.is_empty(),
            "no listed seat narrows the question; everyone at the table is asked"
        );
    }
}
