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
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

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
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|err| {
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
            return Err(classify_error(status));
        }

        let parsed: ChatResponse = response.json().await.map_err(|err| {
            warn!(
                op,
                agent_id = self.id.as_str(),
                error = %err,
                "vllm: malformed response body"
            );
            DomainError::InvariantViolated {
                reason: "vllm: malformed response body",
            }
        })?;

        extract_text(parsed).inspect_err(|_| {
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
        let system = system_prompt_generate(self.id.as_str(), self.specialty.as_str());
        let user = user_prompt_generate(&request);
        let content = self.complete(system, user, "generate").await?;
        Ok(Revision { content })
    }

    async fn critique(
        &self,
        peer_content: &str,
        constraints: &TaskConstraints,
    ) -> Result<Critique, DomainError> {
        let system = system_prompt_critique(self.id.as_str(), self.specialty.as_str());
        let user = user_prompt_critique(peer_content, constraints);
        let feedback = self.complete(system, user, "critique").await?;
        Ok(Critique { feedback })
    }

    async fn revise(
        &self,
        own_content: &str,
        critique: &Critique,
    ) -> Result<Revision, DomainError> {
        let system = system_prompt_revise(self.id.as_str(), self.specialty.as_str());
        let user = user_prompt_revise(own_content, critique);
        let content = self.complete(system, user, "revise").await?;
        Ok(Revision { content })
    }
}

// ---------------------------------------------------------------------------
// Prompts (domain-agnostic, identical in shape to openai / anthropic)
//
// Deliberately duplicated across the three providers today. A follow-up
// refactor slice extracts them into `agents::prompts` once the
// divergence pressure proves stable to zero — keeping them local keeps
// each provider PR reviewable in isolation.
// ---------------------------------------------------------------------------

fn system_prompt_generate(id: &str, specialty: &str) -> String {
    format!(
        "You are a specialist agent in the Underpass Choreographer.\n\
         Your agent id is \"{id}\". Your specialty is \"{specialty}\".\n\
         \n\
         Role:\n\
         - Propose a solution to the task, within your specialty.\n\
         - Keep the proposal concrete, concise, and self-contained.\n\
         - Do not claim capabilities you lack, do not invent facts.\n\
         \n\
         Output contract:\n\
         - Answer only with the proposal body. No preamble, no signature."
    )
}

fn system_prompt_critique(id: &str, specialty: &str) -> String {
    format!(
        "You are a specialist agent in the Underpass Choreographer.\n\
         Your agent id is \"{id}\". Your specialty is \"{specialty}\".\n\
         \n\
         Role:\n\
         - Critique a peer's proposal for this task.\n\
         - Flag concrete weaknesses; do not restate the proposal.\n\
         - Prioritise critique that the peer can act on in a revision.\n\
         \n\
         Output contract:\n\
         - Answer only with the critique body. No preamble."
    )
}

fn system_prompt_revise(id: &str, specialty: &str) -> String {
    format!(
        "You are a specialist agent in the Underpass Choreographer.\n\
         Your agent id is \"{id}\". Your specialty is \"{specialty}\".\n\
         \n\
         Role:\n\
         - Revise your own proposal in response to the supplied critique.\n\
         - Address the concrete points raised; keep what already works.\n\
         \n\
         Output contract:\n\
         - Answer only with the revised proposal body. No preamble."
    )
}

fn user_prompt_generate(request: &DraftRequest) -> String {
    let rubric = serialize_rubric(&request.constraints);
    let diverse_note = if request.diverse {
        "You are one of several peers; propose a distinctive angle rather than a safest-seeming default."
    } else {
        "Propose the option you judge best on the merits."
    };
    format!(
        "Task:\n{task}\n\n\
         Rubric (opaque constraints to apply):\n{rubric}\n\n\
         {diverse_note}\n\n\
         Produce your proposal now.",
        task = request.task.as_str(),
    )
}

fn user_prompt_critique(peer_content: &str, constraints: &TaskConstraints) -> String {
    let rubric = serialize_rubric(constraints);
    format!(
        "Peer proposal to critique:\n---\n{peer_content}\n---\n\n\
         Rubric (opaque constraints to apply):\n{rubric}\n\n\
         Critique it now."
    )
}

fn user_prompt_revise(own_content: &str, critique: &Critique) -> String {
    format!(
        "Your previous proposal:\n---\n{own_content}\n---\n\n\
         Critique to address:\n---\n{feedback}\n---\n\n\
         Produce the revised proposal now.",
        feedback = critique.feedback,
    )
}

fn serialize_rubric(constraints: &TaskConstraints) -> String {
    let rubric = constraints.rubric();
    if rubric.is_empty() {
        "(empty)".to_owned()
    } else {
        serde_json::to_string_pretty(rubric.as_map())
            .unwrap_or_else(|_| "(unrepresentable)".to_owned())
    }
}

// ---------------------------------------------------------------------------
// Wire types (identical to openai::ChatRequest / ChatResponse)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<ChatMessage<'a>>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatResponseMessage>,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

fn extract_text(resp: ChatResponse) -> Result<String, DomainError> {
    let first = resp
        .choices
        .into_iter()
        .next()
        .ok_or(DomainError::InvariantViolated {
            reason: "vllm: no choices in response",
        })?;
    let content = first
        .message
        .and_then(|m| m.content)
        .ok_or(DomainError::InvariantViolated {
            reason: "vllm: choice has no message.content",
        })?;
    if content.trim().is_empty() {
        return Err(DomainError::InvariantViolated {
            reason: "vllm: empty text content",
        });
    }
    Ok(content)
}

fn classify_error(status: StatusCode) -> DomainError {
    match status.as_u16() {
        401 | 403 => DomainError::InvariantViolated {
            reason: "vllm: unauthorized",
        },
        429 => DomainError::InvariantViolated {
            reason: "vllm: rate-limited",
        },
        400..=499 => DomainError::InvariantViolated {
            reason: "vllm: bad request",
        },
        _ => DomainError::InvariantViolated {
            reason: "vllm: upstream error",
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

    // --- prompt assembly -----------------------------------------------

    #[test]
    fn system_prompt_is_domain_agnostic() {
        let s = system_prompt_generate("a1", "triage");
        assert!(s.contains("a1"));
        assert!(s.contains("triage"));
        for forbidden in ["story", "backlog", "sprint", "pull request"] {
            assert!(
                !s.to_lowercase().contains(forbidden),
                "domain vocabulary leak: {forbidden}"
            );
        }
    }
}
