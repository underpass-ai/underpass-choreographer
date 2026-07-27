//! LLM-judge validator.
//!
//! An LLM-backed [`ValidatorPort`] that rates a proposal's intrinsic
//! quality, so a deliberation's winner is the *strongest* proposal — not
//! an arbitrary one among those that merely pass the structural
//! validators. The numeric verdict (0.0–1.0) is carried in the report's
//! `details` under [`crate::scoring::JUDGE_SCORE_DETAIL_KEY`];
//! [`crate::scoring::JudgeAwareScoring`] reads it to rank proposals.
//!
//! It speaks the same OpenAI/vLLM Chat Completions wire shape as the
//! provider agents, reusing [`super::openai_compat`].

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use choreo_core::entities::{TaskConstraints, ValidatorReport};
use choreo_core::error::DomainError;
use choreo_core::ports::{MetricsRecorderPort, ValidatorPort};
use choreo_core::value_objects::{Attributes, DurationMs, LlmErrorKind, Score, TokenUsage};
use reqwest::Client;
use serde_json::{json, Value};
use tracing::warn;

use super::openai_compat::{self as wire, ChatMessage, ChatRequest, ChatResponse, ErrorStrings};
use crate::scoring::JUDGE_SCORE_DETAIL_KEY;

const JUDGE_ERRORS: ErrorStrings = ErrorStrings {
    unauthorized: "judge: unauthorized",
    rate_limited: "judge: rate-limited",
    bad_request: "judge: bad request",
    upstream_error: "judge: upstream error",
    malformed_body: "judge: malformed response body",
    no_choices: "judge: no choices in response",
    missing_content: "judge: choice has no message.content",
    empty_content: "judge: empty text content",
};

/// `kind` of the report the judge emits.
pub const JUDGE_KIND: &str = "llm_judge";

const DEFAULT_MAX_TOKENS: u32 = 256;
const DEFAULT_TIMEOUT: Duration = Duration::from_mins(1);

const SYSTEM_PROMPT: &str = "You are an impartial, rigorous judge of the proposals produced in a \
multi-agent deliberation. You reward proposals that are specific, internally consistent (no \
contradictions), complete, and actionable; you penalise vagueness, unfilled placeholders, and \
self-contradiction. You never rewrite the proposal — you only rate it.";

/// An LLM-backed quality judge, plugged into the deliberation as a
/// validator.
pub struct LlmJudgeValidator {
    endpoint: String,
    model: String,
    max_tokens: u32,
    threshold: f64,
    http: Client,
    metrics: Arc<dyn MetricsRecorderPort>,
}

impl fmt::Debug for LlmJudgeValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlmJudgeValidator")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("threshold", &self.threshold)
            .finish()
    }
}

impl LlmJudgeValidator {
    /// Build a judge against an OpenAI/vLLM-compatible `endpoint` serving
    /// `model`, passing a proposal when its score reaches `threshold`
    /// (0.0–1.0). The `passed` flag does not drive the ranking — the
    /// numeric score does — but it lets the judge act as a quality gate
    /// too.
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        threshold: f64,
        metrics: Arc<dyn MetricsRecorderPort>,
    ) -> Result<Self, DomainError> {
        let endpoint = super::endpoint::validate_provider_endpoint("judge.endpoint", endpoint)?;
        let model = model.into().trim().to_owned();
        if model.is_empty() {
            return Err(DomainError::EmptyField {
                field: "judge.model",
            });
        }
        if !(0.0..=1.0).contains(&threshold) {
            return Err(DomainError::OutOfRange {
                field: "judge.threshold",
                value: threshold,
                min: 0.0,
                max: 1.0,
            });
        }
        let http = build_client(DEFAULT_TIMEOUT)?;
        Ok(Self {
            endpoint,
            model,
            max_tokens: DEFAULT_MAX_TOKENS,
            threshold,
            http,
            metrics,
        })
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Result<Self, DomainError> {
        if max_tokens == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "judge.max_tokens",
            });
        }
        self.max_tokens = max_tokens;
        Ok(self)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, DomainError> {
        self.http = build_client(timeout)?;
        Ok(self)
    }

    /// Time one rating call and record its latency, success or failure,
    /// before propagating the result. Latency is the leading signal of
    /// the judge approaching its timeout, so it is recorded on every path.
    async fn rate(&self, content: &str) -> Result<f64, DomainError> {
        let started = Instant::now();
        let outcome = self.rate_inner(content).await;
        self.metrics
            .observe_judge_latency(&self.model, elapsed_ms(started));
        outcome
    }

    async fn rate_inner(&self, content: &str) -> Result<f64, DomainError> {
        let user = format!(
            "Rate the following proposal on a 0–100 integer scale (100 = excellent). Respond with \
             ONLY a JSON object: {{\"score\": <integer 0-100>, \"reason\": \"<one short sentence>\"}}. \
             Do not wrap it in markdown.\n\nProposal:\n---\n{content}\n---"
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
                // A connection-level failure before any HTTP status; a
                // deadline overrun is reported distinctly so saturation is
                // separable from a dead endpoint.
                let kind = if err.is_timeout() {
                    LlmErrorKind::Timeout
                } else {
                    LlmErrorKind::Transport
                };
                self.metrics.record_judge_error(&self.model, kind);
                warn!(error = %err, "judge: request failed");
                DomainError::InvariantViolated {
                    reason: "judge: request failed",
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            self.metrics
                .record_judge_error(&self.model, LlmErrorKind::from_status(status.as_u16()));
            return Err(wire::classify_error(status, &JUDGE_ERRORS));
        }

        let parsed: ChatResponse = response.json().await.map_err(|err| {
            self.metrics
                .record_judge_error(&self.model, LlmErrorKind::MalformedBody);
            warn!(error = %err, "judge: malformed response body");
            DomainError::InvariantViolated {
                reason: JUDGE_ERRORS.malformed_body,
            }
        })?;
        let usage = parsed.usage;
        let text = wire::extract_text(parsed, &JUDGE_ERRORS).inspect_err(|_| {
            self.metrics
                .record_judge_error(&self.model, LlmErrorKind::EmptyContent);
        })?;
        if let Some(usage) = usage {
            self.metrics.record_judge_tokens(
                &self.model,
                TokenUsage::new(usage.prompt_tokens, usage.completion_tokens),
            );
        }
        parse_score(&text).inspect_err(|_| {
            // The call succeeded but the judge's reply was not a usable
            // score object — a malformed body at the contract level.
            self.metrics
                .record_judge_error(&self.model, LlmErrorKind::MalformedBody);
        })
    }
}

#[async_trait]
impl ValidatorPort for LlmJudgeValidator {
    fn kind(&self) -> &str {
        JUDGE_KIND
    }

    async fn validate(
        &self,
        proposal_content: &str,
        _constraints: &TaskConstraints,
    ) -> Result<ValidatorReport, DomainError> {
        let score = self.rate(proposal_content).await?;
        // `rate` already clamps to [0.0, 1.0], so this is always valid.
        self.metrics
            .observe_judge_score(&self.model, Score::new(score).unwrap_or(Score::MIN));
        let details = Attributes::new(BTreeMap::from([(
            JUDGE_SCORE_DETAIL_KEY.to_owned(),
            json!(score),
        )]))?;
        ValidatorReport::new(
            JUDGE_KIND,
            score >= self.threshold,
            format!("llm judge score {score:.2}"),
            details,
        )
    }
}

/// Whole-millisecond latency since `started`, as the domain duration type.
fn elapsed_ms(started: Instant) -> DurationMs {
    DurationMs::from_millis(started.elapsed().as_millis() as u64)
}

fn build_client(timeout: Duration) -> Result<Client, DomainError> {
    Client::builder().timeout(timeout).build().map_err(|err| {
        warn!(error = %err, "judge: failed to build http client");
        DomainError::InvariantViolated {
            reason: "judge: failed to build http client",
        }
    })
}

/// Parse the judge's reply (a `{"score": 0-100, ...}` object, possibly
/// surrounded by prose or markdown fences) into a 0.0–1.0 score.
fn parse_score(text: &str) -> Result<f64, DomainError> {
    let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) else {
        return Err(DomainError::InvariantViolated {
            reason: "judge: reply is not a JSON object",
        });
    };
    if end <= start {
        return Err(DomainError::InvariantViolated {
            reason: "judge: reply is not a JSON object",
        });
    }
    let value: Value =
        serde_json::from_str(&text[start..=end]).map_err(|_| DomainError::InvariantViolated {
            reason: "judge: reply JSON is malformed",
        })?;
    let score =
        value
            .get("score")
            .and_then(Value::as_f64)
            .ok_or(DomainError::InvariantViolated {
                reason: "judge: reply has no numeric score",
            })?;
    Ok((score / 100.0).clamp(0.0, 1.0))
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

    /// Captures `record_judge_error` so a test can assert which kind the
    /// judge classified a failure as. Every other method is a no-op.
    #[derive(Default)]
    struct CapturingMetrics {
        errors: std::sync::Mutex<Vec<&'static str>>,
    }
    impl MetricsRecorderPort for CapturingMetrics {
        fn observe_deliberation_duration(
            &self,
            _specialty: &choreo_core::value_objects::Specialty,
            _duration: DurationMs,
        ) {
        }
        fn record_deliberation_outcome(
            &self,
            _specialty: &choreo_core::value_objects::Specialty,
            _outcome: choreo_core::value_objects::DeliberationOutcome,
        ) {
        }
        fn observe_winner_score(
            &self,
            _specialty: &choreo_core::value_objects::Specialty,
            _score: Score,
        ) {
        }
        fn observe_judge_latency(&self, _model: &str, _duration: DurationMs) {}
        fn observe_judge_score(&self, _model: &str, _score: Score) {}
        fn record_judge_error(&self, _model: &str, kind: LlmErrorKind) {
            self.errors.lock().unwrap().push(kind.as_label());
        }
        fn record_provider_error(&self, _provider: &str, _kind: LlmErrorKind) {}
        fn record_judge_tokens(&self, _model: &str, _usage: TokenUsage) {}
        fn record_provider_tokens(&self, _provider: &str, _usage: TokenUsage) {}
        fn observe_provider_request(
            &self,
            _provider: &str,
            _operation: &str,
            _duration: DurationMs,
        ) {
        }
        fn inc_provider_in_flight(&self, _provider: &str) {}
        fn dec_provider_in_flight(&self, _provider: &str) {}
        fn record_discrimination(
            &self,
            _specialty: &choreo_core::value_objects::Specialty,
            _result: choreo_core::value_objects::Discrimination,
        ) {
        }
        fn record_ceremony_outcome(
            &self,
            _ceremony: &str,
            _outcome: choreo_core::value_objects::CeremonyOutcome,
        ) {
        }
        fn observe_ceremony_duration(&self, _ceremony: &str, _duration: DurationMs) {}
        fn observe_ceremony_step_duration(
            &self,
            _ceremony: &str,
            _step: &str,
            _duration: DurationMs,
        ) {
        }
        fn record_ceremony_step(
            &self,
            _ceremony: &str,
            _step: &str,
            _status: choreo_core::value_objects::StepStatus,
        ) {
        }
        fn record_ceremony_transition_blocked(&self, _ceremony: &str, _from_state: &str) {}
        fn observe_nats_publish(&self, _subject_kind: &str, _duration: DurationMs) {}
        fn record_nats_publish_error(&self, _subject_kind: &str, _reason: &str) {}
        fn set_postgres_pool_in_use(&self, _connections: i64) {}
        fn record_scoring_mode(&self, _mode: choreo_core::value_objects::ScoringMode) {}
    }

    fn judge_with(server: &MockServer, metrics: Arc<dyn MetricsRecorderPort>) -> LlmJudgeValidator {
        LlmJudgeValidator::new(server.uri(), "test-model", 0.5, metrics)
            .unwrap()
            .with_timeout(Duration::from_secs(5))
            .unwrap()
    }

    #[test]
    fn parse_score_handles_plain_json() {
        assert!((parse_score(r#"{"score": 80, "reason": "ok"}"#).unwrap() - 0.8).abs() < 1e-9);
    }

    #[test]
    fn parse_score_handles_markdown_fences_and_prose() {
        let reply = "Sure!\n```json\n{\"score\": 95, \"reason\": \"specific\"}\n```";
        assert!((parse_score(reply).unwrap() - 0.95).abs() < 1e-9);
    }

    #[test]
    fn parse_score_clamps_above_range() {
        assert_eq!(parse_score(r#"{"score": 130}"#).unwrap(), 1.0);
    }

    #[test]
    fn parse_score_rejects_non_json() {
        assert!(parse_score("no json here").is_err());
    }

    #[test]
    fn parse_score_rejects_missing_score() {
        assert!(parse_score(r#"{"reason": "x"}"#).is_err());
    }

    #[test]
    fn threshold_out_of_range_is_rejected() {
        assert!(matches!(
            LlmJudgeValidator::new("http://x", "m", 1.5, metrics()).unwrap_err(),
            DomainError::OutOfRange {
                field: "judge.threshold",
                ..
            }
        ));
    }

    fn judge(server: &MockServer) -> LlmJudgeValidator {
        LlmJudgeValidator::new(server.uri(), "test-model", 0.5, metrics())
            .unwrap()
            .with_timeout(Duration::from_secs(5))
            .unwrap()
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

    #[tokio::test]
    async fn validate_carries_the_score_in_details_and_passes_above_threshold() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(
                r#"{"score": 88, "reason": "specific and consistent"}"#,
            )))
            .expect(1)
            .mount(&server)
            .await;

        let report = judge(&server)
            .validate(
                "a detailed, consistent proposal",
                &TaskConstraints::default(),
            )
            .await
            .unwrap();

        assert_eq!(report.kind(), "llm_judge");
        assert!(report.passed());
        assert!(
            (report
                .details()
                .get(JUDGE_SCORE_DETAIL_KEY)
                .and_then(Value::as_f64)
                .unwrap()
                - 0.88)
                .abs()
                < 1e-9
        );
    }

    #[tokio::test]
    async fn rate_limited_response_records_a_rate_limited_judge_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let metrics = Arc::new(CapturingMetrics::default());
        let err = judge_with(&server, metrics.clone())
            .validate("anything", &TaskConstraints::default())
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "judge: rate-limited"
            }
        ));
        assert_eq!(metrics.errors.lock().unwrap().as_slice(), &["rate_limited"]);
    }

    #[tokio::test]
    async fn validate_marks_low_quality_as_not_passed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(
                r#"{"score": 20, "reason": "vague placeholders"}"#,
            )))
            .expect(1)
            .mount(&server)
            .await;

        let report = judge(&server)
            .validate("vague", &TaskConstraints::default())
            .await
            .unwrap();

        assert!(!report.passed());
    }
}
