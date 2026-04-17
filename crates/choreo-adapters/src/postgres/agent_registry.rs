//! Postgres implementation of [`AgentRegistryPort`] + [`AgentResolverPort`].
//!
//! Rows persist the typed [`AgentDescriptor`] (id, specialty, kind,
//! attributes). On read, descriptors are rehydrated into live
//! `Arc<dyn AgentPort>` handles via the wired [`AgentFactoryPort`];
//! no pickled provider state crosses the database boundary. That keeps
//! the registry content portable across replicas — each replica can
//! materialize its own live handles using whatever factory it has
//! feature-enabled.
//!
//! Adapter-specific limitation in this slice: `register(agent)` asks
//! the passed-in agent for its id / specialty, but then we need a
//! descriptor to persist — we only store kind="noop" today because
//! the composition root only wires the NoopAgentFactory. When the
//! dispatching factory lands (with vLLM / Anthropic / OpenAI kinds),
//! we will switch register to take an [`AgentDescriptor`] directly so
//! the persisted kind is an honest reflection of the wiring.

use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    AgentDescriptor, AgentFactoryPort, AgentPort, AgentRegistryPort, AgentResolverPort,
};
use choreo_core::value_objects::{AgentId, AgentKind, Attributes, Specialty};
use serde_json::Value as JsonValue;
use sqlx::Row;

use super::error::{serde_to_domain, sqlx_to_domain};
use super::pool::PostgresPool;

#[derive(Clone)]
pub struct PostgresAgentRegistry {
    pool: PostgresPool,
    factory: Arc<dyn AgentFactoryPort>,
}

impl std::fmt::Debug for PostgresAgentRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresAgentRegistry").finish()
    }
}

impl PostgresAgentRegistry {
    #[must_use]
    pub fn new(pool: PostgresPool, factory: Arc<dyn AgentFactoryPort>) -> Self {
        Self { pool, factory }
    }

    /// Persist a full [`AgentDescriptor`]. Used by wiring / registration
    /// paths that already have a descriptor in hand; the
    /// [`AgentRegistryPort::register`] surface degrades to this by
    /// reconstructing a descriptor with `kind = "noop"` because that
    /// is the only kind the noop factory recognises today.
    pub async fn insert_descriptor(&self, descriptor: &AgentDescriptor) -> Result<(), DomainError> {
        let attributes: JsonValue = serde_json::to_value(&descriptor.attributes)
            .map_err(|e| serde_to_domain(&e, "insert_descriptor"))?;
        let result = sqlx::query(
            "
            INSERT INTO agents (agent_id, specialty, kind, attributes)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (agent_id) DO NOTHING
            ",
        )
        .bind(descriptor.id.as_str())
        .bind(descriptor.specialty.as_str())
        .bind(descriptor.kind.as_str())
        .bind(&attributes)
        .execute(self.pool.inner())
        .await
        .map_err(|e| sqlx_to_domain(e, "insert_descriptor"))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::AlreadyExists { what: "agent" });
        }
        Ok(())
    }

    async fn load_descriptor(&self, id: &AgentId) -> Result<Option<AgentDescriptor>, DomainError> {
        let row = sqlx::query(
            "SELECT agent_id, specialty, kind, attributes FROM agents WHERE agent_id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(self.pool.inner())
        .await
        .map_err(|e| sqlx_to_domain(e, "resolve"))?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(descriptor_from_row(&row)?))
    }
}

#[async_trait]
impl AgentRegistryPort for PostgresAgentRegistry {
    async fn register(&self, agent: Arc<dyn AgentPort>) -> Result<(), DomainError> {
        // The port surface carries a live agent, but we persist
        // descriptors only. Until a dispatching factory lands, every
        // agent is materialised by NoopAgentFactory, so recording
        // kind="noop" is the honest projection.
        let descriptor = AgentDescriptor {
            id: agent.id().clone(),
            specialty: agent.specialty().clone(),
            kind: AgentKind::new("noop")?,
            attributes: Attributes::empty(),
        };
        self.insert_descriptor(&descriptor).await
    }

    async fn unregister(&self, id: &AgentId) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM agents WHERE agent_id = $1")
            .bind(id.as_str())
            .execute(self.pool.inner())
            .await
            .map_err(|e| sqlx_to_domain(e, "unregister"))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound { what: "agent" });
        }
        Ok(())
    }
}

#[async_trait]
impl AgentResolverPort for PostgresAgentRegistry {
    async fn resolve(&self, id: &AgentId) -> Result<Arc<dyn AgentPort>, DomainError> {
        let descriptor = self
            .load_descriptor(id)
            .await?
            .ok_or(DomainError::NotFound { what: "agent" })?;
        self.factory.create(descriptor).await
    }
}

fn descriptor_from_row(row: &sqlx::postgres::PgRow) -> Result<AgentDescriptor, DomainError> {
    let agent_id: String = row
        .try_get("agent_id")
        .map_err(|e| sqlx_to_domain(e, "resolve"))?;
    let specialty: String = row
        .try_get("specialty")
        .map_err(|e| sqlx_to_domain(e, "resolve"))?;
    let kind: String = row
        .try_get("kind")
        .map_err(|e| sqlx_to_domain(e, "resolve"))?;
    let attrs_value: JsonValue = row
        .try_get("attributes")
        .map_err(|e| sqlx_to_domain(e, "resolve"))?;
    let attributes: Attributes =
        serde_json::from_value(attrs_value).map_err(|e| serde_to_domain(&e, "resolve"))?;
    Ok(AgentDescriptor {
        id: AgentId::new(agent_id)?,
        specialty: Specialty::new(specialty)?,
        kind: AgentKind::new(kind)?,
        attributes,
    })
}
