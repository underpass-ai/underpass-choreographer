use choreo_core::value_objects::{
    AuditActorKind, CeremonyId, CeremonyReasonKind, CeremonyRecordRef, MemoryConfidence, RoleId,
};

/// Saying why one thing a session produced led to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertCeremonyReasonInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) role_id: RoleId,
    /// What kind of party fills that seat.
    ///
    /// Carried, never worked out. The engine sees a seat and cannot
    /// see what fills it.
    pub(crate) role_kind: AuditActorKind,
    pub(crate) from: CeremonyRecordRef,
    pub(crate) to: CeremonyRecordRef,
    pub(crate) kind: CeremonyReasonKind,
    pub(crate) why: String,
    pub(crate) confidence: MemoryConfidence,
}

impl AssertCeremonyReasonInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        role_id: RoleId,
        role_kind: AuditActorKind,
        from: CeremonyRecordRef,
        to: CeremonyRecordRef,
        kind: CeremonyReasonKind,
        why: impl Into<String>,
        confidence: MemoryConfidence,
    ) -> Self {
        Self {
            instance_id,
            role_id,
            role_kind,
            from,
            to,
            kind,
            why: why.into(),
            confidence,
        }
    }
}
