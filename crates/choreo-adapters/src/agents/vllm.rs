//! vLLM adapter.
//!
//! vLLM serves the same Chat Completions wire shape as OpenAI
//! (`POST /v1/chat/completions`). Two differences justify a
//! dedicated adapter rather than reusing [`OpenAiAgent`]:
//!
//! 1. **Auth is optional.** vLLM deployments inside a cluster
//!    commonly run without authentication; a separate adapter
//!    makes the unauthenticated case first-class, not a special
//!    case of OpenAI.
//! 2. **Defaults are cluster-oriented.** The endpoint points at an
//!    in-cluster service, and no default model is assumed — every
//!    deployment loads a different one, so the operator must say.
//!
//! The wire format is deliberately identical to the OpenAI adapter
//! so both providers can coexist in a deliberation without surprise
//! for reviewers.
//!
//! **Secrets are masked.** When an optional bearer token is
//! configured, [`VllmBearerToken`] implements `Debug` with a fixed
//! redaction so it cannot slip through logs.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use choreo_core::entities::TaskConstraints;
use choreo_core::error::DomainError;
use choreo_core::ports::{AgentPort, Critique, DraftRequest, Revision};
use choreo_core::value_objects::{AgentId, Specialty};
use reqwest::{Client, RequestBuilder};
use tracing::{debug, warn};

use super::openai_compat::{self as wire, ChatMessage, ChatRequest, ChatResponse, ErrorStrings};
use super::prompts;

/// Static error reasons for the vLLM provider.
const VLLM_ERRORS: ErrorStrings = ErrorStrings {
    unauthorized: "vllm: unauthorized",
    rate_limited: "vllm: rate-limited",
    bad_request: "vllm: bad request",
    upstream_error: "vllm: upstream error",
    malformed_body: "vllm: malformed response body",
    no_choices: "vllm: no choices in response",
    missing_content: "vllm: choice has no message.content",
    empty_content: "vllm: empty text content",
};

const DEFAULT_ENDPOINT: &str = "http://vllm-server:8000";
const DEFAULT_MAX_TOKENS: u32 = 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Optional bearer token
// ---------------------------------------------------------------------------

/// Opaque bearer token for vLLM deployments fronted by an auth proxy.
/// Its `Debug` impl is a fixed redaction. Construction rejects empty
/// values; if authentication is not needed, do not construct one.
#[derive(Clone)]
pub struct VllmBearerToken(String);

impl VllmBearerToken {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = raw.into().trim().to_owned();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "vllm.bearer_token",
            });
        }
        Ok(Self(trimmed))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for VllmBearerToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VllmBearerToken(**redacted**)")
    }
}

// ---------------------------------------------------------------------------
// Optional mTLS client identity
// ---------------------------------------------------------------------------

/// Client certificate + private key (PEM-encoded) for mTLS-protected
/// vLLM endpoints. The bytes are held in memory and fed to
/// [`reqwest::Identity`] when the HTTP client is built; they never
/// appear in `Debug` output.
#[derive(Clone)]
pub struct VllmClientIdentity {
    pem_bundle: Vec<u8>,
}

impl VllmClientIdentity {
    /// Build an identity from concatenated cert + key PEM. The two
    /// inputs are joined with a newline so the PEM separators stay
    /// well-formed even if the caller forgot the trailing `\n`.
    pub fn from_cert_and_key(cert_pem: &[u8], key_pem: &[u8]) -> Result<Self, DomainError> {
        if cert_pem.is_empty() {
            return Err(DomainError::EmptyField {
                field: "vllm.client_cert_pem",
            });
        }
        if key_pem.is_empty() {
            return Err(DomainError::EmptyField {
                field: "vllm.client_key_pem",
            });
        }
        let mut bundle = Vec::with_capacity(cert_pem.len() + key_pem.len() + 1);
        bundle.extend_from_slice(cert_pem);
        if !cert_pem.ends_with(b"\n") {
            bundle.push(b'\n');
        }
        bundle.extend_from_slice(key_pem);
        Ok(Self { pem_bundle: bundle })
    }

    fn expose(&self) -> &[u8] {
        &self.pem_bundle
    }
}

impl fmt::Debug for VllmClientIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VllmClientIdentity(**redacted**)")
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Static configuration for the vLLM adapter. Every field is
/// validated on construction.
///
/// Unlike `OpenAiConfig` this has no mandatory credential. Model
/// must be explicitly set — vLLM deployments serve whichever weights
/// the operator loaded; there is no sensible default.
#[derive(Debug, Clone)]
pub struct VllmConfig {
    endpoint: String,
    model: String,
    bearer: Option<VllmBearerToken>,
    client_identity: Option<VllmClientIdentity>,
    max_tokens: u32,
    timeout: Duration,
}

impl VllmConfig {
    /// Build a config. Model must be non-empty.
    pub fn new(model: impl Into<String>) -> Result<Self, DomainError> {
        let model = model.into().trim().to_owned();
        if model.is_empty() {
            return Err(DomainError::EmptyField {
                field: "vllm.model",
            });
        }
        Ok(Self {
            endpoint: DEFAULT_ENDPOINT.to_owned(),
            model,
            bearer: None,
            client_identity: None,
            max_tokens: DEFAULT_MAX_TOKENS,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Result<Self, DomainError> {
        let value = endpoint.into().trim().to_owned();
        if value.is_empty() {
            return Err(DomainError::EmptyField {
                field: "vllm.endpoint",
            });
        }
        self.endpoint = value;
        Ok(self)
    }

    #[must_use]
    pub fn with_bearer(mut self, bearer: VllmBearerToken) -> Self {
        self.bearer = Some(bearer);
        self
    }

    /// Attach a client certificate + private key for mTLS-protected
    /// endpoints. The identity is handed to `reqwest` when the
    /// agent's HTTP client is built; if the PEM is malformed, the
    /// error surfaces at agent construction time.
    #[must_use]
    pub fn with_client_identity(mut self, identity: VllmClientIdentity) -> Self {
        self.client_identity = Some(identity);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Result<Self, DomainError> {
        if max_tokens == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "vllm.max_tokens",
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

pub struct VllmAgent {
    id: AgentId,
    specialty: Specialty,
    config: VllmConfig,
    http: Client,
}

impl fmt::Debug for VllmAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VllmAgent")
            .field("id", &self.id)
            .field("specialty", &self.specialty)
            .field("config", &self.config)
            .finish()
    }
}

impl VllmAgent {
    pub fn new(id: AgentId, specialty: Specialty, config: VllmConfig) -> Result<Self, DomainError> {
        let mut builder = Client::builder().timeout(config.timeout);
        if let Some(identity) = &config.client_identity {
            let parsed = reqwest::Identity::from_pem(identity.expose()).map_err(|err| {
                debug!(error = %err, "vllm: malformed client identity PEM");
                DomainError::InvariantViolated {
                    reason: "vllm: client identity PEM is malformed",
                }
            })?;
            builder = builder.identity(parsed);
        }
        let http = builder.build().map_err(|err| {
            debug!(error = %err, "vllm: failed to build http client");
            DomainError::InvariantViolated {
                reason: "vllm: failed to build http client",
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
        let body = ChatRequest {
            model: &self.config.model,
            max_tokens: self.config.max_tokens,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system,
                },
                ChatMessage {
                    role: "user",
                    content: user,
                },
            ],
        };

        let url = format!(
            "{}/v1/chat/completions",
            self.config.endpoint.trim_end_matches('/')
        );
        let request: RequestBuilder = self.http.post(&url).json(&body);
        let request = match &self.config.bearer {
            Some(token) => request.bearer_auth(token.expose()),
            None => request,
        };

        let response = request.send().await.map_err(|err| {
            warn!(
                op,
                agent_id = self.id.as_str(),
                error = %err,
                "vllm: request failed"
            );
            DomainError::InvariantViolated {
                reason: "vllm: request failed",
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
                "vllm: upstream returned non-success"
            );
            return Err(wire::classify_error(status, &VLLM_ERRORS));
        }

        let parsed: ChatResponse = response.json().await.map_err(|err| {
            warn!(
                op,
                agent_id = self.id.as_str(),
                error = %err,
                "vllm: malformed response body"
            );
            DomainError::InvariantViolated {
                reason: VLLM_ERRORS.malformed_body,
            }
        })?;

        wire::extract_text(parsed, &VLLM_ERRORS).inspect_err(|_| {
            warn!(op, agent_id = self.id.as_str(), "vllm: empty text content");
        })
    }
}

#[async_trait]
impl AgentPort for VllmAgent {
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

// Prompts live in `super::prompts`; wire types / extract_text /
// classify_error live in `super::openai_compat`. This adapter only
// carries its own config + optional-auth policy + error labels.

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{Rounds, Rubric, TaskDescription};
    use serde_json::json;
    use wiremock::matchers::{any, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_agent(server: &MockServer) -> VllmAgent {
        let config = VllmConfig::new("Qwen/Qwen3-0.6B")
            .unwrap()
            .with_endpoint(server.uri())
            .unwrap()
            .with_max_tokens(256)
            .unwrap()
            .with_timeout(Duration::from_secs(5));
        VllmAgent::new(
            AgentId::new("vllm-1").unwrap(),
            Specialty::new("triage").unwrap(),
            config,
        )
        .unwrap()
    }

    fn test_agent_with_bearer(server: &MockServer, token: &str) -> VllmAgent {
        let config = VllmConfig::new("Qwen/Qwen3-0.6B")
            .unwrap()
            .with_endpoint(server.uri())
            .unwrap()
            .with_bearer(VllmBearerToken::new(token).unwrap())
            .with_max_tokens(256)
            .unwrap();
        VllmAgent::new(
            AgentId::new("vllm-1").unwrap(),
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

    fn chat_response(text: &str) -> serde_json::Value {
        json!({
            "id": "cmpl-test",
            "object": "chat.completion",
            "model": "Qwen/Qwen3-0.6B",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": text},
                "finish_reason": "stop"
            }]
        })
    }

    // --- config ---------------------------------------------------------

    #[test]
    fn empty_model_is_rejected() {
        assert!(matches!(
            VllmConfig::new("   ").unwrap_err(),
            DomainError::EmptyField {
                field: "vllm.model"
            }
        ));
    }

    #[test]
    fn empty_endpoint_is_rejected() {
        let cfg = VllmConfig::new("m").unwrap();
        assert!(matches!(
            cfg.with_endpoint("  ").unwrap_err(),
            DomainError::EmptyField {
                field: "vllm.endpoint"
            }
        ));
    }

    #[test]
    fn zero_max_tokens_is_rejected() {
        let cfg = VllmConfig::new("m").unwrap();
        assert!(matches!(
            cfg.with_max_tokens(0).unwrap_err(),
            DomainError::MustBeNonZero {
                field: "vllm.max_tokens"
            }
        ));
    }

    // --- secret / debug -------------------------------------------------

    #[test]
    fn bearer_token_debug_is_redacted() {
        let t = VllmBearerToken::new("very-secret-token").unwrap();
        let shown = format!("{t:?}");
        assert!(!shown.contains("very-secret"), "token leaked: {shown}");
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn client_identity_debug_is_redacted() {
        let pem = b"-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----";
        let key = b"-----BEGIN PRIVATE KEY-----\nxyz\n-----END PRIVATE KEY-----";
        let identity = VllmClientIdentity::from_cert_and_key(pem, key).unwrap();
        let shown = format!("{identity:?}");
        assert!(!shown.contains("abc"), "cert leaked: {shown}");
        assert!(!shown.contains("xyz"), "key leaked: {shown}");
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn client_identity_rejects_empty_inputs() {
        let key = b"-----BEGIN PRIVATE KEY-----\nxyz\n-----END PRIVATE KEY-----";
        assert!(matches!(
            VllmClientIdentity::from_cert_and_key(b"", key).unwrap_err(),
            DomainError::EmptyField {
                field: "vllm.client_cert_pem"
            }
        ));
        let cert = b"-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----";
        assert!(matches!(
            VllmClientIdentity::from_cert_and_key(cert, b"").unwrap_err(),
            DomainError::EmptyField {
                field: "vllm.client_key_pem"
            }
        ));
    }

    #[test]
    fn malformed_client_identity_surfaces_at_agent_construction() {
        // PEM that looks structurally valid but is NOT a real cert.
        // `reqwest::Identity::from_pem` will reject this during
        // agent construction; that's the invariant we pin here.
        let cert = b"-----BEGIN CERTIFICATE-----\nnot-really-base64\n-----END CERTIFICATE-----";
        let key = b"-----BEGIN PRIVATE KEY-----\nalso-not-base64\n-----END PRIVATE KEY-----";
        let identity = VllmClientIdentity::from_cert_and_key(cert, key).unwrap();
        let config = VllmConfig::new("m").unwrap().with_client_identity(identity);
        let err = VllmAgent::new(
            AgentId::new("a").unwrap(),
            Specialty::new("triage").unwrap(),
            config,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "vllm: client identity PEM is malformed"
            }
        ));
    }

    #[test]
    fn empty_bearer_token_is_rejected() {
        assert!(matches!(
            VllmBearerToken::new("   ").unwrap_err(),
            DomainError::EmptyField {
                field: "vllm.bearer_token"
            }
        ));
    }

    #[test]
    fn agent_debug_does_not_leak_bearer() {
        let cfg = VllmConfig::new("m")
            .unwrap()
            .with_bearer(VllmBearerToken::new("tkn-abc-123").unwrap());
        let agent = VllmAgent::new(
            AgentId::new("a").unwrap(),
            Specialty::new("triage").unwrap(),
            cfg,
        )
        .unwrap();
        let shown = format!("{agent:?}");
        assert!(!shown.contains("tkn-abc-123"));
        assert!(shown.contains("redacted"));
    }

    // --- happy path (no auth) -------------------------------------------

    #[tokio::test]
    async fn generate_without_auth_sends_no_authorization_header() {
        let server = MockServer::start().await;

        // Match the happy path ...
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("ok proposal")))
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent(&server);
        let out = agent.generate(draft()).await.unwrap();
        assert_eq!(out.content, "ok proposal");

        // ... and verify the received request had no Authorization.
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1);
        assert!(
            received[0].headers.get("authorization").is_none(),
            "unauthenticated vllm deployments must not send an Authorization header"
        );
    }

    #[tokio::test]
    async fn critique_without_auth_works() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("tighten x")))
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent(&server);
        let out = agent
            .critique("peer", &TaskConstraints::default())
            .await
            .unwrap();
        assert_eq!(out.feedback, "tighten x");
    }

    #[tokio::test]
    async fn revise_without_auth_works() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("v2")))
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent(&server);
        let out = agent
            .revise(
                "v1",
                &Critique {
                    feedback: "more detail".to_owned(),
                },
            )
            .await
            .unwrap();
        assert_eq!(out.content, "v2");
    }

    // --- optional bearer -------------------------------------------------

    #[tokio::test]
    async fn with_bearer_sends_authorization_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer proxy-token-xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("ok")))
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent_with_bearer(&server, "proxy-token-xyz");
        agent.generate(draft()).await.unwrap();
    }

    // --- request body shape ---------------------------------------------

    #[tokio::test]
    async fn request_body_targets_configured_model_and_max_tokens() {
        use wiremock::matchers::body_string_contains;

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains(r#""model":"Qwen/Qwen3-0.6B""#))
            .and(body_string_contains(r#""role":"system""#))
            .and(body_string_contains(r#""role":"user""#))
            .and(body_string_contains(r#""max_tokens":256"#))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("ok")))
            .expect(1)
            .mount(&server)
            .await;

        test_agent(&server).generate(draft()).await.unwrap();
    }

    // --- error handling -------------------------------------------------

    #[tokio::test]
    async fn unauthorized_maps_to_domain_error() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "vllm: unauthorized"
            }
        ));
    }

    #[tokio::test]
    async fn rate_limit_maps_to_domain_error() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(429))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "vllm: rate-limited"
            }
        ));
    }

    #[tokio::test]
    async fn upstream_5xx_maps_to_domain_error() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "vllm: upstream error"
            }
        ));
    }

    #[tokio::test]
    async fn malformed_body_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "vllm: malformed response body"
            }
        ));
    }

    #[tokio::test]
    async fn empty_choices_are_rejected() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "x",
                "choices": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "vllm: no choices in response"
            }
        ));
    }

    #[tokio::test]
    async fn whitespace_content_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("   ")))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "vllm: empty text content"
            }
        ));
    }

    // Prompt / rubric tests live in `super::prompts` — covered once,
    // owned by the module that holds the shared helpers.
}
