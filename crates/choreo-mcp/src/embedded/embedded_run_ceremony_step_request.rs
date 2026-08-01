use choreo_app::usecases::RunCeremonyStepInput;
use choreo_core::value_objects::{
    AuditActorKind, CeremonyId, DurationMs, IdempotencyKey, LeaseOwnerId, StepId,
};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;
use uuid::Uuid;

use super::embedded_request_fields::{
    load_instance_definition, optional_string, optional_u64, required_actor_kind, required_string,
};

const DEFAULT_LEASE_OWNER_ID: &str = "choreo-mcp-embedded";
const DEFAULT_LEASE_TTL_MS: u64 = 30_000;

/// Validated MCP request that executes one step on a persistent instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedRunCeremonyStepRequest {
    ceremony_id: CeremonyId,
    step_id: StepId,
    actor_kind: AuditActorKind,
    lease_owner_id: LeaseOwnerId,
    idempotency_key: IdempotencyKey,
    lease_ttl: DurationMs,
}

impl EmbeddedRunCeremonyStepRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        let (definition, _instance) =
            load_instance_definition(choreographer, &self.ceremony_id).await?;
        let role_id = definition
            .role_id_for_step(&self.step_id)
            .map_err(|error| format!("ceremony step has no authorized role: {error}"))?;

        choreographer
            .run_step(RunCeremonyStepInput::new(
                self.ceremony_id.clone(),
                role_id,
                self.actor_kind,
                self.step_id,
                self.lease_owner_id,
                self.idempotency_key,
                self.lease_ttl,
            ))
            .await
            .map_err(|error| format!("failed to run ceremony step: {error}"))?;
        Ok(self.ceremony_id)
    }
}

impl TryFrom<&Value> for EmbeddedRunCeremonyStepRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        let lease_owner_id = optional_string(object, "lease_owner_id")?
            .unwrap_or_else(|| DEFAULT_LEASE_OWNER_ID.to_owned());
        let idempotency_key = optional_string(object, "idempotency_key")?
            .unwrap_or_else(|| format!("choreo-mcp-{}", Uuid::new_v4()));
        let lease_ttl_ms = optional_u64(object, "lease_ttl_ms")?.unwrap_or_default();

        Ok(Self {
            ceremony_id: CeremonyId::new(required_string(object, "ceremony_id")?)
                .map_err(|error| error.to_string())?,
            step_id: StepId::new(required_string(object, "step_id")?)
                .map_err(|error| error.to_string())?,
            actor_kind: required_actor_kind(object, "actor_kind")?,
            lease_owner_id: LeaseOwnerId::new(lease_owner_id).map_err(|error| error.to_string())?,
            idempotency_key: IdempotencyKey::new(idempotency_key)
                .map_err(|error| error.to_string())?,
            lease_ttl: DurationMs::from_millis(if lease_ttl_ms == 0 {
                DEFAULT_LEASE_TTL_MS
            } else {
                lease_ttl_ms
            }),
        })
    }
}
