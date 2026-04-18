//! Shared wire types and helpers for providers that speak the
//! OpenAI Chat Completions shape (`POST /v1/chat/completions`).
//!
//! Two adapters live on top of this module today: `openai` (with
//! mandatory bearer auth) and `vllm` (with optional bearer auth and
//! different defaults). The wire format is identical for both; the
//! difference is policy and configuration, which each adapter owns.
//!
//! Domain errors carry a `&'static str` reason, so the provider
//! prefix cannot be formatted at runtime. Each adapter passes its
//! own [`ErrorStrings`] table so the correct prefix lands in the
//! error without requiring `String` allocations anywhere in the
//! error path.

use choreo_core::error::DomainError;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

/// Per-provider static error reasons.
///
/// Every adapter owns a `const` instance of this table and threads
/// it through [`classify_error`] and [`extract_text`]. The constant
/// makes it impossible to format a provider prefix at runtime —
/// which is what keeps `DomainError::InvariantViolated { reason }`
/// a `&'static str` across the board.
pub(super) struct ErrorStrings {
    pub unauthorized: &'static str,
    pub rate_limited: &'static str,
    pub bad_request: &'static str,
    pub upstream_error: &'static str,
    pub malformed_body: &'static str,
    pub no_choices: &'static str,
    pub missing_content: &'static str,
    pub empty_content: &'static str,
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub(super) struct ChatRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    pub messages: Vec<ChatMessage<'a>>,
}

#[derive(Serialize)]
pub(super) struct ChatMessage<'a> {
    pub role: &'a str,
    pub content: String,
}

#[derive(Deserialize)]
pub(super) struct ChatResponse {
    #[serde(default)]
    pub choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
pub(super) struct ChatChoice {
    #[serde(default)]
    pub message: Option<ChatResponseMessage>,
}

#[derive(Deserialize)]
pub(super) struct ChatResponseMessage {
    #[serde(default)]
    pub content: Option<String>,
    /// Qwen3 (and other reasoning-parser-enabled models) split their
    /// output between `content` (the final answer) and `reasoning`
    /// (the chain-of-thought). When the token budget is consumed by
    /// reasoning and `content` comes back null, we still have the
    /// reasoning text — fall back to it rather than erroring, so the
    /// deliberation keeps working against reasoning-configured vLLM
    /// deployments.
    #[serde(default)]
    pub reasoning: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the first choice's text content. Rejects missing /
/// empty / whitespace-only responses with provider-tagged errors.
///
/// Falls back to the reasoning trace when the primary `content`
/// comes back null/empty but the response carries a non-empty
/// `reasoning` (Qwen3 and similar reasoning-parser-enabled models
/// with a short token budget). We log the fallback at debug-level
/// from the caller — the signal is still domain-meaningful text,
/// it just arrives through a different wire field.
pub(super) fn extract_text(resp: ChatResponse, errs: &ErrorStrings) -> Result<String, DomainError> {
    let first = resp
        .choices
        .into_iter()
        .next()
        .ok_or(DomainError::InvariantViolated {
            reason: errs.no_choices,
        })?;
    let message = first.message.ok_or(DomainError::InvariantViolated {
        reason: errs.missing_content,
    })?;

    let primary = message.content.and_then(|c| {
        let trimmed = c.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(c)
        }
    });

    if let Some(text) = primary {
        return Ok(text);
    }

    // Fallback: reasoning field.
    if let Some(reasoning) = message.reasoning {
        let trimmed = reasoning.trim();
        if !trimmed.is_empty() {
            return Ok(reasoning);
        }
    }

    // Neither `content` nor `reasoning` usable. Distinguish the two
    // upstream shapes: "field absent" vs "field present but empty" —
    // both are operational bugs but the diagnostic reason differs.
    Err(DomainError::InvariantViolated {
        reason: errs.empty_content,
    })
}

/// Classify an HTTP response status into a provider-tagged domain
/// error. The status categories match both OpenAI and vLLM's
/// observed behaviour.
pub(super) fn classify_error(status: StatusCode, errs: &ErrorStrings) -> DomainError {
    match status.as_u16() {
        401 | 403 => DomainError::InvariantViolated {
            reason: errs.unauthorized,
        },
        429 => DomainError::InvariantViolated {
            reason: errs.rate_limited,
        },
        400..=499 => DomainError::InvariantViolated {
            reason: errs.bad_request,
        },
        _ => DomainError::InvariantViolated {
            reason: errs.upstream_error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_ERRS: ErrorStrings = ErrorStrings {
        unauthorized: "test: unauthorized",
        rate_limited: "test: rate-limited",
        bad_request: "test: bad request",
        upstream_error: "test: upstream error",
        malformed_body: "test: malformed body",
        no_choices: "test: no choices",
        missing_content: "test: missing content",
        empty_content: "test: empty content",
    };

    fn parse_response(value: serde_json::Value) -> ChatResponse {
        serde_json::from_value(value).expect("valid ChatResponse json")
    }

    #[test]
    fn extract_text_returns_content_on_happy_path() {
        let resp = parse_response(json!({
            "choices": [
                {"message": {"role": "assistant", "content": "hello"}}
            ]
        }));
        assert_eq!(extract_text(resp, &TEST_ERRS).unwrap(), "hello");
    }

    #[test]
    fn extract_text_rejects_empty_choices() {
        let resp = parse_response(json!({"choices": []}));
        let err = extract_text(resp, &TEST_ERRS).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "test: no choices"
            }
        ));
    }

    #[test]
    fn extract_text_rejects_missing_message() {
        let resp = parse_response(json!({
            "choices": [{}]
        }));
        let err = extract_text(resp, &TEST_ERRS).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "test: missing content"
            }
        ));
    }

    #[test]
    fn extract_text_rejects_missing_content_without_reasoning() {
        let resp = parse_response(json!({
            "choices": [{"message": {"role": "assistant"}}]
        }));
        let err = extract_text(resp, &TEST_ERRS).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "test: empty content"
            }
        ));
    }

    #[test]
    fn extract_text_rejects_whitespace_content_without_reasoning() {
        let resp = parse_response(json!({
            "choices": [{"message": {"role": "assistant", "content": "   \n\t"}}]
        }));
        let err = extract_text(resp, &TEST_ERRS).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "test: empty content"
            }
        ));
    }

    #[test]
    fn extract_text_falls_back_to_reasoning_when_content_is_null() {
        // Real-world shape produced by Qwen3 with --reasoning-parser=qwen3
        // when `max_tokens` was spent inside the reasoning phase and
        // `content` never materialised on the wire.
        let resp = parse_response(json!({
            "choices": [{"message": {
                "role": "assistant",
                "content": null,
                "reasoning": "Thinking Process:\n1. Analyze..."
            }}]
        }));
        let text = extract_text(resp, &TEST_ERRS).unwrap();
        assert!(
            text.starts_with("Thinking"),
            "reasoning fallback not used: got {text:?}"
        );
    }

    #[test]
    fn extract_text_prefers_content_over_reasoning_when_both_present() {
        let resp = parse_response(json!({
            "choices": [{"message": {
                "role": "assistant",
                "content": "primary answer",
                "reasoning": "should not be picked"
            }}]
        }));
        assert_eq!(extract_text(resp, &TEST_ERRS).unwrap(), "primary answer");
    }

    #[test]
    fn extract_text_rejects_content_and_reasoning_both_empty() {
        let resp = parse_response(json!({
            "choices": [{"message": {
                "role": "assistant",
                "content": "",
                "reasoning": "   "
            }}]
        }));
        let err = extract_text(resp, &TEST_ERRS).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "test: empty content"
            }
        ));
    }

    #[test]
    fn classify_error_maps_401_to_unauthorized() {
        assert!(matches!(
            classify_error(StatusCode::from_u16(401).unwrap(), &TEST_ERRS),
            DomainError::InvariantViolated {
                reason: "test: unauthorized"
            }
        ));
    }

    #[test]
    fn classify_error_maps_403_to_unauthorized() {
        assert!(matches!(
            classify_error(StatusCode::from_u16(403).unwrap(), &TEST_ERRS),
            DomainError::InvariantViolated {
                reason: "test: unauthorized"
            }
        ));
    }

    #[test]
    fn classify_error_maps_429_to_rate_limited() {
        assert!(matches!(
            classify_error(StatusCode::TOO_MANY_REQUESTS, &TEST_ERRS),
            DomainError::InvariantViolated {
                reason: "test: rate-limited"
            }
        ));
    }

    #[test]
    fn classify_error_maps_other_4xx_to_bad_request() {
        for code in [400u16, 404, 422] {
            let status = StatusCode::from_u16(code).unwrap();
            assert!(
                matches!(
                    classify_error(status, &TEST_ERRS),
                    DomainError::InvariantViolated {
                        reason: "test: bad request"
                    }
                ),
                "unexpected mapping for {code}",
            );
        }
    }

    #[test]
    fn classify_error_maps_5xx_to_upstream() {
        for code in [500u16, 502, 503] {
            let status = StatusCode::from_u16(code).unwrap();
            assert!(
                matches!(
                    classify_error(status, &TEST_ERRS),
                    DomainError::InvariantViolated {
                        reason: "test: upstream error"
                    }
                ),
                "unexpected mapping for {code}",
            );
        }
    }

    /// `malformed_body` is carried by `ErrorStrings` but used by the
    /// adapters when `response.json()` itself fails (before reaching
    /// `extract_text`). Reference it here so the constant is not
    /// flagged as unused under feature combinations that exclude
    /// tests.
    #[test]
    fn error_strings_expose_malformed_body_label() {
        assert_eq!(TEST_ERRS.malformed_body, "test: malformed body");
    }
}
