//! Dynamic intervention owned by a running ceremony instance.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::DomainError;
use crate::value_objects::{
    CeremonyInterventionContent, CeremonyInterventionId, CeremonyInterventionKind,
    CeremonyInterventionProvenance, CeremonyInterventionResponse, CeremonyInterventionStatus,
    CeremonyInterventionTarget, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyIntervention {
    id: CeremonyInterventionId,
    kind: CeremonyInterventionKind,
    requested_by: RoleId,
    target: CeremonyInterventionTarget,
    request: CeremonyInterventionContent,
    #[serde(default)]
    provenance: Option<CeremonyInterventionProvenance>,
    responses: Vec<CeremonyInterventionResponse>,
    status: CeremonyInterventionStatus,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    closed_at: Option<OffsetDateTime>,
}

impl CeremonyIntervention {
    #[must_use]
    pub fn open(
        id: CeremonyInterventionId,
        kind: CeremonyInterventionKind,
        requested_by: RoleId,
        target: CeremonyInterventionTarget,
        request: CeremonyInterventionContent,
        now: OffsetDateTime,
    ) -> Self {
        Self::open_with_provenance(id, kind, requested_by, target, request, None, now)
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn open_with_provenance(
        id: CeremonyInterventionId,
        kind: CeremonyInterventionKind,
        requested_by: RoleId,
        target: CeremonyInterventionTarget,
        request: CeremonyInterventionContent,
        provenance: Option<CeremonyInterventionProvenance>,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id,
            kind,
            requested_by,
            target,
            request,
            provenance,
            responses: Vec::new(),
            status: CeremonyInterventionStatus::Open,
            created_at: now,
            updated_at: now,
            closed_at: None,
        }
    }

    pub fn respond(
        &mut self,
        role_id: RoleId,
        content: CeremonyInterventionContent,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        if !self.status.is_open() {
            return Err(DomainError::InvariantViolated {
                reason: "closed ceremony interventions cannot receive responses",
            });
        }
        if !self.target.accepts(&role_id) {
            return Err(DomainError::InvariantViolated {
                reason: "ceremony intervention does not target responding role",
            });
        }
        if self
            .responses
            .iter()
            .any(|response| response.role_id() == &role_id)
        {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_intervention.response_role",
            });
        }
        self.responses
            .push(CeremonyInterventionResponse::new(role_id, content, now));
        self.updated_at = now;
        Ok(())
    }

    pub fn close(&mut self, role_id: &RoleId, now: OffsetDateTime) -> Result<(), DomainError> {
        if !self.status.is_open() {
            return Err(DomainError::InvariantViolated {
                reason: "ceremony intervention is already closed",
            });
        }
        if role_id != &self.requested_by {
            return Err(DomainError::InvariantViolated {
                reason: "only the requesting role can close a ceremony intervention",
            });
        }
        self.status = CeremonyInterventionStatus::Closed;
        self.updated_at = now;
        self.closed_at = Some(now);
        Ok(())
    }

    #[must_use]
    pub fn id(&self) -> &CeremonyInterventionId {
        &self.id
    }

    #[must_use]
    pub const fn kind(&self) -> CeremonyInterventionKind {
        self.kind
    }

    #[must_use]
    pub fn requested_by(&self) -> &RoleId {
        &self.requested_by
    }

    #[must_use]
    pub fn target(&self) -> &CeremonyInterventionTarget {
        &self.target
    }

    #[must_use]
    pub fn request(&self) -> &CeremonyInterventionContent {
        &self.request
    }

    #[must_use]
    pub fn provenance(&self) -> Option<&CeremonyInterventionProvenance> {
        self.provenance.as_ref()
    }

    #[must_use]
    pub fn responses(&self) -> &[CeremonyInterventionResponse] {
        &self.responses
    }

    #[must_use]
    pub const fn status(&self) -> CeremonyInterventionStatus {
        self.status
    }

    #[must_use]
    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    #[must_use]
    pub fn updated_at(&self) -> OffsetDateTime {
        self.updated_at
    }

    #[must_use]
    pub fn closed_at(&self) -> Option<OffsetDateTime> {
        self.closed_at
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use crate::value_objects::Attributes;

    use super::*;

    fn content(message: &str) -> CeremonyInterventionContent {
        CeremonyInterventionContent::new(message, Attributes::empty()).unwrap()
    }

    #[test]
    fn accepts_one_response_per_target_role_and_requester_controls_close() {
        let now = datetime!(2026-07-20 12:00:00 UTC);
        let engineer = RoleId::new("ENGINEER").unwrap();
        let observer = RoleId::new("OBSERVER").unwrap();
        let mut intervention = CeremonyIntervention::open(
            CeremonyInterventionId::new("intervention-1").unwrap(),
            CeremonyInterventionKind::Investigation,
            engineer.clone(),
            CeremonyInterventionTarget::roles([observer.clone()]).unwrap(),
            content("Inspect the queue without consuming messages."),
            now,
        );

        intervention
            .respond(observer.clone(), content("Depth is stable."), now)
            .unwrap();

        assert!(intervention
            .respond(observer.clone(), content("Duplicate."), now)
            .is_err());
        assert!(intervention.close(&observer, now).is_err());
        intervention.close(&engineer, now).unwrap();
        assert_eq!(intervention.status(), CeremonyInterventionStatus::Closed);
    }
}
