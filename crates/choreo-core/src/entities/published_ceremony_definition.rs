//! [`PublishedCeremonyDefinition`] — a definition fixed to a content
//! identity.
//!
//! An agent can already write a definition and run it. What publication
//! adds is that the thing it ran can be named later and shown to be the
//! same thing: an immutable version with a digest an instance binds to
//! and an auditor recomputes.
//!
//! Running an ad-hoc definition stays possible and is not the same act.
//! Investigation should not need a published version; governed reuse
//! should not accept an unpublished one.

use crate::error::DomainError;
use crate::value_objects::{CeremonyDefinitionDigest, CeremonyName, CeremonyVersion};

use super::CeremonyDefinition;

/// A definition and the digest that identifies its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedCeremonyDefinition {
    definition: CeremonyDefinition,
    digest: CeremonyDefinitionDigest,
}

impl PublishedCeremonyDefinition {
    /// Fix a definition to its content identity.
    ///
    /// Only a definition can be sealed, and a `CeremonyDefinition`
    /// cannot exist while invalid — so an unpublishable draft can never
    /// reach this constructor.
    pub fn seal(definition: CeremonyDefinition) -> Result<Self, DomainError> {
        let digest = definition.digest()?;
        Ok(Self { definition, digest })
    }

    #[must_use]
    pub fn definition(&self) -> &CeremonyDefinition {
        &self.definition
    }

    #[must_use]
    pub fn digest(&self) -> CeremonyDefinitionDigest {
        self.digest
    }

    #[must_use]
    pub fn name(&self) -> &CeremonyName {
        self.definition.name()
    }

    #[must_use]
    pub fn version(&self) -> &CeremonyVersion {
        self.definition.version()
    }

    #[must_use]
    pub fn into_definition(self) -> CeremonyDefinition {
        self.definition
    }
}

/// What publishing did.
///
/// Publishing the same content twice is not a failure — a retried
/// publication must be safe, or a caller that loses a response has no
/// correct next move. Publishing *different* content under a version
/// that is taken is a conflict, and like a revision conflict it is an
/// outcome the caller must resolve rather than an error to swallow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicationOutcome {
    Published(PublishedCeremonyDefinition),
    /// The identical definition was already published under this
    /// version. Nothing changed, and nothing needed to.
    AlreadyPublished(PublishedCeremonyDefinition),
    /// The version is taken by different content. A published version
    /// is immutable, so the answer is a new version, never an
    /// overwrite.
    VersionOccupied {
        published: CeremonyDefinitionDigest,
        offered: CeremonyDefinitionDigest,
    },
}

impl PublicationOutcome {
    #[must_use]
    pub fn published(&self) -> Option<&PublishedCeremonyDefinition> {
        match self {
            Self::Published(published) | Self::AlreadyPublished(published) => Some(published),
            Self::VersionOccupied { .. } => None,
        }
    }

    #[must_use]
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::VersionOccupied { .. })
    }

    /// Whether this call is what put the definition there.
    #[must_use]
    pub fn is_new(&self) -> bool {
        matches!(self, Self::Published(_))
    }
}
