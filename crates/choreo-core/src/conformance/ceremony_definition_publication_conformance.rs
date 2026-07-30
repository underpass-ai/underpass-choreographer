//! Conformance suite for [`CeremonyDefinitionPublicationPort`].
//!
//! The property under test is immutability, and it is easy to fail by
//! accident: a store built on the ordinary repository's `save` accepts
//! everything and overwrites silently, which looks like it works until
//! an instance is bound to a version whose content has since changed
//! underneath it.

use crate::entities::{CeremonyDefinition, PublishedCeremonyDefinition};
use crate::error::DomainError;
use crate::ports::CeremonyDefinitionPublicationPort;
use crate::value_objects::{
    CeremonyName, CeremonyState, CeremonyTransition, CeremonyVersion, StateId, TransitionTrigger,
};

use super::ConformanceFailure;

/// Every property a [`CeremonyDefinitionPublicationPort`]
/// implementation must satisfy.
#[derive(Debug)]
pub struct CeremonyDefinitionPublicationConformance;

impl CeremonyDefinitionPublicationConformance {
    pub async fn run(
        publications: &dyn CeremonyDefinitionPublicationPort,
    ) -> Result<Vec<&'static str>, ConformanceFailure> {
        let mut passed = Vec::new();
        Self::an_unpublished_version_is_absent(publications).await?;
        passed.push("an_unpublished_version_is_absent");
        Self::publishing_stores_the_definition_and_its_digest(publications).await?;
        passed.push("publishing_stores_the_definition_and_its_digest");
        Self::republishing_identical_content_is_idempotent(publications).await?;
        passed.push("republishing_identical_content_is_idempotent");
        Self::a_taken_version_is_never_overwritten(publications).await?;
        passed.push("a_taken_version_is_never_overwritten");
        Self::versions_of_one_ceremony_coexist(publications).await?;
        passed.push("versions_of_one_ceremony_coexist");
        Ok(passed)
    }

    async fn an_unpublished_version_is_absent(
        publications: &dyn CeremonyDefinitionPublicationPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "an_unpublished_version_is_absent";
        let name = ceremony_name(PROPERTY, "absent")?;

        let found = call(
            PROPERTY,
            publications.published(&name, &CeremonyVersion::v1()).await,
        )?;
        if found.is_some() {
            return Err(failure(
                PROPERTY,
                "a version that was never published came back",
            ));
        }
        Ok(())
    }

    async fn publishing_stores_the_definition_and_its_digest(
        publications: &dyn CeremonyDefinitionPublicationPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "publishing_stores_the_definition_and_its_digest";
        let name = ceremony_name(PROPERTY, "stored")?;
        let sealed = sealed(PROPERTY, &name, "done")?;
        let digest = sealed.digest();

        let outcome = call(PROPERTY, publications.publish(sealed).await)?;
        if !outcome.is_new() {
            return Err(failure(
                PROPERTY,
                "a first publication was not reported as new",
            ));
        }

        let stored = call(
            PROPERTY,
            publications.published(&name, &CeremonyVersion::v1()).await,
        )?
        .ok_or_else(|| failure(PROPERTY, "a published version is not readable"))?;
        if stored.digest() != digest {
            return Err(failure(
                PROPERTY,
                "the stored digest is not the one that was published",
            ));
        }
        Ok(())
    }

    /// A caller that loses the response must be able to retry. If
    /// republishing identical content failed, it would have no correct
    /// next move.
    async fn republishing_identical_content_is_idempotent(
        publications: &dyn CeremonyDefinitionPublicationPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "republishing_identical_content_is_idempotent";
        let name = ceremony_name(PROPERTY, "retried")?;

        call(
            PROPERTY,
            publications.publish(sealed(PROPERTY, &name, "done")?).await,
        )?;
        let again = call(
            PROPERTY,
            publications.publish(sealed(PROPERTY, &name, "done")?).await,
        )?;

        if again.is_conflict() {
            return Err(failure(
                PROPERTY,
                "republishing identical content was reported as a conflict",
            ));
        }
        if again.is_new() {
            return Err(failure(
                PROPERTY,
                "republishing identical content was reported as a first publication",
            ));
        }
        Ok(())
    }

    /// The property publication exists for.
    async fn a_taken_version_is_never_overwritten(
        publications: &dyn CeremonyDefinitionPublicationPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "a_taken_version_is_never_overwritten";
        let name = ceremony_name(PROPERTY, "immutable")?;
        let original = sealed(PROPERTY, &name, "done")?;
        let original_digest = original.digest();

        call(PROPERTY, publications.publish(original).await)?;
        let outcome = call(
            PROPERTY,
            publications
                .publish(sealed(PROPERTY, &name, "finished")?)
                .await,
        )?;

        if !outcome.is_conflict() {
            return Err(failure(
                PROPERTY,
                "different content was accepted under a version that was already taken",
            ));
        }

        let stored = call(
            PROPERTY,
            publications.published(&name, &CeremonyVersion::v1()).await,
        )?
        .ok_or_else(|| failure(PROPERTY, "the published version disappeared"))?;
        if stored.digest() != original_digest {
            return Err(failure(
                PROPERTY,
                "a rejected publication still replaced the stored definition",
            ));
        }
        Ok(())
    }

    async fn versions_of_one_ceremony_coexist(
        publications: &dyn CeremonyDefinitionPublicationPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "versions_of_one_ceremony_coexist";
        let name = ceremony_name(PROPERTY, "versioned")?;
        let second = CeremonyVersion::new("2.0")
            .map_err(|error| failure(PROPERTY, format!("invalid version: {error}")))?;

        call(
            PROPERTY,
            publications.publish(sealed(PROPERTY, &name, "done")?).await,
        )?;
        let outcome = call(
            PROPERTY,
            publications
                .publish(sealed_at(PROPERTY, &name, second.clone(), "finished")?)
                .await,
        )?;
        if !outcome.is_new() {
            return Err(failure(
                PROPERTY,
                "a new version of an existing ceremony was refused",
            ));
        }

        let first = call(
            PROPERTY,
            publications.published(&name, &CeremonyVersion::v1()).await,
        )?;
        if first.is_none() {
            return Err(failure(
                PROPERTY,
                "publishing a new version removed the previous one",
            ));
        }
        Ok(())
    }
}

fn call<T>(
    property: &'static str,
    outcome: Result<T, DomainError>,
) -> Result<T, ConformanceFailure> {
    outcome.map_err(|error| failure(property, format!("the adapter returned an error: {error}")))
}

fn failure(property: &'static str, detail: impl Into<String>) -> ConformanceFailure {
    ConformanceFailure::new(property, detail)
}

fn ceremony_name(property: &'static str, suffix: &str) -> Result<CeremonyName, ConformanceFailure> {
    CeremonyName::new(format!("conformance_{property}_{suffix}"))
        .map_err(|error| failure(property, format!("invalid ceremony name: {error}")))
}

fn sealed(
    property: &'static str,
    name: &CeremonyName,
    terminal: &str,
) -> Result<PublishedCeremonyDefinition, ConformanceFailure> {
    sealed_at(property, name, CeremonyVersion::v1(), terminal)
}

/// `terminal` names the terminal state, which is what makes two
/// otherwise identical definitions differ materially.
fn sealed_at(
    property: &'static str,
    name: &CeremonyName,
    version: CeremonyVersion,
    terminal: &str,
) -> Result<PublishedCeremonyDefinition, ConformanceFailure> {
    let build = || -> Result<PublishedCeremonyDefinition, DomainError> {
        let definition = CeremonyDefinition::new(
            name.clone(),
            version,
            None,
            Vec::new(),
            Vec::new(),
            vec![
                CeremonyState::initial(StateId::new("OPEN")?),
                CeremonyState::terminal(StateId::new(terminal)?),
            ],
            vec![CeremonyTransition::new(
                StateId::new("OPEN")?,
                StateId::new(terminal)?,
                TransitionTrigger::new("finish")?,
                Vec::new(),
            )?],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )?;
        PublishedCeremonyDefinition::seal(definition)
    };
    build().map_err(|error| {
        failure(
            property,
            format!("the suite built an invalid definition: {error}"),
        )
    })
}
