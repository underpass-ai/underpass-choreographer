use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CeremonyInterventionKind {
    Opinion,
    Investigation,
    Action,
}

impl CeremonyInterventionKind {
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Opinion => "opinion",
            Self::Investigation => "investigation",
            Self::Action => "action",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_stable() {
        assert_eq!(CeremonyInterventionKind::Opinion.as_label(), "opinion");
        assert_eq!(
            CeremonyInterventionKind::Investigation.as_label(),
            "investigation"
        );
        assert_eq!(CeremonyInterventionKind::Action.as_label(), "action");
    }
}
