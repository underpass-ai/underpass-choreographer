use std::path::PathBuf;

use choreo_adapters::yaml::CeremonyDefinitionYaml;

const CATALOG: &[&str] = &[
    "daily-standup.yaml",
    "editorial-planning-meeting.yaml",
    "editorial-planning-meeting-vllm.yaml",
    "engineering-planning.yaml",
    "speaker-talk-qa.yaml",
    "sprint-planning.yaml",
    "technical-debate.yaml",
];

#[test]
fn shipped_catalog_ceremonies_are_free_of_validation_warnings() {
    let mut offenders = Vec::new();

    for file in CATALOG {
        let definition = CeremonyDefinitionYaml::parse_path(catalog_path(file))
            .unwrap_or_else(|error| panic!("{file} must parse: {error}"));
        let report = definition.analyze();

        assert!(
            report.is_valid(),
            "{file} produced blocking findings: {:?}",
            report.errors().collect::<Vec<_>>()
        );

        for warning in report.warnings() {
            offenders.push(format!("{file}: {warning:?}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "catalog ceremonies produced reachability warnings:\n{}",
        offenders.join("\n")
    );
}

fn catalog_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/e2e/ceremonies")
        .join(file)
}
