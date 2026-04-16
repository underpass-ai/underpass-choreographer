//! [`DomainError`] → [`tonic::Status`] mapping.
//!
//! The transport contract uses standard gRPC status codes. Each
//! domain-error variant has exactly one canonical mapping so clients
//! can dispatch reliably.

use choreo_core::error::DomainError;
use tonic::Status;

/// Map a [`DomainError`] onto a [`tonic::Status`] with the most
/// specific canonical code that matches the variant's semantics.
///
/// Every variant of `DomainError` is covered explicitly; adding a new
/// variant in core will fail the match exhaustiveness check here and
/// force the author to pick a code.
#[allow(clippy::needless_pass_by_value)] // idiomatic conversion: consume the error.
#[must_use]
pub fn domain_error_to_status(err: DomainError) -> Status {
    let msg = err.to_string();
    match err {
        DomainError::EmptyField { .. }
        | DomainError::FieldTooLong { .. }
        | DomainError::InvalidCharacters { .. }
        | DomainError::OutOfRange { .. }
        | DomainError::MustBeNonZero { .. }
        | DomainError::EmptyCollection { .. } => Status::invalid_argument(msg),
        DomainError::InvalidTransition { .. } | DomainError::InvariantViolated { .. } => {
            Status::failed_precondition(msg)
        }
        DomainError::NotFound { .. } => Status::not_found(msg),
        DomainError::AlreadyExists { .. } => Status::already_exists(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::Code;

    #[test]
    fn invalid_argument_for_validation_errors() {
        let cases = [
            DomainError::EmptyField { field: "x" },
            DomainError::FieldTooLong {
                field: "x",
                actual: 10,
                max: 5,
            },
            DomainError::InvalidCharacters { field: "x" },
            DomainError::OutOfRange {
                field: "x",
                value: 1.0,
                min: 0.0,
                max: 0.5,
            },
            DomainError::MustBeNonZero { field: "x" },
            DomainError::EmptyCollection { field: "x" },
        ];
        for err in cases {
            assert_eq!(domain_error_to_status(err).code(), Code::InvalidArgument);
        }
    }

    #[test]
    fn failed_precondition_for_state_errors() {
        assert_eq!(
            domain_error_to_status(DomainError::InvalidTransition { from: "a", to: "b" }).code(),
            Code::FailedPrecondition
        );
        assert_eq!(
            domain_error_to_status(DomainError::InvariantViolated { reason: "r" }).code(),
            Code::FailedPrecondition
        );
    }

    #[test]
    fn not_found_and_already_exists_are_distinct() {
        assert_eq!(
            domain_error_to_status(DomainError::NotFound { what: "x" }).code(),
            Code::NotFound
        );
        assert_eq!(
            domain_error_to_status(DomainError::AlreadyExists { what: "x" }).code(),
            Code::AlreadyExists
        );
    }

    #[test]
    fn message_is_preserved() {
        let err = DomainError::NotFound { what: "council" };
        let status = domain_error_to_status(err.clone());
        assert!(status.message().contains("council"));
    }
}
