//! Buffered collector for `StreamDeliberation`.
//!
//! MCP stdio is synchronous request/response — there is no
//! progress-notification surface that would let a coding agent
//! consume the stream live. We buffer the entire server stream into
//! a single response, returning every frame in order plus the final
//! winner pulled out of the last `result`-typed frame for caller
//! convenience.

use choreo_mcp_proto::v1 as pb;
use futures::StreamExt;
use serde_json::{json, Value};
use tonic::Streaming;

use super::proto_to_json::{deliberation_result_to_json, deliberation_update_to_json};

/// Collect a `StreamDeliberation` server stream into a single JSON
/// response. Errors mid-stream surface as a tool error; partial
/// frames already seen are NOT returned (rejecting half-baked output
/// is honest — the caller can retry).
pub(crate) async fn collect_stream(
    mut stream: Streaming<pb::StreamDeliberationResponse>,
) -> Result<Value, String> {
    let mut frames: Vec<Value> = Vec::new();
    let mut winner: Option<Value> = None;
    let mut task_id: Option<String> = None;

    while let Some(item) = stream.next().await {
        let response =
            item.map_err(|status| format!("stream item failed: {}", status.message()))?;
        let Some(update) = response.update else {
            continue;
        };

        if task_id.is_none() && !update.task_id.is_empty() {
            task_id = Some(update.task_id.clone());
        }

        // Extract the winner from a `result` payload before the
        // value moves into `deliberation_update_to_json`.
        if let Some(pb::deliberation_update::Payload::Result(ref r)) = update.payload {
            winner = Some(deliberation_result_to_json(r.clone()));
        }

        frames.push(deliberation_update_to_json(update));
    }

    Ok(json!({
        "task_id": task_id.unwrap_or_default(),
        "frames": frames,
        "winner": winner.unwrap_or(Value::Null),
    }))
}
