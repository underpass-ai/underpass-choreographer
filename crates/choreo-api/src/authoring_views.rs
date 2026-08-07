use serde::{Deserialize, Serialize};

/// One defect found in a definition draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionDefectView {
    /// `error` or `warning`.
    pub severity: String,
    /// Where, in the author's terms: "state `X`", "guard `Y`", …
    pub locus: String,
    /// What is wrong, in a sentence.
    pub defect: String,
    /// Whether this alone prevents publication.
    pub blocking: bool,
}

/// What analysis found — all of it.
///
/// Every defect at once, never the first one (ADR-002 upstream): fixing
/// defects one at a time spends the author's attention on round trips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionAnalysisView {
    /// Identity declared by the parsed draft.
    pub definition_name: String,
    pub definition_version: String,
    /// Whether the draft, as analyzed, could be published.
    pub publishable: bool,
    /// Canonical hex digest the executable definition will publish with.
    ///
    /// Present exactly when the draft is publishable. This is the same
    /// identity [`PublishedDefinitionView::digest`] returns and ceremony
    /// instances bind to; it is not a hash of the source bytes.
    pub definition_digest: Option<String>,
    pub defects: Vec<DefinitionDefectView>,
}

/// A definition that is now published, or already was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedDefinitionView {
    pub name: String,
    pub version: String,
    /// Hex digest of the published content — what an instance binds to, and
    /// what makes "this exact procedure ran" provable.
    pub digest: String,
    /// True when the identical content was already published under this name
    /// and version. Nothing changed, and nothing needed to — which is what
    /// makes a retried publish safe.
    pub already_published: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_analysis_survives_the_wire() {
        let analysis = DefinitionAnalysisView {
            definition_name: "scope_discovery".to_owned(),
            definition_version: "1.0".to_owned(),
            publishable: false,
            definition_digest: None,
            defects: vec![DefinitionDefectView {
                severity: "error".to_owned(),
                locus: "state `ORPHAN`".to_owned(),
                defect: "state is unreachable".to_owned(),
                blocking: true,
            }],
        };
        let bytes = serde_json::to_vec(&analysis).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<DefinitionAnalysisView>(&bytes).expect("deserializes"),
            analysis
        );
    }

    #[test]
    fn a_publication_names_what_an_instance_will_bind_to() {
        let published = PublishedDefinitionView {
            name: "scope_discovery".to_owned(),
            version: "1.0".to_owned(),
            digest: "abc123".to_owned(),
            already_published: false,
        };
        assert!(
            !published.digest.is_empty(),
            "a publication without a digest cannot be bound to, only believed"
        );
    }
}
