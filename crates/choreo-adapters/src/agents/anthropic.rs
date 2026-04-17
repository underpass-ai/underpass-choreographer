//! Anthropic Messages API adapter.
//!
//! Implements [`AgentPort`] against
//! `POST https://api.anthropic.com/v1/messages`. Uses the Anthropic
//! prompt-caching feature (`cache_control: ephemeral`) on the system
//! block so repeated `generate` / `critique` / `revise` calls on a
//! long-lived agent reuse the cached system prompt rather than pay
//! for its tokens on every request.
//!
//! **Provider-agnostic by convention.** The system prompts here speak
//! only the Choreographer's vocabulary (agent, specialty, task,
//! proposal, critique, revision). They do not assume any application
//! domain. Operators can attach domain-specific context through
//! `TaskConstraints.rubric` which is passed through verbatim to the
//! model.
//!
//! **Secrets are masked.** [`AnthropicApiKey`] implements `Debug` with
//! a fixed redaction so an accidental `dbg!` or `tracing` field never
//! leaks the credential.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use choreo_core::entities::TaskConstraints;
use choreo_core::error::DomainError;
use choreo_core::ports::{AgentPort, Critique, DraftRequest, Revision};
use choreo_core::value_objects::{AgentId, Specialty};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::prompts;

const ANTHROPIC_VERSION_HEADER: &str = "2023-06-01";
const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com";
const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
const DEFAULT_MAX_TOKENS: u32 = 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Secret wrapper
// ---------------------------------------------------------------------------

/// Opaque API key. Its `Debug` impl is a fixed redaction so the
/// secret value cannot slip into logs, event payloads, or test
/// snapshots by accident.
#[derive(Clone)]
pub struct AnthropicApiKey(String);

impl AnthropicApiKey {
    /// Validate and construct. Empty / whitespace-only keys are
    /// rejected at the boundary so a misconfigured deployment fails
    /// fast instead of receiving a 401 on the first request.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = raw.into().trim().to_owned();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "anthropic.api_key",
            });
        }
        Ok(Self(trimmed))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AnthropicApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AnthropicApiKey(**redacted**)")
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Static configuration for the Anthropic adapter.
///
/// All fields are validated on construction. Defaults match the
/// Messages API's current conventions.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    api_key: AnthropicApiKey,
    endpoint: String,
    model: String,
    max_tokens: u32,
    timeout: Duration,
}

impl AnthropicConfig {
    #[must_use]
    pub fn new(api_key: AnthropicApiKey) -> Self {
        Self {
            api_key,
            endpoint: DEFAULT_ENDPOINT.to_owned(),
            model: DEFAULT_MODEL.to_owned(),
            max_tokens: DEFAULT_MAX_TOKENS,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Result<Self, DomainError> {
        let value = endpoint.into().trim().to_owned();
        if value.is_empty() {
            return Err(DomainError::EmptyField {
                field: "anthropic.endpoint",
            });
        }
        self.endpoint = value;
        Ok(self)
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Result<Self, DomainError> {
        let value = model.into().trim().to_owned();
        if value.is_empty() {
            return Err(DomainError::EmptyField {
                field: "anthropic.model",
            });
        }
        self.model = value;
        Ok(self)
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Result<Self, DomainError> {
        if max_tokens == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "anthropic.max_tokens",
            });
        }
        self.max_tokens = max_tokens;
        Ok(self)
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

/// Anthropic-backed agent. One instance typically handles many
/// deliberations; the underlying [`reqwest::Client`] keeps a
/// connection pool between calls.
pub struct AnthropicAgent {
    id: AgentId,
    specialty: Specialty,
    config: AnthropicConfig,
    http: Client,
}

impl fmt::Debug for AnthropicAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicAgent")
            .field("id", &self.id)
            .field("specialty", &self.specialty)
            .field("config", &self.config)
            .finish()
    }
}

impl AnthropicAgent {
    pub fn new(
        id: AgentId,
        specialty: Specialty,
        config: AnthropicConfig,
    ) -> Result<Self, DomainError> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|err| {
                debug!(error = %err, "anthropic: failed to build http client");
                DomainError::InvariantViolated {
                    reason: "anthropic: failed to build http client",
                }
            })?;
        Ok(Self {
            id,
            specialty,
            config,
            http,
        })
    }

    async fn complete(
        &self,
        system: String,
        user: String,
        op: &str,
    ) -> Result<String, DomainError> {
        let body = MessagesRequest {
            model: &self.config.model,
            max_tokens: self.config.max_tokens,
            system: vec![SystemBlock {
                ty: "text",
                text: system,
                cache_control: Some(CacheControl { ty: "ephemeral" }),
            }],
            messages: vec![Message {
                role: "user",
                content: user,
            }],
        };

        let url = format!("{}/v1/messages", self.config.endpoint.trim_end_matches('/'));
        let response = self
            .http
            .post(&url)
            .header("x-api-key", self.config.api_key.expose())
            .header("anthropic-version", ANTHROPIC_VERSION_HEADER)
            .json(&body)
            .send()
            .await
            .map_err(|err| {
                warn!(
                    op,
                    agent_id = self.id.as_str(),
                    error = %err,
                    "anthropic: request failed"
                );
                DomainError::InvariantViolated {
                    reason: "anthropic: request failed",
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            warn!(
                op,
                agent_id = self.id.as_str(),
                status = status.as_u16(),
                body = %body_text,
                "anthropic: upstream returned non-success"
            );
            return Err(classify_error(status));
        }

        let parsed: MessagesResponse = response.json().await.map_err(|err| {
            warn!(
                op,
                agent_id = self.id.as_str(),
                error = %err,
                "anthropic: malformed response body"
            );
            DomainError::InvariantViolated {
                reason: "anthropic: malformed response body",
            }
        })?;

        extract_text(parsed).inspect_err(|_| {
            warn!(
                op,
                agent_id = self.id.as_str(),
                "anthropic: empty text content"
            );
        })
    }
}

#[async_trait]
impl AgentPort for AnthropicAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn specialty(&self) -> &Specialty {
        &self.specialty
    }

    async fn generate(&self, request: DraftRequest) -> Result<Revision, DomainError> {
        let system = prompts::system_prompt_generate(self.id.as_str(), self.specialty.as_str());
        let user = prompts::user_prompt_generate(&request);
        let content = self.complete(system, user, "generate").await?;
        Ok(Revision { content })
    }

    async fn critique(
        &self,
        peer_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<Critique, DomainError> {
        let system = prompts::system_prompt_critique(self.id.as_str(), self.specialty.as_str());
        let user = prompts::user_prompt_critique(peer_content, constraints);
        let feedback = self.complete(system, user, "critique").await?;
        Ok(Critique { feedback })
    }

    async fn revise(
        &self,
        own_content: &str,
        critique: &Critique,
    ) -> Result<Revision, DomainError> {
        let system = prompts::system_prompt_revise(self.id.as_str(), self.specialty.as_str());
        let user = prompts::user_prompt_revise(own_content, critique);
        let content = self.complete(system, user, "revise").await?;
        Ok(Revision { content })
    }
}

// ---------------------------------------------------------------------------
// Wire types (matching the Anthropic Messages API)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: Vec<SystemBlock>,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct SystemBlock {
    #[serde(rename = "type")]
    ty: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    ty: &'static str,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type", default)]
    ty: String,
    #[serde(default)]
    text: Option<String>,
}

fn extract_text(resp: MessagesResponse) -> Result<String, DomainError> {
    let combined: Vec<String> = resp
        .content
        .into_iter()
        .filter(|b| b.ty == "text")
        .filter_map(|b| b.text)
        .collect();
    if combined.is_empty() {
        return Err(DomainError::InvariantViolated {
            reason: "anthropic: no text content in response",
        });
    }
    let joined = combined.join("\n");
    if joined.trim().is_empty() {
        return Err(DomainError::InvariantViolated {
            reason: "anthropic: empty text content",
        });
    }
    Ok(joined)
}

fn classify_error(status: StatusCode) -> DomainError {
    match status.as_u16() {
        401 | 403 => DomainError::InvariantViolated {
            reason: "anthropic: unauthorized",
        },
        429 => DomainError::InvariantViolated {
            reason: "anthropic: rate-limited",
        },
        400..=499 => DomainError::InvariantViolated {
            reason: "anthropic: bad request",
        },
        _ => DomainError::InvariantViolated {
            reason: "anthropic: upstream error",
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{Rounds, Rubric, TaskDescription};
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_agent(server: &MockServer) -> AnthropicAgent {
        let config = AnthropicConfig::new(AnthropicApiKey::new("sk-test-12345").unwrap())
            .with_endpoint(server.uri())
            .unwrap()
            .with_model("claude-haiku-4-5-20251001")
            .unwrap()
            .with_max_tokens(256)
            .unwrap()
            .with_timeout(Duration::from_secs(5));
        AnthropicAgent::new(
            AgentId::new("anth-1").unwrap(),
            Specialty::new("triage").unwrap(),
            config,
        )
        .unwrap()
    }

    fn draft() -> DraftRequest {
        DraftRequest {
            task: TaskDescription::new("Investigate the incoming alert.").unwrap(),
            constraints: TaskConstraints::new(Rubric::empty(), Rounds::default(), None, None),
            diverse: true,
        }
    }

    fn messages_response(text: &str) -> serde_json::Value {
        json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": text}
            ],
            "model": "claude-haiku-4-5-20251001",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 12}
        })
    }

    // --- api key / debug -------------------------------------------------

    #[test]
    fn api_key_debug_is_redacted() {
        let k = AnthropicApiKey::new("sk-secret-very-long-123").unwrap();
        let shown = format!("{k:?}");
        assert!(
            !shown.contains("sk-secret"),
            "api key leaked through Debug: {shown}"
        );
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn empty_api_key_rejected() {
        let err = AnthropicApiKey::new("   ").unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "anthropic.api_key"
            }
        ));
    }

    #[test]
    fn agent_debug_does_not_leak_secret() {
        let cfg = AnthropicConfig::new(AnthropicApiKey::new("sk-shhh").unwrap());
        let agent = AnthropicAgent::new(
            AgentId::new("a").unwrap(),
            Specialty::new("triage").unwrap(),
            cfg,
        )
        .unwrap();
        let shown = format!("{agent:?}");
        assert!(!shown.contains("sk-shhh"));
        assert!(shown.contains("redacted"));
    }

    // --- config validation -----------------------------------------------

    #[test]
    fn empty_endpoint_rejected() {
        let cfg = AnthropicConfig::new(AnthropicApiKey::new("k").unwrap());
        let err = cfg.with_endpoint("  ").unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "anthropic.endpoint"
            }
        ));
    }

    #[test]
    fn zero_max_tokens_rejected() {
        let cfg = AnthropicConfig::new(AnthropicApiKey::new("k").unwrap());
        let err = cfg.with_max_tokens(0).unwrap_err();
        assert!(matches!(
            err,
            DomainError::MustBeNonZero {
                field: "anthropic.max_tokens"
            }
        ));
    }

    // --- happy path ------------------------------------------------------

    #[tokio::test]
    async fn generate_hits_the_messages_endpoint_and_returns_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("anthropic-version", ANTHROPIC_VERSION_HEADER))
            .and(header("x-api-key", "sk-test-12345"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(messages_response("a proposal.")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent(&server);
        let out = agent.generate(draft()).await.unwrap();
        assert_eq!(out.content, "a proposal.");
    }

    #[tokio::test]
    async fn critique_returns_feedback_string() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(messages_response("consider edge case X")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent(&server);
        let out = agent
            .critique("peer proposal", &TaskConstraints::default())
            .await
            .unwrap();
        assert_eq!(out.feedback, "consider edge case X");
    }

    #[tokio::test]
    async fn revise_returns_new_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(messages_response("better proposal.")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent(&server);
        let out = agent
            .revise(
                "old proposal",
                &Critique {
                    feedback: "tighten X".to_owned(),
                },
            )
            .await
            .unwrap();
        assert_eq!(out.content, "better proposal.");
    }

    // --- request body shape ---------------------------------------------

    #[tokio::test]
    async fn request_body_uses_ephemeral_cache_control_on_system_block() {
        use wiremock::matchers::body_string_contains;

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(body_string_contains(r#""type":"ephemeral""#))
            .and(body_string_contains(
                r#""model":"claude-haiku-4-5-20251001""#,
            ))
            .and(body_string_contains(r#""cache_control""#))
            .respond_with(ResponseTemplate::new(200).set_body_json(messages_response("ok")))
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent(&server);
        agent.generate(draft()).await.unwrap();
    }

    // --- error handling --------------------------------------------------

    #[tokio::test]
    async fn unauthorized_status_maps_to_domain_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_string("invalid key"))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "anthropic: unauthorized"
            }
        ));
    }

    #[tokio::test]
    async fn rate_limit_status_maps_to_domain_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(429))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "anthropic: rate-limited"
            }
        ));
    }

    #[tokio::test]
    async fn upstream_5xx_maps_to_domain_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "anthropic: upstream error"
            }
        ));
    }

    #[tokio::test]
    async fn malformed_response_body_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "anthropic: malformed response body"
            }
        ));
    }

    #[tokio::test]
    async fn empty_text_content_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "msg_x",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-haiku-4-5-20251001",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 0, "output_tokens": 0}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    #[tokio::test]
    async fn tool_use_blocks_are_ignored_extracting_only_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "msg_x",
                "type": "message",
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "t1", "name": "x", "input": {}},
                    {"type": "text", "text": "the actual proposal"}
                ],
                "model": "claude-haiku-4-5-20251001",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 1, "output_tokens": 1}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let out = test_agent(&server).generate(draft()).await.unwrap();
        assert_eq!(out.content, "the actual proposal");
    }

    // Prompt / rubric tests live in `super::prompts` — covered once,
    // owned by the module that holds the shared helpers.
}
