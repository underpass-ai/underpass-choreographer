use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Start a ceremony instance from a published definition.
///
/// Published only, by design. A consumer reaches authoring — drafts, analysis,
/// publication — through the engine's own surfaces, where every defect is
/// reported and every version is immutable. What a consumer may *start* is
/// what was published, which is why every instance started through this
/// contract carries a definition digest: "this exact procedure ran" is
/// provable for all of them, with no draft-shaped exception to remember.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartCeremonyRequest {
    /// The identity the new instance will answer to.
    pub ceremony_id: String,
    pub definition_name: String,
    pub definition_version: String,
    /// The consumer's own keys, carried opaquely. This is where a consuming
    /// product ties the instance to its own aggregate; the engine does not
    /// know what the keys mean and is not asked to.
    pub context: BTreeMap<String, serde_json::Value>,
    /// Who is opening the session, in the caller's own terms. Not a role from
    /// the definition: whoever opens a session may be a participant, an
    /// operator, or a scheduler that never takes part.
    pub actor_id: String,
    /// One of `human`, `agent`, `service`, `engine`. Carried, never worked
    /// out; anything else is refused rather than guessed at.
    pub actor_kind: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_survives_the_wire() {
        let request = StartCeremonyRequest {
            ceremony_id: "c-1".to_owned(),
            definition_name: "scope_discovery".to_owned(),
            definition_version: "1.0".to_owned(),
            context: BTreeMap::from([(
                "requested_by".to_owned(),
                serde_json::Value::String("consumer-1".to_owned()),
            )]),
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
        };
        let bytes = serde_json::to_vec(&request).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<StartCeremonyRequest>(&bytes).expect("deserializes"),
            request
        );
    }
}
