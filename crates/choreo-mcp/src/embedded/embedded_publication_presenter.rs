use choreo_core::entities::PublicationOutcome;
use serde_json::{json, Value};

/// Render what publishing did.
pub(crate) struct EmbeddedPublicationPresenter;

impl EmbeddedPublicationPresenter {
    pub(crate) fn present(outcome: &PublicationOutcome) -> Value {
        match outcome {
            PublicationOutcome::Published(published) => json!({
                "outcome": "published",
                "ceremony": published.name().as_str(),
                "version": published.version().as_str(),
                "digest": published.digest().to_hex(),
            }),
            PublicationOutcome::AlreadyPublished(published) => json!({
                "outcome": "already_published",
                "ceremony": published.name().as_str(),
                "version": published.version().as_str(),
                "digest": published.digest().to_hex(),
            }),
            // Both digests are reported: the caller has to see that what
            // it offered is not what is there, or "occupied" reads as a
            // transient failure worth retrying.
            PublicationOutcome::VersionOccupied { published, offered } => json!({
                "outcome": "version_occupied",
                "published_digest": published.to_hex(),
                "offered_digest": offered.to_hex(),
            }),
        }
    }
}
