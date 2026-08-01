//! What kind of party the caller says acted.
//!
//! One parser for every verb that asks. Two copies of a four-arm match
//! is how one edge quietly starts accepting a spelling the other
//! refuses, and the whole value of the field is that every edge treats
//! it the same way.

use choreo_core::error::DomainError;
use choreo_core::value_objects::AuditActorKind;

/// Refused rather than defaulted when it is missing or unknown.
///
/// A default would put a kind in the record that nobody chose, and the
/// whole reason the field exists is that the engine must not choose
/// one.
pub fn actor_kind_from_proto(
    raw: &str,
    field: &'static str,
) -> Result<AuditActorKind, DomainError> {
    Ok(match raw {
        "human" => AuditActorKind::Human,
        "agent" => AuditActorKind::Agent,
        "service" => AuditActorKind::Service,
        "engine" => AuditActorKind::Engine,
        _ => return Err(DomainError::InvalidCharacters { field }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_the_domain_has_can_be_spelled() {
        for kind in [
            AuditActorKind::Human,
            AuditActorKind::Agent,
            AuditActorKind::Service,
            AuditActorKind::Engine,
        ] {
            assert_eq!(
                actor_kind_from_proto(kind.as_str(), "role_kind").unwrap(),
                kind,
                "the wire spelling of {kind:?} does not parse back"
            );
        }
    }

    /// The empty string is the one that matters.
    ///
    /// A caller who sends nothing gets proto's default, so "missing"
    /// and "empty" arrive here identically. Accepting it would make the
    /// field optional in practice while looking required in the schema.
    #[test]
    fn an_undeclared_kind_is_refused_rather_than_defaulted() {
        for raw in ["", "  ", "Human", "person", "robot"] {
            assert!(
                actor_kind_from_proto(raw, "role_kind").is_err(),
                "{raw:?} was accepted as a kind"
            );
        }
    }
}
