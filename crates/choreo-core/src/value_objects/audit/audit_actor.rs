use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::value_objects::RoleId;

use super::AuditActorKind;

const MAX_ACTOR_ID_LEN: usize = 256;

/// Who caused an audited fact.
///
/// The role is optional because not every actor acts through one — the
/// engine's own timeouts do not — but when a role exists it is part of
/// the attribution, not decoration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditActor {
    actor_id: String,
    kind: AuditActorKind,
    #[serde(default)]
    role_id: Option<RoleId>,
}

impl AuditActor {
    pub fn new(
        actor_id: impl Into<String>,
        kind: AuditActorKind,
        role_id: Option<RoleId>,
    ) -> Result<Self, DomainError> {
        let actor_id = actor_id.into();
        let trimmed = actor_id.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "audit_actor.actor_id",
            });
        }
        if trimmed.len() > MAX_ACTOR_ID_LEN {
            return Err(DomainError::FieldTooLong {
                field: "audit_actor.actor_id",
                actual: trimmed.len(),
                max: MAX_ACTOR_ID_LEN,
            });
        }
        Ok(Self {
            actor_id: trimmed.to_owned(),
            kind,
            role_id,
        })
    }

    /// The engine acting on its own behalf.
    pub fn engine(actor_id: impl Into<String>) -> Result<Self, DomainError> {
        Self::new(actor_id, AuditActorKind::Engine, None)
    }

    #[must_use]
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    #[must_use]
    pub fn kind(&self) -> AuditActorKind {
        self.kind
    }

    #[must_use]
    pub fn role_id(&self) -> Option<&RoleId> {
        self.role_id.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_actor_id_is_rejected() {
        assert!(matches!(
            AuditActor::new("   ", AuditActorKind::Agent, None),
            Err(DomainError::EmptyField {
                field: "audit_actor.actor_id"
            })
        ));
    }

    #[test]
    fn an_overlong_actor_id_is_rejected() {
        assert!(matches!(
            AuditActor::new(
                "a".repeat(MAX_ACTOR_ID_LEN + 1),
                AuditActorKind::Agent,
                None
            ),
            Err(DomainError::FieldTooLong { .. })
        ));
    }

    #[test]
    fn the_engine_actor_carries_no_role() {
        let actor = AuditActor::engine("choreo").unwrap();

        assert_eq!(actor.kind(), AuditActorKind::Engine);
        assert!(actor.role_id().is_none());
    }
}
