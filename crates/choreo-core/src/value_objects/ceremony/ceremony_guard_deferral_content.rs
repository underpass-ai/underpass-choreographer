//! Human-provided context for deferring a ceremony guard decision.

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_STATEMENT_LEN: usize = 4_096;
const MAX_REASON_LEN: usize = 4_096;
const MAX_RECONSIDERATION_CONDITION_LEN: usize = 1_024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyGuardDeferralContent {
    statement: String,
    reason: String,
    reconsider_when: Vec<String>,
}

impl CeremonyGuardDeferralContent {
    pub fn new(
        statement: impl Into<String>,
        reason: impl Into<String>,
        reconsider_when: Vec<String>,
    ) -> Result<Self, DomainError> {
        if reconsider_when.is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "ceremony_guard_deferral.reconsider_when",
            });
        }

        let statement = statement.into();
        let statement = validated_text(
            &statement,
            "ceremony_guard_deferral.statement",
            MAX_STATEMENT_LEN,
        )?;
        let reason = reason.into();
        let reason = validated_text(&reason, "ceremony_guard_deferral.reason", MAX_REASON_LEN)?;
        let reconsider_when = reconsider_when
            .into_iter()
            .map(|condition| {
                validated_text(
                    &condition,
                    "ceremony_guard_deferral.reconsider_when",
                    MAX_RECONSIDERATION_CONDITION_LEN,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            statement,
            reason,
            reconsider_when,
        })
    }

    #[must_use]
    pub fn statement(&self) -> &str {
        &self.statement
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub fn reconsider_when(&self) -> &[String] {
        &self.reconsider_when
    }
}

fn validated_text(value: &str, field: &'static str, max_len: usize) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::EmptyField { field });
    }
    if trimmed.len() > max_len {
        return Err(DomainError::FieldTooLong {
            field,
            actual: trimmed.len(),
            max: max_len,
        });
    }
    if trimmed
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(DomainError::InvalidCharacters { field });
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_the_human_statement_and_reconsideration_conditions() {
        let content = CeremonyGuardDeferralContent::new(
            "I do not know yet.",
            "The resolution is not clear.",
            vec!["New evidence explains the resolution.".to_owned()],
        )
        .unwrap();

        assert_eq!(content.statement(), "I do not know yet.");
        assert_eq!(content.reason(), "The resolution is not clear.");
        assert_eq!(
            content.reconsider_when(),
            ["New evidence explains the resolution."]
        );
    }

    #[test]
    fn requires_at_least_one_reconsideration_condition() {
        let error =
            CeremonyGuardDeferralContent::new("Not yet.", "Unclear.", Vec::new()).unwrap_err();

        assert!(matches!(error, DomainError::EmptyCollection { .. }));
    }
}
