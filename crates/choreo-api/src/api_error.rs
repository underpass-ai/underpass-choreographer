use serde::{Deserialize, Serialize};

/// How this contract fails.
///
/// Three shapes, because a consumer acts differently on each: waiting is a
/// remedy for `Unavailable`, asking for something else is the remedy for
/// `CeremonyNotFound`, and `Refused` means the engine looked at the request and
/// said no — retrying it unchanged will not change the answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ApiError {
    #[error("the ceremony engine is unavailable: {reason}")]
    Unavailable { reason: String },

    #[error("no ceremony named `{ceremony_id}`")]
    CeremonyNotFound { ceremony_id: String },

    #[error("the ceremony engine refused: {reason}")]
    Refused { reason: String },
}

impl ApiError {
    /// Whether trying again, unchanged, could plausibly succeed.
    ///
    /// Published on the error rather than left to the consumer, because a
    /// consumer keeping its own table of which errors are worth retrying goes
    /// stale the first time this enum grows.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Unavailable { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unavailability_invites_a_retry() {
        assert!(ApiError::Unavailable {
            reason: "starting".to_owned()
        }
        .is_transient());
        assert!(!ApiError::CeremonyNotFound {
            ceremony_id: "c-1".to_owned()
        }
        .is_transient());
        assert!(
            !ApiError::Refused {
                reason: "unbound definition".to_owned()
            }
            .is_transient(),
            "retrying a refusal unchanged asks the same question and earns the \
             same answer"
        );
    }
}
