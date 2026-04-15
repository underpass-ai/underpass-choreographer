//! [`ConfigurationPort`] — read-only access to validated service
//! configuration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Minimal configuration surface the core needs to reason about the
/// service. Adapter implementations (env vars, Figment, Kubernetes
/// downward API, …) map to this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub grpc_port: u16,
    pub nats_enabled: bool,
    pub nats_url: String,
    pub trigger_subject: String,
    pub publish_prefix: String,
}

#[async_trait]
pub trait ConfigurationPort: Send + Sync {
    async fn load(&self) -> Result<ServiceConfig, DomainError>;
}
