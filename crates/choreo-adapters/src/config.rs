//! Configuration adapter.
//!
//! Loads [`ServiceConfig`] from environment variables prefixed with
//! `CHOREO_`. Defaults match the chart's `values.yaml`.

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{ConfigurationPort, ServiceConfig};
use figment::{
    providers::{Env, Serialized},
    Figment,
};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Read-only configuration adapter backed by process environment.
///
/// Recognised variables (prefixed with `CHOREO_`):
///
/// | Var                        | Default               |
/// |----------------------------|-----------------------|
/// | `CHOREO_GRPC_PORT`         | `50055`               |
/// | `CHOREO_NATS_ENABLED`      | `true`                |
/// | `CHOREO_NATS_URL`          | `nats://nats:4222`    |
/// | `CHOREO_TRIGGER_SUBJECT`   | `choreo.trigger.>`    |
/// | `CHOREO_PUBLISH_PREFIX`    | `choreo`              |
///
/// The adapter performs no IO at construction; `load` returns a
/// snapshot of the current environment.
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvConfiguration;

impl EnvConfiguration {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Defaults {
    grpc_port: u16,
    nats_enabled: bool,
    nats_url: String,
    trigger_subject: String,
    publish_prefix: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            grpc_port: 50055,
            nats_enabled: true,
            nats_url: "nats://nats:4222".to_owned(),
            trigger_subject: "choreo.trigger.>".to_owned(),
            publish_prefix: "choreo".to_owned(),
        }
    }
}

#[async_trait]
impl ConfigurationPort for EnvConfiguration {
    async fn load(&self) -> Result<ServiceConfig, DomainError> {
        let figment = Figment::from(Serialized::defaults(Defaults::default()))
            .merge(Env::prefixed("CHOREO_").split("__"));

        let loaded: Defaults = figment.extract().map_err(|err| {
            debug!(error = %err, "configuration load failed");
            DomainError::InvariantViolated {
                reason: "invalid choreographer environment configuration",
            }
        })?;

        Ok(ServiceConfig {
            grpc_port: loaded.grpc_port,
            nats_enabled: loaded.nats_enabled,
            nats_url: loaded.nats_url,
            trigger_subject: loaded.trigger_subject,
            publish_prefix: loaded.publish_prefix,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Process-env mutations are shared state. All env-touching tests
    /// in this module serialize on this single mutex so racy
    /// `set_var` / `remove_var` across tests cannot corrupt each
    /// other's snapshot.
    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn clear_env() {
        for (k, _) in std::env::vars() {
            if k.starts_with("CHOREO_") {
                std::env::remove_var(k);
            }
        }
    }

    #[tokio::test]
    async fn defaults_when_env_is_empty() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();

        let cfg = EnvConfiguration::new().load().await.unwrap();
        assert_eq!(cfg.grpc_port, 50055);
        assert!(cfg.nats_enabled);
        assert_eq!(cfg.nats_url, "nats://nats:4222");
        assert_eq!(cfg.trigger_subject, "choreo.trigger.>");
        assert_eq!(cfg.publish_prefix, "choreo");
    }

    #[tokio::test]
    async fn env_overrides_defaults() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();
        std::env::set_var("CHOREO_GRPC_PORT", "50099");
        std::env::set_var("CHOREO_NATS_ENABLED", "false");
        std::env::set_var("CHOREO_PUBLISH_PREFIX", "choreo.prod");

        let cfg = EnvConfiguration::new().load().await.unwrap();
        assert_eq!(cfg.grpc_port, 50099);
        assert!(!cfg.nats_enabled);
        assert_eq!(cfg.publish_prefix, "choreo.prod");

        clear_env();
    }

    #[tokio::test]
    async fn invalid_env_value_yields_domain_error() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();
        std::env::set_var("CHOREO_GRPC_PORT", "not-a-port");

        let err = EnvConfiguration::new().load().await.unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { .. }));

        clear_env();
    }
}
