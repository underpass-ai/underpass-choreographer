//! Postgres implementation of [`DeliberationRepositoryPort`].
//!
//! Storage shape: the full `Deliberation` aggregate is serialised as
//! JSONB on `deliberations.body`; a handful of scalar columns are
//! kept alongside for indexable filters (specialty, phase) and a
//! cheap winner lookup. See `migrations/postgres/0001_deliberations.sql`.

use async_trait::async_trait;
use choreo_core::entities::{Deliberation, DeliberationPhase};
use choreo_core::error::DomainError;
use choreo_core::ports::DeliberationRepositoryPort;
use choreo_core::value_objects::TaskId;
use serde_json::Value as JsonValue;
use sqlx::Row;

use super::error::{serde_to_domain, sqlx_to_domain};
use super::pool::PostgresPool;

#[derive(Clone)]
pub struct PostgresDeliberationRepository {
    pool: PostgresPool,
}

impl std::fmt::Debug for PostgresDeliberationRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresDeliberationRepository").finish()
    }
}

impl PostgresDeliberationRepository {
    #[must_use]
    pub fn new(pool: PostgresPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeliberationRepositoryPort for PostgresDeliberationRepository {
    async fn save(&self, deliberation: &Deliberation) -> Result<(), DomainError> {
        let body: JsonValue =
            serde_json::to_value(deliberation).map_err(|e| serde_to_domain(&e, "save"))?;
        let phase = phase_name(deliberation.phase());
        let winner = deliberation
            .ranking()
            .first()
            .map(|p| p.as_str().to_owned());

        sqlx::query(
            "
            INSERT INTO deliberations
                (task_id, specialty, phase, winner_proposal_id, body, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (task_id) DO UPDATE SET
                specialty          = EXCLUDED.specialty,
                phase              = EXCLUDED.phase,
                winner_proposal_id = EXCLUDED.winner_proposal_id,
                body               = EXCLUDED.body,
                updated_at         = NOW()
            ",
        )
        .bind(deliberation.task_id().as_str())
        .bind(deliberation.specialty().as_str())
        .bind(phase)
        .bind(winner)
        .bind(&body)
        .execute(self.pool.inner())
        .await
        .map_err(|e| sqlx_to_domain(e, "save"))?;

        Ok(())
    }

    async fn get(&self, task_id: &TaskId) -> Result<Deliberation, DomainError> {
        let row = sqlx::query("SELECT body FROM deliberations WHERE task_id = $1")
            .bind(task_id.as_str())
            .fetch_one(self.pool.inner())
            .await
            .map_err(|e| sqlx_to_domain(e, "get"))?;
        let body: JsonValue = row.try_get("body").map_err(|e| sqlx_to_domain(e, "get"))?;
        serde_json::from_value(body).map_err(|e| serde_to_domain(&e, "get"))
    }

    async fn exists(&self, task_id: &TaskId) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 AS one FROM deliberations WHERE task_id = $1")
            .bind(task_id.as_str())
            .fetch_optional(self.pool.inner())
            .await
            .map_err(|e| sqlx_to_domain(e, "exists"))?;
        Ok(row.is_some())
    }
}

fn phase_name(phase: DeliberationPhase) -> &'static str {
    match phase {
        DeliberationPhase::Proposing => "Proposing",
        DeliberationPhase::Revising => "Revising",
        DeliberationPhase::Validating => "Validating",
        DeliberationPhase::Scoring => "Scoring",
        DeliberationPhase::Completed => "Completed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phase_has_a_stable_column_value() {
        use std::collections::HashSet;
        let names: HashSet<&str> = [
            DeliberationPhase::Proposing,
            DeliberationPhase::Revising,
            DeliberationPhase::Validating,
            DeliberationPhase::Scoring,
            DeliberationPhase::Completed,
        ]
        .iter()
        .map(|p| phase_name(*p))
        .collect();
        assert_eq!(names.len(), 5);
    }
}
