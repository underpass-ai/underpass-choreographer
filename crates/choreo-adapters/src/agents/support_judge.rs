//! LLM-backed evidence-support judge.
//!
//! Implements [`EvidenceSupportJudgePort`] over the same OpenAI/vLLM
//! Chat Completions wire shape as the provider agents (reusing
//! [`super::openai_compat`]), mirroring [`super::judge`]'s posture on
//! endpoints, metrics and error classification.
//!
//! The judge answers exactly one question per call: *does this claim's
//! cited evidence, on its own, support the claim?* It never sees
//! evidence the claim did not cite, and it never rewrites anything —
//! its verdict (supported / confidence / rationale) is returned to the
//! `claims-evidence-supported` validator, which applies the contract's
//! deterministic threshold and records the verdict in the decision
//! record.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    EvidenceExcerpt, EvidenceSupportJudgePort, MetricsRecorderPort, SupportVerdict,
};
use choreo_core::value_objects::{DurationMs, LlmErrorKind, TokenUsage};
use reqwest::Client;
use serde_json::Value;
use tracing::warn;

use super::openai_compat::{self as wire, ChatMessage, ChatRequest, ChatResponse, ErrorStrings};

const SUPPORT_JUDGE_ERRORS: ErrorStrings = ErrorStrings {
    unauthorized: "support-judge: unauthorized",
    rate_limited: "support-judge: rate-limited",
    bad_request: "support-judge: bad request",
    upstream_error: "support-judge: upstream error",
    malformed_body: "support-judge: malformed response body",
    no_choices: "support-judge: no choices in response",
    missing_content: "support-judge: choice has no message.content",
    empty_content: "support-judge: empty text content",
};

const DEFAULT_MAX_TOKENS: u32 = 256;
const DEFAULT_TIMEOUT: Duration = Duration::from_mins(1);

const SYSTEM_PROMPT: &str = "You are a strict auditor of evidence. You are given one claim and \
the only evidence excerpts the claim cited. Judge solely whether those excerpts, on their own, \
support the claim as stated. Ignore style, plausibility, and outside knowledge: evidence that \
merely relates to the topic without establishing the claim does NOT support it. You never \
rewrite the claim — you only judge it.";

/// An LLM-backed implementation of [`EvidenceSupportJudgePort`].
pub struct LlmEvidenceSupportJudge {
    endpoint: String,
    model: String,
    max_tokens: u32,
    http: Client,
    metrics: Arc<dyn MetricsRecorderPort>,
}

impl fmt::Debug for LlmEvidenceSupportJudge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlmEvidenceSupportJudge")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .finish()
    }
}

impl LlmEvidenceSupportJudge {
    /// Build a support judge against an OpenAI/vLLM-compatible
    /// `endpoint` serving `model`.
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        metrics: Arc<dyn MetricsRecorderPort>,
    ) -> Result<Self, DomainError> {
        let endpoint =
            super::endpoint::validate_provider_endpoint("support_judge.endpoint", endpoint)?;
        let model = model.into().trim().to_owned();
        if model.is_empty() {
            return Err(DomainError::EmptyField {
                field: "support_judge.model",
            });
        }
        let http = build_client(DEFAULT_TIMEOUT)?;
        Ok(Self {
            endpoint,
            model,
            max_tokens: DEFAULT_MAX_TOKENS,
            http,
            metrics,
        })
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Result<Self, DomainError> {
        if max_tokens == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "support_judge.max_tokens",
            });
        }
        self.max_tokens = max_tokens;
        Ok(self)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, DomainError> {
        self.http = build_client(timeout)?;
        Ok(self)
    }

    async fn assess_inner(
        &self,
        claim_text: &str,
        evidence: &[EvidenceExcerpt],
    ) -> Result<SupportVerdict, DomainError> {
        use std::fmt::Write as _;
        let mut excerpts = String::new();
        for excerpt in evidence {
            // Writing to a String is infallible; ignore the Ok.
            let _ = writeln!(excerpts, "[{}] {}", excerpt.reference, excerpt.body);
        }
        let user = format!(
            "CLAIM:\n{claim_text}\n\nCITED EVIDENCE (the only admissible support):\n{excerpts}\n\
             Does the cited evidence, on its own, support the claim? Respond with ONLY a JSON \
             object: {{\"supported\": true|false, \"confidence\": <integer 0-100>, \"reason\": \
             \"<one short sentence>\"}}. Do not wrap it in markdown."
        );
        let body = ChatRequest {
            model: &self.model,
            max_tokens: self.max_tokens,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: SYSTEM_PROMPT.to_owned(),
                },
                ChatMessage {
                    role: "user",
                    content: user,
                },
            ],
        };
        let url = format!(
            "{}/v1/chat/completions",
            self.endpoint.trim_end_matches('/')
        );

        let response = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|err| {
                let kind = if err.is_timeout() {
                    LlmErrorKind::Timeout
                } else {
                    LlmErrorKind::Transport
                };
                self.metrics.record_judge_error(&self.model, kind);
                warn!(error = %err, "support-judge: request failed");
                DomainError::InvariantViolated {
                    reason: "support-judge: request failed",
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            self.metrics
                .record_judge_error(&self.model, LlmErrorKind::from_status(status.as_u16()));
            return Err(wire::classify_error(status, &SUPPORT_JUDGE_ERRORS));
        }

        let parsed: ChatResponse = response.json().await.map_err(|err| {
            self.metrics
                .record_judge_error(&self.model, LlmErrorKind::MalformedBody);
            warn!(error = %err, "support-judge: malformed response body");
            DomainError::InvariantViolated {
                reason: SUPPORT_JUDGE_ERRORS.malformed_body,
            }
        })?;
        let usage = parsed.usage;
        let text = wire::extract_text(parsed, &SUPPORT_JUDGE_ERRORS).inspect_err(|_| {
            self.metrics
                .record_judge_error(&self.model, LlmErrorKind::EmptyContent);
        })?;
        if let Some(usage) = usage {
            self.metrics.record_judge_tokens(
                &self.model,
                TokenUsage::new(usage.prompt_tokens, usage.completion_tokens),
            );
        }
        parse_verdict(&text).inspect_err(|_| {
            self.metrics
                .record_judge_error(&self.model, LlmErrorKind::MalformedBody);
        })
    }
}

#[async_trait]
impl EvidenceSupportJudgePort for LlmEvidenceSupportJudge {
    async fn assess(
        &self,
        claim_text: &str,
        evidence: &[EvidenceExcerpt],
    ) -> Result<SupportVerdict, DomainError> {
        let started = Instant::now();
        let outcome = self.assess_inner(claim_text, evidence).await;
        self.metrics
            .observe_judge_latency(&self.model, elapsed_ms(started));
        outcome
    }
}

fn elapsed_ms(started: Instant) -> DurationMs {
    DurationMs::from_millis(started.elapsed().as_millis() as u64)
}

fn build_client(timeout: Duration) -> Result<Client, DomainError> {
    Client::builder().timeout(timeout).build().map_err(|err| {
        warn!(error = %err, "support-judge: failed to build http client");
        DomainError::InvariantViolated {
            reason: "support-judge: failed to build http client",
        }
    })
}

/// Parse the judge's reply (a `{"supported": bool, "confidence": 0-100,
/// "reason": "…"}` object, possibly surrounded by prose or markdown
/// fences) into a [`SupportVerdict`].
fn parse_verdict(text: &str) -> Result<SupportVerdict, DomainError> {
    let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) else {
        return Err(DomainError::InvariantViolated {
            reason: "support-judge: reply is not a JSON object",
        });
    };
    if end <= start {
        return Err(DomainError::InvariantViolated {
            reason: "support-judge: reply is not a JSON object",
        });
    }
    let value: Value =
        serde_json::from_str(&text[start..=end]).map_err(|_| DomainError::InvariantViolated {
            reason: "support-judge: reply JSON is malformed",
        })?;
    let supported =
        value
            .get("supported")
            .and_then(Value::as_bool)
            .ok_or(DomainError::InvariantViolated {
                reason: "support-judge: reply has no boolean `supported`",
            })?;
    let confidence =
        value
            .get("confidence")
            .and_then(Value::as_f64)
            .ok_or(DomainError::InvariantViolated {
                reason: "support-judge: reply has no numeric `confidence`",
            })?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let confidence = confidence.clamp(0.0, 100.0).round() as u8;
    let rationale = value
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Ok(SupportVerdict {
        supported,
        confidence,
        rationale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::ports::NoopMetricsRecorder;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn metrics() -> Arc<dyn MetricsRecorderPort> {
        Arc::new(NoopMetricsRecorder)
    }

    fn judge(server: &MockServer) -> LlmEvidenceSupportJudge {
        LlmEvidenceSupportJudge::new(server.uri(), "test-model", metrics())
            .unwrap()
            .with_timeout(Duration::from_secs(5))
            .unwrap()
    }

    fn excerpt() -> Vec<EvidenceExcerpt> {
        vec![EvidenceExcerpt {
            reference: "ev-1".to_owned(),
            body: "journalctl: typha (pid 4830) holds 0.0.0.0:5473".to_owned(),
        }]
    }

    fn chat_response(text: &str) -> serde_json::Value {
        json!({
            "id": "cmpl-test",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": text},
                "finish_reason": "stop"
            }]
        })
    }

    #[test]
    fn parse_verdict_handles_plain_json() {
        let verdict =
            parse_verdict(r#"{"supported": true, "confidence": 90, "reason": "matches"}"#).unwrap();
        assert!(verdict.supported);
        assert_eq!(verdict.confidence, 90);
        assert_eq!(verdict.rationale, "matches");
    }

    #[test]
    fn parse_verdict_handles_markdown_fences_and_prose() {
        let reply = "Sure!\n```json\n{\"supported\": false, \"confidence\": 80, \"reason\": \"unrelated\"}\n```";
        let verdict = parse_verdict(reply).unwrap();
        assert!(!verdict.supported);
        assert_eq!(verdict.confidence, 80);
    }

    #[test]
    fn parse_verdict_clamps_confidence() {
        assert_eq!(
            parse_verdict(r#"{"supported": true, "confidence": 130}"#)
                .unwrap()
                .confidence,
            100
        );
    }

    #[test]
    fn parse_verdict_rejects_missing_fields() {
        assert!(parse_verdict(r#"{"confidence": 90}"#).is_err());
        assert!(parse_verdict(r#"{"supported": true}"#).is_err());
        assert!(parse_verdict("no json here").is_err());
    }

    #[tokio::test]
    async fn assess_returns_the_parsed_verdict() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(
                r#"{"supported": true, "confidence": 88, "reason": "the excerpt states it"}"#,
            )))
            .expect(1)
            .mount(&server)
            .await;

        let verdict = judge(&server)
            .assess("typha holds the port", &excerpt())
            .await
            .unwrap();
        assert!(verdict.supported);
        assert_eq!(verdict.confidence, 88);
        assert_eq!(verdict.rationale, "the excerpt states it");
    }

    #[tokio::test]
    async fn upstream_error_is_classified() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let err = judge(&server)
            .assess("anything", &excerpt())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "support-judge: rate-limited"
            }
        ));
    }

    #[tokio::test]
    async fn unparseable_reply_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(chat_response("the evidence looks fine to me")),
            )
            .mount(&server)
            .await;

        let err = judge(&server)
            .assess("anything", &excerpt())
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn blank_model_is_rejected() {
        assert!(matches!(
            LlmEvidenceSupportJudge::new("http://x", "  ", metrics()).unwrap_err(),
            DomainError::EmptyField {
                field: "support_judge.model"
            }
        ));
    }
}
