//! Optional startup seeding of demo councils and agents.
//!
//! Controlled by `CHOREO_SEED_SPECIALTIES`: a comma-separated list
//! of specialty labels. For each label, the binary registers a
//! [`NoopAgent`] and a single-agent [`Council`] so the service is
//! immediately exercisable over gRPC / NATS.
//!
//! Seeding is **off by default**. The presence of the variable is
//! an opt-in operator choice. Real deployments wire agents through
//! adapter-specific channels (future slices: RegisterAgent RPC,
//! config-driven factories).

use choreo_adapters::noop::NoopAgent;
use choreo_core::entities::Council;
use choreo_core::error::DomainError;
use choreo_core::ports::{AgentRegistryPort, ClockPort, CouncilRegistryPort};
use choreo_core::value_objects::{AgentId, CouncilId, Specialty};
use std::sync::Arc;
use thiserror::Error;
use tracing::info;

/// Environment variable used to opt in to startup seeding.
pub const SEED_ENV_VAR: &str = "CHOREO_SEED_SPECIALTIES";

#[derive(Debug, Error)]
pub enum SeedingError {
    #[error("seeding failed: {0}")]
    Domain(#[from] DomainError),
}

/// Read the opt-in env var and apply seeding if present.
pub async fn apply_env_seeding(
    clock: &dyn ClockPort,
    agent_registry: &dyn AgentRegistryPort,
    council_registry: &dyn CouncilRegistryPort,
) -> Result<(), SeedingError> {
    let Ok(raw) = std::env::var(SEED_ENV_VAR) else {
        return Ok(());
    };
    let labels: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if labels.is_empty() {
        return Ok(());
    }
    info!(specialties = ?labels, "seeding demo councils");
    apply_seeding(clock, agent_registry, council_registry, &labels).await
}

/// Seed one `NoopAgent` and one single-agent council per specialty.
///
/// Split from [`apply_env_seeding`] so tests can drive seeding
/// without touching process env.
pub async fn apply_seeding(
    clock: &dyn ClockPort,
    agent_registry: &dyn AgentRegistryPort,
    council_registry: &dyn CouncilRegistryPort,
    specialties: &[&str],
) -> Result<(), SeedingError> {
    for label in specialties {
        let specialty = Specialty::new(*label)?;
        let agent_id = AgentId::new(format!("seed-{label}-0"))?;
        let agent = Arc::new(NoopAgent::new(agent_id.clone(), specialty.clone()));
        // If the agent is already registered (re-seeding) swallow the
        // AlreadyExists error so restarts stay idempotent.
        match agent_registry.register(agent).await {
            Ok(()) | Err(DomainError::AlreadyExists { .. }) => {}
            Err(err) => return Err(err.into()),
        }
        let council_id = CouncilId::new(format!("seed-{label}"))?;
        let council = Council::new(council_id, specialty.clone(), vec![agent_id], clock.now())?;
        match council_registry.register(council).await {
            Ok(()) | Err(DomainError::AlreadyExists { .. }) => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_adapters::clock::SystemClock;
    use choreo_adapters::memory::{InMemoryAgentRegistry, InMemoryCouncilRegistry};
    use choreo_core::ports::{AgentResolverPort, CouncilRegistryPort};

    #[tokio::test]
    async fn seeding_is_idempotent() {
        let clock = SystemClock::new();
        let agents = InMemoryAgentRegistry::new();
        let councils = InMemoryCouncilRegistry::new();

        apply_seeding(&clock, &agents, &councils, &["triage", "reviewer"])
            .await
            .unwrap();
        assert_eq!(councils.len().await, 2);
        assert_eq!(agents.len().await, 2);

        // Second pass must not fail and must not duplicate.
        apply_seeding(&clock, &agents, &councils, &["triage", "reviewer"])
            .await
            .unwrap();
        assert_eq!(councils.len().await, 2);
        assert_eq!(agents.len().await, 2);
    }

    #[tokio::test]
    async fn seeded_council_is_resolvable_and_matches_specialty() {
        let clock = SystemClock::new();
        let agents = InMemoryAgentRegistry::new();
        let councils = InMemoryCouncilRegistry::new();
        apply_seeding(&clock, &agents, &councils, &["triage"])
            .await
            .unwrap();

        let triage = Specialty::new("triage").unwrap();
        let council = councils.get(&triage).await.unwrap();
        assert_eq!(council.specialty(), &triage);
        assert_eq!(council.size(), 1);

        // The agent referenced by the council resolves.
        let id = council.agents().iter().next().unwrap();
        let resolved = agents.resolve(id).await.unwrap();
        assert_eq!(resolved.specialty(), &triage);
    }

    #[tokio::test]
    async fn invalid_specialty_surfaces_as_domain_error() {
        let clock = SystemClock::new();
        let agents = InMemoryAgentRegistry::new();
        let councils = InMemoryCouncilRegistry::new();
        let err = apply_seeding(&clock, &agents, &councils, &["   "])
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SeedingError::Domain(DomainError::EmptyField { .. })
        ));
    }

    #[tokio::test]
    async fn empty_label_list_is_a_noop() {
        let clock = SystemClock::new();
        let agents = InMemoryAgentRegistry::new();
        let councils = InMemoryCouncilRegistry::new();
        apply_seeding(&clock, &agents, &councils, &[])
            .await
            .unwrap();
        assert!(agents.is_empty().await);
        assert!(councils.is_empty().await);
    }
}
