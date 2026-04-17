//! OpenAI Chat Completions API adapter.
//!
//! Implements [`AgentPort`] against
//! `POST https://api.openai.com/v1/chat/completions`. The body shape
//! is the broadly-compatible Chat Completions format, so the same
//! code also works against Azure OpenAI and most OpenAI-compatible
//! servers (vLLM, llama.cpp, …) — configure a different
//! `endpoint` + `model`.
//!
//! **Provider-agnostic by convention.** The system prompts speak
//! only the Choreographer's vocabulary (agent, specialty, task,
//! proposal, critique, revision). Domain-specific context flows
//! through `TaskConstraints.rubric` which is passed through
//! verbatim in the user message.
//!
//! **Secrets are masked.** [`OpenAiApiKey`] implements `Debug` with
//! a fixed redaction so an accidental `dbg!` or `tracing` field
//! never leaks the credential.

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

const DEFAULT_ENDPOINT: &str = "https://api.openai.com";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_MAX_TOKENS: u32 = 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Secret wrapper
// ---------------------------------------------------------------------------

/// Opaque API key. Its `Debug` impl is a fixed redaction so the
/// secret value cannot slip into logs, event payloads, or test
/// snapshots.
#[derive(Clone)]
pub struct OpenAiApiKey(String);

impl OpenAiApiKey {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = raw.into().trim().to_owned();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "openai.api_key",
            });
        }
        Ok(Self(trimmed))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OpenAiApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OpenAiApiKey(**redacted**)")
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Static configuration for the OpenAI adapter. Every field is
/// validated on construction.
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    api_key: OpenAiApiKey,
    endpoint: String,
    model: String,
    max_tokens: u32,
    timeout: Duration,
}

impl OpenAiConfig {
    #[must_use]
    pub fn new(api_key: OpenAiApiKey) -> Self {
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
                field: "openai.endpoint",
            });
        }
        self.endpoint = value;
        Ok(self)
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Result<Self, DomainError> {
        let value = model.into().trim().to_owned();
        if value.is_empty() {
            return Err(DomainError::EmptyField {
                field: "openai.model",
            });
        }
        self.model = value;
        Ok(self)
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Result<Self, DomainError> {
        if max_tokens == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "openai.max_tokens",
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

/// OpenAI-backed agent. The underlying [`reqwest::Client`] keeps a
/// connection pool across the three `AgentPort` methods.
pub struct OpenAiAgent {
    id: AgentId,
    specialty: Specialty,
    config: OpenAiConfig,
    http: Client,
}

impl fmt::Debug for OpenAiAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiAgent")
            .field("id", &self.id)
            .field("specialty", &self.specialty)
            .field("config", &self.config)
            .finish()
    }
}

impl OpenAiAgent {
    pub fn new(
        id: AgentId,
        specialty: Specialty,
        config: OpenAiConfig,
    ) -> Result<Self, DomainError> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|err| {
                debug!(error = %err, "openai: failed to build http client");
                DomainError::InvariantViolated {
                    reason: "openai: failed to build http client",
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
        let response = self
            .http
            .post(&url)
            .bearer_auth(self.config.api_key.expose())
            .json(&body)
            .send()
            .await
            .map_err(|err| {
                warn!(
                    op,
                    agent_id = self.id.as_str(),
                    error = %err,
                    "openai: request failed"
                );
                DomainError::InvariantViolated {
                    reason: "openai: request failed",
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
                "openai: upstream returned non-success"
            );
            return Err(classify_error(status));
        }

        let parsed: ChatResponse = response.json().await.map_err(|err| {
            warn!(
                op,
                agent_id = self.id.as_str(),
                error = %err,
                "openai: malformed response body"
            );
            DomainError::InvariantViolated {
                reason: "openai: malformed response body",
            }
        })?;

        extract_text(parsed).inspect_err(|_| {
            warn!(
                op,
                agent_id = self.id.as_str(),
                "openai: empty text content"
            );
        })
    }
}

#[async_trait]
impl AgentPort for OpenAiAgent {
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
// Prompts (domain-agnostic; identical to the Anthropic adapter's set
// on purpose — both providers see the same Choreographer voice)
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
// Wire types
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
            reason: "openai: no choices in response",
        })?;
    let content = first
        .message
        .and_then(|m| m.content)
        .ok_or(DomainError::InvariantViolated {
            reason: "openai: choice has no message.content",
        })?;
    if content.trim().is_empty() {
        return Err(DomainError::InvariantViolated {
            reason: "openai: empty text content",
        });
    }
    Ok(content)
}

fn classify_error(status: StatusCode) -> DomainError {
    match status.as_u16() {
        401 | 403 => DomainError::InvariantViolated {
            reason: "openai: unauthorized",
        },
        429 => DomainError::InvariantViolated {
            reason: "openai: rate-limited",
        },
        400..=499 => DomainError::InvariantViolated {
            reason: "openai: bad request",
        },
        _ => DomainError::InvariantViolated {
            reason: "openai: upstream error",
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

    fn test_agent(server: &MockServer) -> OpenAiAgent {
        let config = OpenAiConfig::new(OpenAiApiKey::new("sk-test-12345").unwrap())
            .with_endpoint(server.uri())
            .unwrap()
            .with_model("gpt-4o-mini")
            .unwrap()
            .with_max_tokens(256)
            .unwrap()
            .with_timeout(Duration::from_secs(5));
        OpenAiAgent::new(
            AgentId::new("openai-1").unwrap(),
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
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "created": 1_700_000_000,
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": text},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 12, "total_tokens": 22}
        })
    }

    // --- secret / debug -------------------------------------------------

    #[test]
    fn api_key_debug_is_redacted() {
        let k = OpenAiApiKey::new("sk-very-secret-123").unwrap();
        let shown = format!("{k:?}");
        assert!(!shown.contains("sk-very"), "api key leaked: {shown}");
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn empty_api_key_rejected() {
        assert!(matches!(
            OpenAiApiKey::new("   ").unwrap_err(),
            DomainError::EmptyField {
                field: "openai.api_key"
            }
        ));
    }

    #[test]
    fn agent_debug_does_not_leak_secret() {
        let cfg = OpenAiConfig::new(OpenAiApiKey::new("sk-shhh").unwrap());
        let agent = OpenAiAgent::new(
            AgentId::new("a").unwrap(),
            Specialty::new("triage").unwrap(),
            cfg,
        )
        .unwrap();
        let shown = format!("{agent:?}");
        assert!(!shown.contains("sk-shhh"));
        assert!(shown.contains("redacted"));
    }

    // --- config validation ---------------------------------------------

    #[test]
    fn empty_endpoint_rejected() {
        let cfg = OpenAiConfig::new(OpenAiApiKey::new("k").unwrap());
        assert!(matches!(
            cfg.with_endpoint("  ").unwrap_err(),
            DomainError::EmptyField {
                field: "openai.endpoint"
            }
        ));
    }

    #[test]
    fn zero_max_tokens_rejected() {
        let cfg = OpenAiConfig::new(OpenAiApiKey::new("k").unwrap());
        assert!(matches!(
            cfg.with_max_tokens(0).unwrap_err(),
            DomainError::MustBeNonZero {
                field: "openai.max_tokens"
            }
        ));
    }

    // --- happy path -----------------------------------------------------

    #[tokio::test]
    async fn generate_hits_chat_completions_and_returns_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test-12345"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("a proposal.")))
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent(&server);
        let out = agent.generate(draft()).await.unwrap();
        assert_eq!(out.content, "a proposal.");
    }

    #[tokio::test]
    async fn critique_returns_feedback() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response("consider edge case X")),
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
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response("better proposal.")),
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

    // --- request body shape --------------------------------------------

    #[tokio::test]
    async fn request_body_contains_system_and_user_messages_with_expected_model() {
        use wiremock::matchers::body_string_contains;

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains(r#""model":"gpt-4o-mini""#))
            .and(body_string_contains(r#""role":"system""#))
            .and(body_string_contains(r#""role":"user""#))
            .and(body_string_contains(r#""max_tokens":256"#))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("ok")))
            .expect(1)
            .mount(&server)
            .await;

        let agent = test_agent(&server);
        agent.generate(draft()).await.unwrap();
    }

    // --- error handling -------------------------------------------------

    #[tokio::test]
    async fn unauthorized_status_maps_to_domain_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("invalid key"))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "openai: unauthorized"
            }
        ));
    }

    #[tokio::test]
    async fn rate_limit_status_maps_to_domain_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "openai: rate-limited"
            }
        ));
    }

    #[tokio::test]
    async fn upstream_5xx_maps_to_domain_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "openai: upstream error"
            }
        ));
    }

    #[tokio::test]
    async fn malformed_response_body_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "openai: malformed response body"
            }
        ));
    }

    #[tokio::test]
    async fn no_choices_in_response_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-x",
                "object": "chat.completion",
                "choices": [],
                "model": "gpt-4o-mini"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "openai: no choices in response"
            }
        ));
    }

    #[tokio::test]
    async fn choice_with_empty_content_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("   \n\t")))
            .expect(1)
            .mount(&server)
            .await;

        let err = test_agent(&server).generate(draft()).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "openai: empty text content"
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

    #[test]
    fn rubric_serialization_handles_empty() {
        let c = TaskConstraints::default();
        assert_eq!(serialize_rubric(&c), "(empty)");
    }
}
