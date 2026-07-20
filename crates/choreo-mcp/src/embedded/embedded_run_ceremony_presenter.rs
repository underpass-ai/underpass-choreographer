use choreo_adapters::mermaid::CeremonyConversationDiagram;
use choreo_app::usecases::RunCeremonyOutput;
use serde_json::{json, Value};

const WINNER_CONTENT_KEY: &str = "winner_content";

/// Projects an embedded use-case result onto the stable MCP response.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EmbeddedRunCeremonyPresenter;

impl EmbeddedRunCeremonyPresenter {
    #[must_use]
    pub(crate) fn present(output: &RunCeremonyOutput) -> Value {
        let instance = output.instance();
        let definition = output.definition();
        let steps = output
            .step_traces()
            .iter()
            .map(|trace| {
                json!({
                    "state_id": trace.state_id().as_str(),
                    "step_id": trace.step_id().as_str(),
                    "role_id": trace.role_id().as_str(),
                    "status": trace.status().as_label(),
                    "attempt": trace.attempt().get(),
                    "output": trace
                        .output()
                        .attributes()
                        .get(WINNER_CONTENT_KEY)
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                })
            })
            .collect::<Vec<_>>();

        json!({
            "ceremony_id": instance.id().as_str(),
            "definition_name": definition.name().as_str(),
            "definition_version": definition.version().as_str(),
            "final_state": instance.current_state().as_str(),
            "completed": instance.is_completed(definition),
            "steps": steps,
            "mermaid_sequence": CeremonyConversationDiagram::render(definition),
        })
    }
}
