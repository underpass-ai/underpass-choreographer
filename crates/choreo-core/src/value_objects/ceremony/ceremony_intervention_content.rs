use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::value_objects::Attributes;

const MAX_MESSAGE_LEN: usize = 16_384;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyInterventionContent {
    message: String,
    details: Attributes,
}

impl CeremonyInterventionContent {
    pub fn new(message: impl Into<String>, details: Attributes) -> Result<Self, DomainError> {
        let message = message.into();
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "ceremony_intervention.message",
            });
        }
        if trimmed.len() > MAX_MESSAGE_LEN {
            return Err(DomainError::FieldTooLong {
                field: "ceremony_intervention.message",
                actual: trimmed.len(),
                max: MAX_MESSAGE_LEN,
            });
        }
        if trimmed
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(DomainError::InvalidCharacters {
                field: "ceremony_intervention.message",
            });
        }
        Ok(Self {
            message: trimmed.to_owned(),
            details,
        })
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn details(&self) -> &Attributes {
        &self.details
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_multiline_participant_language() {
        let content = CeremonyInterventionContent::new(
            "Look at the queue.\nDo not consume messages.",
            Attributes::empty(),
        )
        .unwrap();

        assert!(content.message().contains('\n'));
    }

    #[test]
    fn rejects_blank_and_binary_content() {
        assert!(CeremonyInterventionContent::new(" ", Attributes::empty()).is_err());
        assert!(CeremonyInterventionContent::new("bad\0message", Attributes::empty()).is_err());
    }
}
