use serde::{Deserialize, Serialize};

/// What kind of participant caused an audited fact.
///
/// The distinction is not cosmetic: an approval attributed to a human
/// and one attributed to an agent carry different authority, and an
/// audit that cannot tell them apart cannot support either claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActorKind {
    Human,
    Agent,
    Service,
    /// The engine acting on its own behalf — lease expiry, timeout,
    /// automatic transition. Never a substitute for a human decision.
    Engine,
}

impl AuditActorKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::Service => "service",
            Self::Engine => "engine",
        }
    }

    #[must_use]
    pub fn is_human(self) -> bool {
        self == Self::Human
    }
}
