use async_trait::async_trait;
use choreo_adapters::yaml::CeremonyDefinitionYaml;
use choreo_core::entities::CeremonyDefinition;
use choreo_core::error::DomainError;
use choreo_core::ports::CeremonyDefinitionSourcePort;

/// Supplies caller-owned ceremony definitions to the application layer.
#[derive(Debug, Clone)]
pub struct InProcessCeremonyDefinitionSource {
    definitions: Vec<CeremonyDefinition>,
}

impl InProcessCeremonyDefinitionSource {
    #[must_use]
    pub fn new(definitions: impl IntoIterator<Item = CeremonyDefinition>) -> Self {
        Self {
            definitions: definitions.into_iter().collect(),
        }
    }

    pub fn from_yaml(raw: &str) -> Result<Self, DomainError> {
        CeremonyDefinitionYaml::parse_str(raw).map(|definition| Self::new([definition]))
    }
}

#[async_trait]
impl CeremonyDefinitionSourcePort for InProcessCeremonyDefinitionSource {
    async fn load(&self) -> Result<Vec<CeremonyDefinition>, DomainError> {
        Ok(self.definitions.clone())
    }
}
