//! Configuration adapter.
//!
//! Loads [`ServiceConfig`] from environment variables prefixed with
//! `CHOREO_`. Defaults match the chart's `values.yaml`.

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{ConfigurationPort, GrpcTlsConfig, ServiceConfig};
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
/// | Var                              | Default               |
/// |----------------------------------|-----------------------|
/// | `CHOREO_GRPC_PORT`               | `50055`               |
/// | `CHOREO_NATS_ENABLED`            | `true`                |
/// | `CHOREO_NATS_URL`                | `nats://nats:4222`    |
/// | `CHOREO_TRIGGER_SUBJECT`         | `choreo.trigger.>`    |
/// | `CHOREO_PUBLISH_PREFIX`          | `choreo`              |
/// | `CHOREO_POSTGRES_URL`            | (unset)               |
/// | `CHOREO_CEREMONY_STORE_PATH`     | (unset)               |
/// | `CHOREO_GRPC_TLS_MODE`           | `none`                |
/// | `CHOREO_GRPC_TLS_CERT_PATH`      | (unset)               |
/// | `CHOREO_GRPC_TLS_KEY_PATH`       | (unset)               |
/// | `CHOREO_GRPC_TLS_CLIENT_CA_PATH` | (unset)               |
///
/// `CHOREO_GRPC_TLS_MODE` accepts `none`, `server`, or `mutual`.
/// `server` requires both `_CERT_PATH` and `_KEY_PATH`; `mutual`
/// additionally requires `_CLIENT_CA_PATH`. Validation runs at load
/// time so the adapter never produces an internally inconsistent
/// snapshot.
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
    http_port: u16,
    nats_enabled: bool,
    nats_url: String,
    trigger_subject: String,
    publish_prefix: String,
    postgres_url: String,
    ceremony_store_path: String,
    grpc_tls_mode: String,
    grpc_tls_cert_path: String,
    grpc_tls_key_path: String,
    grpc_tls_client_ca_path: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            grpc_port: 50055,
            http_port: 8080,
            nats_enabled: true,
            nats_url: "nats://nats:4222".to_owned(),
            trigger_subject: "choreo.trigger.>".to_owned(),
            publish_prefix: "choreo".to_owned(),
            postgres_url: String::new(),
            ceremony_store_path: String::new(),
            grpc_tls_mode: "none".to_owned(),
            grpc_tls_cert_path: String::new(),
            grpc_tls_key_path: String::new(),
            grpc_tls_client_ca_path: String::new(),
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

        let postgres_url = if loaded.postgres_url.trim().is_empty() {
            None
        } else {
            Some(loaded.postgres_url)
        };

        let ceremony_store_path = nonempty(&loaded.ceremony_store_path);

        let grpc_tls = build_grpc_tls(
            &loaded.grpc_tls_mode,
            &loaded.grpc_tls_cert_path,
            &loaded.grpc_tls_key_path,
            &loaded.grpc_tls_client_ca_path,
        )?;

        Ok(ServiceConfig {
            grpc_port: loaded.grpc_port,
            http_port: loaded.http_port,
            nats_enabled: loaded.nats_enabled,
            nats_url: loaded.nats_url,
            trigger_subject: loaded.trigger_subject,
            publish_prefix: loaded.publish_prefix,
            postgres_url,
            ceremony_store_path,
            grpc_tls,
        })
    }
}

fn nonempty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn build_grpc_tls(
    mode: &str,
    cert_path: &str,
    key_path: &str,
    client_ca_path: &str,
) -> Result<GrpcTlsConfig, DomainError> {
    let mode = mode.trim().to_ascii_lowercase();
    match mode.as_str() {
        "" | "none" | "disabled" => Ok(GrpcTlsConfig::Disabled),
        "server" => {
            let cert = nonempty(cert_path).ok_or(DomainError::EmptyField {
                field: "grpc_tls.cert_path",
            })?;
            let key = nonempty(key_path).ok_or(DomainError::EmptyField {
                field: "grpc_tls.key_path",
            })?;
            Ok(GrpcTlsConfig::Server {
                cert_path: cert,
                key_path: key,
            })
        }
        "mutual" | "mtls" => {
            let cert = nonempty(cert_path).ok_or(DomainError::EmptyField {
                field: "grpc_tls.cert_path",
            })?;
            let key = nonempty(key_path).ok_or(DomainError::EmptyField {
                field: "grpc_tls.key_path",
            })?;
            let ca = nonempty(client_ca_path).ok_or(DomainError::EmptyField {
                field: "grpc_tls.client_ca_path",
            })?;
            Ok(GrpcTlsConfig::Mutual {
                cert_path: cert,
                key_path: key,
                client_ca_path: ca,
            })
        }
        _ => Err(DomainError::InvariantViolated {
            reason: "grpc_tls.mode must be one of: none, server, mutual",
        }),
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
        assert_eq!(cfg.http_port, 8080);
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
    async fn postgres_url_defaults_to_none_and_trims_empty() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();

        let cfg = EnvConfiguration::new().load().await.unwrap();
        assert!(cfg.postgres_url.is_none());

        std::env::set_var("CHOREO_POSTGRES_URL", "   ");
        let cfg = EnvConfiguration::new().load().await.unwrap();
        assert!(
            cfg.postgres_url.is_none(),
            "whitespace-only must be treated as unset"
        );

        std::env::set_var("CHOREO_POSTGRES_URL", "postgres://x/y");
        let cfg = EnvConfiguration::new().load().await.unwrap();
        assert_eq!(cfg.postgres_url.as_deref(), Some("postgres://x/y"));

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

    #[tokio::test]
    async fn grpc_tls_defaults_to_disabled() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();

        let cfg = EnvConfiguration::new().load().await.unwrap();
        assert_eq!(cfg.grpc_tls, GrpcTlsConfig::Disabled);
        assert_eq!(cfg.grpc_tls.mode_name(), "none");
    }

    #[tokio::test]
    async fn grpc_tls_server_mode_requires_cert_and_key() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();
        std::env::set_var("CHOREO_GRPC_TLS_MODE", "server");
        // cert + key missing
        let err = EnvConfiguration::new().load().await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "grpc_tls.cert_path"
            }
        ));
        clear_env();
    }

    #[tokio::test]
    async fn grpc_tls_server_mode_loads_when_both_paths_set() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();
        std::env::set_var("CHOREO_GRPC_TLS_MODE", "server");
        std::env::set_var("CHOREO_GRPC_TLS_CERT_PATH", "/etc/tls/tls.crt");
        std::env::set_var("CHOREO_GRPC_TLS_KEY_PATH", "/etc/tls/tls.key");

        let cfg = EnvConfiguration::new().load().await.unwrap();
        assert_eq!(
            cfg.grpc_tls,
            GrpcTlsConfig::Server {
                cert_path: "/etc/tls/tls.crt".to_owned(),
                key_path: "/etc/tls/tls.key".to_owned(),
            }
        );
        assert_eq!(cfg.grpc_tls.mode_name(), "server");
        clear_env();
    }

    #[tokio::test]
    async fn grpc_tls_mutual_mode_requires_client_ca() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();
        std::env::set_var("CHOREO_GRPC_TLS_MODE", "mutual");
        std::env::set_var("CHOREO_GRPC_TLS_CERT_PATH", "/etc/tls/tls.crt");
        std::env::set_var("CHOREO_GRPC_TLS_KEY_PATH", "/etc/tls/tls.key");
        // client CA missing

        let err = EnvConfiguration::new().load().await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "grpc_tls.client_ca_path"
            }
        ));
        clear_env();
    }

    #[tokio::test]
    async fn grpc_tls_mutual_mode_loads_when_all_three_paths_set() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();
        std::env::set_var("CHOREO_GRPC_TLS_MODE", "mutual");
        std::env::set_var("CHOREO_GRPC_TLS_CERT_PATH", "/etc/tls/tls.crt");
        std::env::set_var("CHOREO_GRPC_TLS_KEY_PATH", "/etc/tls/tls.key");
        std::env::set_var("CHOREO_GRPC_TLS_CLIENT_CA_PATH", "/etc/tls/ca.crt");

        let cfg = EnvConfiguration::new().load().await.unwrap();
        assert_eq!(
            cfg.grpc_tls,
            GrpcTlsConfig::Mutual {
                cert_path: "/etc/tls/tls.crt".to_owned(),
                key_path: "/etc/tls/tls.key".to_owned(),
                client_ca_path: "/etc/tls/ca.crt".to_owned(),
            }
        );
        assert_eq!(cfg.grpc_tls.mode_name(), "mutual");
        clear_env();
    }

    #[tokio::test]
    async fn grpc_tls_invalid_mode_is_rejected() {
        let _guard = ENV_LOCK.lock().await;
        clear_env();
        std::env::set_var("CHOREO_GRPC_TLS_MODE", "weird");

        let err = EnvConfiguration::new().load().await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "grpc_tls.mode must be one of: none, server, mutual"
            }
        ));
        clear_env();
    }
}
