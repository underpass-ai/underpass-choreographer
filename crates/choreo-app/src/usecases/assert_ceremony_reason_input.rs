use choreo_core::value_objects::{
    CeremonyId, CeremonyReasonKind, CeremonyRecordRef, MemoryConfidence, RoleId,
};

/// Saying why one thing a session produced led to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertCeremonyReasonInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) role_id: RoleId,
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
        from: CeremonyRecordRef,
        to: CeremonyRecordRef,
        kind: CeremonyReasonKind,
        why: impl Into<String>,
        confidence: MemoryConfidence,
    ) -> Self {
        Self {
            instance_id,
            role_id,
            from,
            to,
            kind,
            why: why.into(),
            confidence,
        }
    }
}
