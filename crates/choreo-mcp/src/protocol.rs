//! MCP wire-protocol helpers and tool catalog.
//!
//! Hand-rolled JSON-RPC 2.0 + MCP `tools/*` shapes — no SDK, the
//! adapter owns every byte that crosses stdio so it never drifts from
//! the gRPC contract it wraps.
//!
//! The base catalog is 1:1 with the `underpass.choreo.v1` gRPC
//! service: 17 tools, one per RPC. Backend-specific adapters may add
//! capabilities that have no remote transport equivalent.

use serde_json::{json, Value};

/// MCP protocol version we advertise.
pub(crate) const PROTOCOL_VERSION: &str = "2024-11-05";

pub(crate) const RUN_CEREMONY_TOOL: &str = "choreo_run_ceremony";
pub(crate) const START_CEREMONY_TOOL: &str = "choreo_start_ceremony";
pub(crate) const RUN_CEREMONY_STEP_TOOL: &str = "choreo_run_ceremony_step";
pub(crate) const APPROVE_CEREMONY_GUARD_TOOL: &str = "choreo_approve_ceremony_guard";
pub(crate) const DEFER_CEREMONY_GUARD_TOOL: &str = "choreo_defer_ceremony_guard";
pub(crate) const APPLY_CEREMONY_TRANSITION_TOOL: &str = "choreo_apply_ceremony_transition";
pub(crate) const ASSERT_CEREMONY_REASON_TOOL: &str = "choreo_assert_ceremony_reason";
pub(crate) const GET_CEREMONY_INSTANCE_TOOL: &str = "choreo_get_ceremony_instance";
pub(crate) const LIST_CEREMONY_INSTANCES_TOOL: &str = "choreo_list_ceremony_instances";
pub(crate) const REQUEST_CEREMONY_INTERVENTION_TOOL: &str = "choreo_request_ceremony_intervention";
pub(crate) const RESPOND_TO_CEREMONY_INTERVENTION_TOOL: &str =
    "choreo_respond_to_ceremony_intervention";
pub(crate) const CLOSE_CEREMONY_INTERVENTION_TOOL: &str = "choreo_close_ceremony_intervention";
pub(crate) const COLLECT_CEREMONY_EVIDENCE_TOOL: &str = "choreo_collect_ceremony_evidence";
pub(crate) const VALIDATE_CEREMONY_DRAFT_TOOL: &str = "choreo_validate_ceremony_draft";
pub(crate) const EXPLAIN_CEREMONY_DRAFT_TOOL: &str = "choreo_explain_ceremony_draft";
pub(crate) const PUBLISH_CEREMONY_DEFINITION_TOOL: &str = "choreo_publish_ceremony_definition";
pub(crate) const DIFF_CEREMONY_DEFINITIONS_TOOL: &str = "choreo_diff_ceremony_definitions";
pub(crate) const BIND_CEREMONY_PARTICIPANTS_TOOL: &str = "choreo_bind_ceremony_participants";
pub(crate) const START_PUBLISHED_CEREMONY_TOOL: &str = "choreo_start_published_ceremony";

const GRPC_TOOL_NAMES: [&str; 35] = [
    "choreo_deliberate",
    "choreo_stream_deliberation",
    "choreo_get_deliberation_result",
    "choreo_orchestrate",
    "choreo_create_council",
    "choreo_list_councils",
    "choreo_delete_council",
    "choreo_register_agent",
    "choreo_unregister_agent",
    "choreo_process_trigger_event",
    "choreo_run_council_decision",
    "choreo_register_contract",
    "choreo_list_contracts",
    "choreo_delete_contract",
    RUN_CEREMONY_TOOL,
    GET_CEREMONY_INSTANCE_TOOL,
    LIST_CEREMONY_INSTANCES_TOOL,
    START_CEREMONY_TOOL,
    START_PUBLISHED_CEREMONY_TOOL,
    RUN_CEREMONY_STEP_TOOL,
    APPLY_CEREMONY_TRANSITION_TOOL,
    APPROVE_CEREMONY_GUARD_TOOL,
    DEFER_CEREMONY_GUARD_TOOL,
    REQUEST_CEREMONY_INTERVENTION_TOOL,
    RESPOND_TO_CEREMONY_INTERVENTION_TOOL,
    CLOSE_CEREMONY_INTERVENTION_TOOL,
    COLLECT_CEREMONY_EVIDENCE_TOOL,
    ASSERT_CEREMONY_REASON_TOOL,
    VALIDATE_CEREMONY_DRAFT_TOOL,
    EXPLAIN_CEREMONY_DRAFT_TOOL,
    PUBLISH_CEREMONY_DEFINITION_TOOL,
    DIFF_CEREMONY_DEFINITIONS_TOOL,
    BIND_CEREMONY_PARTICIPANTS_TOOL,
    "choreo_get_status",
    "choreo_get_metrics",
];

/// Build the `initialize` result. Includes adapter-side metadata so
/// the client can record which backend + TLS posture it negotiated
/// without an extra round-trip.
pub(crate) fn initialize_result(
    server_name: &str,
    server_version: &str,
    backend: &str,
    grpc_tls: &str,
) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": server_name,
            "version": server_version,
        },
        "metadata": {
            "backend": backend,
            "grpc_tls": grpc_tls,
        }
    })
}

/// `tools/list` result filtered to capabilities honored by the active
/// backend.
pub(crate) fn tools_list_result(supports: impl Fn(&str) -> bool) -> Value {
    let tools = tool_catalog()
        .into_iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(&supports)
        })
        .collect::<Vec<_>>();
    json!({ "tools": tools })
}

fn tool_catalog() -> Vec<Value> {
    // One list, ordered exactly as the gRPC service orders its RPCs.
    // A test pins that correspondence both ways: a tool with no RPC,
    // or an RPC with no tool, is a surface that exists on one side
    // only, which is how two distributions drift apart.
    grpc_tool_catalog()
}

fn start_published_ceremony_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony", "version", "actor_id", "actor_kind"],
        "properties": {
            "actor_id": string_schema("Who is opening it, in whatever terms you identify callers by. Not a role from the definition: at the start its roles are not filled yet, and whoever opens a session may be a participant, an operator, or a scheduler that never takes part."),
            "actor_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party that is. Refused when missing or unrecognised, like every other actor kind."
            },
            "ceremony": string_schema("Name of the published ceremony to run."),
            "version": string_schema("Published version to bind this instance to."),
            "ceremony_id": string_schema("Identifier for the new instance. Generated when omitted."),
            "context": {
                "type": "object",
                "description": "Opening context for the working session.",
                "additionalProperties": true
            }
        }
    })
}

/// Either a published version, named, or a document supplied for the
/// occasion. Both at once has no sensible reading, and the schema says
/// so rather than leaving the server to discover it.
fn ceremony_definition_ref_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": description,
        "properties": {
            "ceremony": string_schema("Name of a published definition. Give this with `version`."),
            "version": string_schema("Version of a published definition. Give this with `ceremony`."),
            "definition_yaml": string_schema("A definition supplied for the comparison, instead of naming a published one.")
        }
    })
}

fn ceremony_draft_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["definition_yaml"],
        "properties": {
            "definition_yaml": string_schema(
                "Ceremony definition YAML to analyse. It does not need to be publishable — reporting why it is not is the point."
            )
        }
    })
}

#[allow(clippy::too_many_lines)] // 17 gRPC tool definitions form one auditable transport contract
fn grpc_tool_catalog() -> Vec<Value> {
    vec![
        tool_def(
            "choreo_deliberate",
            "Run a deliberation on the council for the task's specialty. Returns ranked proposals once the council finishes.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["task"],
                "properties": { "task": task_schema() }
            }),
        ),
        tool_def(
            "choreo_stream_deliberation",
            "Run a deliberation and return every phase-transition / result frame buffered into a single response array (no live streaming over MCP stdio).",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["task"],
                "properties": { "task": task_schema() }
            }),
        ),
        tool_def(
            "choreo_get_deliberation_result",
            "Fetch a previously-executed deliberation by task id.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["task_id"],
                "properties": { "task_id": string_schema("Stable task id used at deliberation time.") }
            }),
        ),
        tool_def(
            "choreo_orchestrate",
            "Deliberate AND execute the winning proposal through the wired executor port. Returns the winner plus an execution id.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["task"],
                "properties": {
                    "task": task_schema(),
                    "execution_options": {
                        "type": "object",
                        "additionalProperties": true,
                        "description": "Opaque executor options. Forwarded verbatim to the configured ExecutorPort."
                    }
                }
            }),
        ),
        tool_def(
            "choreo_create_council",
            "Create or replace the council for a specialty. `agent_config` is opaque and passed to the agent factory.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["specialty", "num_agents"],
                "properties": {
                    "specialty": string_schema("Free-form specialty label, e.g. \"triage\"."),
                    "num_agents": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Number of agents to seat on the council."
                    },
                    "agent_config": {
                        "type": "object",
                        "additionalProperties": true,
                        "description": "Opaque config forwarded to the agent factory."
                    }
                }
            }),
        ),
        tool_def(
            "choreo_list_councils",
            "List the councils registered on the choreographer.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "include_agents": {
                        "type": "boolean",
                        "description": "When true, return each council's agent roster."
                    }
                }
            }),
        ),
        tool_def(
            "choreo_delete_council",
            "Delete the council registered for a specialty. Idempotent: `deleted=false` means the council did not exist.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["specialty"],
                "properties": { "specialty": string_schema("Specialty whose council to delete.") }
            }),
        ),
        tool_def(
            "choreo_register_agent",
            "Register an agent on a council. `agent.kind` must be one supported by the wired AgentFactoryPort (e.g. noop, anthropic, openai, vllm).",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["specialty", "agent"],
                "properties": {
                    "specialty": string_schema("Specialty the agent belongs to."),
                    "agent": agent_summary_schema(),
                    "agent_config": {
                        "type": "object",
                        "additionalProperties": true,
                        "description": "Opaque per-agent factory config."
                    }
                }
            }),
        ),
        tool_def(
            "choreo_unregister_agent",
            "Unregister a previously-registered agent by id.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["agent_id"],
                "properties": { "agent_id": string_schema("Agent id returned by choreo_register_agent.") }
            }),
        ),
        tool_def(
            "choreo_process_trigger_event",
            "Submit a domain event that should fan out to one or more deliberations. Returns a TriggerAck reporting the dispatched task ids.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["event"],
                "properties": { "event": trigger_event_schema() }
            }),
        ),
        tool_def(
            "choreo_run_council_decision",
            "Run a council deliberation against a registered output contract and return the validated winner plus candidate breakdown.",
            run_council_decision_schema(),
        ),
        tool_def(
            "choreo_register_contract",
            "Register an `OutputContract` in the in-memory contract registry.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["contract"],
                "properties": { "contract": output_contract_schema() }
            }),
        ),
        tool_def(
            "choreo_list_contracts",
            "List every contract registered in the choreographer.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        ),
        tool_def(
            "choreo_delete_contract",
            "Delete a registered contract by id.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["contract_id"],
                "properties": { "contract_id": string_schema("Contract id previously returned by register_contract.") }
            }),
        ),
        tool_def(
            RUN_CEREMONY_TOOL,
            "Execute a declarative ceremony YAML definition and return final state, step trace, and Mermaid sequence diagram.",
            run_ceremony_schema(),
        ),

        tool_def(
            GET_CEREMONY_INSTANCE_TOOL,
            "Inspect a persistent ceremony instance, including step status and blocking guards.",
            ceremony_instance_schema(),
        ),
        tool_def(
            LIST_CEREMONY_INSTANCES_TOOL,
            "Discover ceremony instances available to this backend so a host can resume one after losing its local conversation context.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        ),
        tool_def(
            START_CEREMONY_TOOL,
            "Mount a ceremony YAML definition and start a persistent in-process instance without advancing it.",
            start_ceremony_schema(),
        ),
        tool_def(
            START_PUBLISHED_CEREMONY_TOOL,
            "Start a ceremony from a published version, binding the instance to that definition's digest so which one ran can be checked afterwards rather than taken on trust.",
            start_published_ceremony_schema(),
        ),
        tool_def(
            RUN_CEREMONY_STEP_TOOL,
            "Execute one declared step on a started ceremony instance and persist its result.",
            run_ceremony_step_schema(),
        ),
        tool_def(
            APPLY_CEREMONY_TRANSITION_TOOL,
            "Apply one enabled ceremony transition and return the updated persistent instance.",
            ceremony_transition_schema(),
        ),
        tool_def(
            APPROVE_CEREMONY_GUARD_TOOL,
            "Record an explicit human approval for a currently-blocking human guard. Call only after the human has authorized it.",
            ceremony_guard_approval_schema(),
        ),
        tool_def(
            DEFER_CEREMONY_GUARD_TOOL,
            "Record an explicit human deferral without satisfying the guard or inferring authorization.",
            ceremony_guard_deferral_schema(),
        ),
        tool_def(
            REQUEST_CEREMONY_INTERVENTION_TOOL,
            "Open a participant-requested opinion, investigation, or action on the live ceremony table. This coordinates the request; it does not authorize external mutations.",
            request_ceremony_intervention_schema(),
        ),
        tool_def(
            RESPOND_TO_CEREMONY_INTERVENTION_TOOL,
            "Record one targeted role's response to an open ceremony intervention.",
            respond_to_ceremony_intervention_schema(),
        ),
        tool_def(
            CLOSE_CEREMONY_INTERVENTION_TOOL,
            "Close an open ceremony intervention as its requesting role.",
            close_ceremony_intervention_schema(),
        ),
        tool_def(
            COLLECT_CEREMONY_EVIDENCE_TOOL,
            "Collect a non-empty evidence pack through the configured read-only host source and attach it to an open intervention.",
            collect_ceremony_evidence_schema(),
        ),
        tool_def(
            ASSERT_CEREMONY_REASON_TOOL,
            "Record why one thing this session produced led to another. Only whoever decided something may say what decided them, and only whoever did it may say how; claims about the world are open to any seat, with a stated confidence.",
            ceremony_reason_schema(),
        ),
        tool_def(
            VALIDATE_CEREMONY_DRAFT_TOOL,
            "Analyse a ceremony draft and report every structural defect at once. Read-only: it neither publishes nor executes the draft.",
            ceremony_draft_schema(),
        ),
        tool_def(
            EXPLAIN_CEREMONY_DRAFT_TOOL,
            "Describe what a ceremony draft declares and what would block its publication, in prose meant to be read back and corrected.",
            ceremony_draft_schema(),
        ),
        tool_def(
            PUBLISH_CEREMONY_DEFINITION_TOOL,
            "Fix a validated draft to an immutable version identified by a content digest. Republishing identical content is a no-op; different content under a taken version is refused, never overwritten.",
            ceremony_draft_schema(),
        ),
        tool_def(
            DIFF_CEREMONY_DEFINITIONS_TOOL,
            "Compare two ceremony definitions and say what changed — and, for each change, whether a session already running the earlier one could go on.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["before", "after"],
                "properties": {
                    "before": ceremony_definition_ref_schema("The earlier definition."),
                    "after": ceremony_definition_ref_schema("The later definition.")
                }
            }),
        ),
        tool_def(
            BIND_CEREMONY_PARTICIPANTS_TOOL,
            "Seat this session's roles: which specialty — and so which council — does each role's work here. A role left unseated is played the way the definition says.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["ceremony_id", "seating"],
                "properties": {
                    "ceremony_id": string_schema("Session being seated."),
                    "seating": {
                        "type": "object",
                        "description": "Role id to specialty. At least one seat; an empty object would change nothing.",
                        "additionalProperties": { "type": "string" }
                    }
                }
            }),
        ),
        tool_def(
            "choreo_get_status",
            "Return service health, version, uptime, and optionally statistics.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "include_stats": {
                        "type": "boolean",
                        "description": "When true, include the full Statistics snapshot in the response."
                    }
                }
            }),
        ),
        tool_def(
            "choreo_get_metrics",
            "Return the current statistics snapshot.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        ),
    ]
}

pub(crate) fn is_grpc_tool(name: &str) -> bool {
    GRPC_TOOL_NAMES.contains(&name)
}

fn run_council_decision_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["contract_id", "description"],
        "properties": {
            "council_id": string_schema("Stable council id. Exactly one of council_id / specialty must be set."),
            "specialty": string_schema("Council specialty. Exactly one of council_id / specialty must be set."),
            "contract_id": string_schema("Registered contract id the deliberation winner must satisfy."),
            "description": string_schema("Free-form task description the council reads."),
            "external_context": external_context_bundle_schema(),
            "validation_mode": {
                "type": "string",
                "enum": [
                    "VALIDATION_MODE_UNSPECIFIED",
                    "VALIDATION_MODE_STRICT",
                    "VALIDATION_MODE_WARN"
                ],
                "description": "STRICT (default) fails when no candidate passes; WARN returns the top-ranked candidate even on failure."
            },
            "metadata": task_metadata_schema()
        },
        "oneOf": [
            { "required": ["council_id"], "not": { "required": ["specialty"] } },
            { "required": ["specialty"], "not": { "required": ["council_id"] } }
        ]
    })
}

fn run_ceremony_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["definition_yaml", "actor_id", "actor_kind"],
        "properties": {
            "ceremony_id": string_schema("Optional stable ceremony instance id. The server mints one when omitted."),
            "definition_yaml": string_schema("Declarative ceremony YAML definition."),
            "actor_id": string_schema("Who is opening it, in whatever terms you identify callers by. Not a role from the definition: at the start its roles are not filled yet, and whoever opens a session may be a participant, an operator, or a scheduler that never takes part."),
            "actor_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party that is. Refused when missing or unrecognised, like every other actor kind."
            },
            "context": {
                "type": "object",
                "additionalProperties": true,
                "description": "Opaque initial ceremony context forwarded to guards and handlers."
            },
            "lease_owner_id": string_schema("Optional logical runner acquiring step leases. The server applies a default when omitted."),
            "lease_ttl_ms": {
                "type": "integer",
                "minimum": 0,
                "description": "Step lease TTL in milliseconds. Zero or omitted uses the server default."
            }
        }
    })
}

fn start_ceremony_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["definition_yaml"],
        "properties": {
            "ceremony_id": string_schema("Optional stable ceremony instance id. The server mints one when omitted."),
            "definition_yaml": string_schema("Declarative ceremony YAML definition."),
            "context": {
                "type": "object",
                "additionalProperties": true,
                "description": "Opaque initial ceremony context forwarded to guards and handlers."
            }
        }
    })
}

fn run_ceremony_step_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id", "step_id", "actor_kind"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id."),
            "step_id": string_schema("Step declared in the instance's current state."),
            "actor_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party is running it. Declared by you, because only you know: which seat runs this step comes from the definition, and that says which seat was required, not what turned up. This records who ran the step, not what produced its output — the handler is named by a host-defined string the engine will not classify."
            },
            "lease_owner_id": string_schema("Optional logical runner acquiring the step lease."),
            "idempotency_key": string_schema("Optional unique execution key. The server mints one when omitted."),
            "lease_ttl_ms": {
                "type": "integer",
                "minimum": 0,
                "description": "Step lease TTL in milliseconds. Zero or omitted uses the server default."
            }
        }
    })
}

fn ceremony_guard_approval_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id", "guard_name", "role_id", "role_kind"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id."),
            "guard_name": string_schema("Currently-blocking human guard explicitly approved by the human participant."),
            "role_id": string_schema("Seat approving it, declared by this ceremony's definition. Required: an approval that names no one is a receipt for a human decision nobody can be shown to have taken."),
            "role_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party filled that seat. Declared by you, because only you know: that this guard demands a human approval says one was required, not that one turned up, and an engine reading compliance off its own requirement would write exactly the receipt it refuses to write."
            }
        }
    })
}

fn ceremony_guard_deferral_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id", "guard_name", "role_id", "role_kind", "statement", "reason", "reconsider_when"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id."),
            "guard_name": string_schema("Currently-blocking human guard whose decision is deferred."),
            "role_id": string_schema("Seat deferring it, declared by this ceremony's definition. The fourth of what, why, when and who — and the only one nobody can reconstruct afterwards."),
            "role_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party filled that seat. Declared, never deduced."
            },
            "statement": string_schema("Human participant's own statement, preserved verbatim."),
            "reason": string_schema("Why the participant cannot decide yet."),
            "reconsider_when": {
                "type": "array",
                "minItems": 1,
                "uniqueItems": true,
                "items": { "type": "string", "minLength": 1 },
                "description": "Concrete conditions that would make it appropriate to ask again."
            }
        }
    })
}

/// Something this session produced that a reason can point at.
fn ceremony_record_ref_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind"],
        "description": description,
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["step", "agenda_item", "contribution", "guard_decision", "transition"],
                "description": "Which of the five it names. Only the field it names is read."
            },
            "step_id": string_schema("For kind `step`."),
            "agenda_item": string_schema("For kind `agenda_item` or `contribution`."),
            "ordinal": {
                "type": "integer",
                "minimum": 0,
                "description": "For kind `contribution`, its place among the answers to its item, counting from zero. For kind `transition`, the move's place in the session, counting from one."
            },
            "guard_name": string_schema("For kind `guard_decision`.")
        }
    })
}

fn ceremony_reason_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id", "role_id", "role_kind", "from", "to", "kind", "why", "confidence"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id."),
            "role_id": string_schema("Seat saying so, declared by this ceremony's definition."),
            "role_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party fills that seat. Declared by you, because only you know: a reason is a judgement, and whether a person or an agent made it is the first thing anyone weighing it wants to know."
            },
            "from": ceremony_record_ref_schema("What is being explained."),
            "to": ceremony_record_ref_schema("What explains it."),
            "kind": {
                "type": "string",
                "enum": [
                    "chosen_because",
                    "achieved_by",
                    "follows_from",
                    "satisfies_constraint",
                    "violates_constraint",
                    "supersedes",
                    "contradicts"
                ],
                "description": "How the first came from the second. `achieved_by` is the how, and it is what turns a resolved session from a precedent into a procedure. `answers` is absent: it states the shape of the session rather than anyone's judgement, and only the engine asserts it."
            },
            "why": string_schema("The reason itself, in one line. Required: an edge asserting a connection while declining to say how is a guess written down as a fact."),
            "confidence": {
                "type": "string",
                "enum": ["high", "medium", "low"],
                "description": "How sure you are. There is no fourth for `not sure enough to say` — a caller who would reach for it can decline to make the claim."
            }
        }
    })
}

fn ceremony_transition_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id", "trigger", "actor_kind"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id."),
            "trigger": string_schema("Transition trigger declared from the instance's current state."),
            "actor_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party is firing it. Declared by you, because only you know: which seat may fire this trigger comes from the definition, and that says which seat was required, not what turned up to fill it."
            }
        }
    })
}

fn ceremony_instance_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id.")
        }
    })
}

fn request_ceremony_intervention_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id", "role_id", "role_kind", "kind", "message"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id."),
            "intervention_id": string_schema("Optional stable intervention id. The server mints one when omitted."),
            "role_id": string_schema("Role requesting the intervention."),
            "role_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party fills that seat. Declared by you, because only you know: the journal records who asked the table for help, and an entry that cannot say whether a person or an agent asked is not worth the write."
            },
            "kind": {
                "type": "string",
                "enum": ["opinion", "investigation", "action"],
                "description": "Intent of the participant-created agenda item."
            },
            "target_role_ids": {
                "type": "array",
                "minItems": 1,
                "uniqueItems": true,
                "items": { "type": "string", "minLength": 1 },
                "description": "Optional responding roles. Omit to address the whole table."
            },
            "message": string_schema("Participant's request in their own words."),
            "details": attributes_schema("Structured request context or evidence references."),
            "provenance": {
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "source_intervention_id",
                    "source_response_role_id",
                    "selected_role_id"
                ],
                "properties": {
                    "source_intervention_id": string_schema("Earlier intervention containing the selected proposal."),
                    "source_response_role_id": string_schema("Role whose response contained the selected proposal."),
                    "selected_role_id": string_schema("Role selected to handle the new intervention.")
                },
                "description": "Optional trace from a table proposal to the intervention created from it."
            }
        }
    })
}

fn respond_to_ceremony_intervention_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id", "intervention_id", "role_id", "role_kind", "message"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id."),
            "intervention_id": string_schema("Open intervention id."),
            "role_id": string_schema("Targeted role contributing this response."),
            "role_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party fills that seat. Declared by you, because only you know: a contribution weighed later as precedent reads differently depending on whether a person or an agent gave it."
            },
            "message": string_schema("Role response, opinion, or result."),
            "details": attributes_schema("Structured response context or evidence references.")
        }
    })
}

fn close_ceremony_intervention_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id", "intervention_id", "role_id", "role_kind"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id."),
            "intervention_id": string_schema("Open intervention id."),
            "role_id": string_schema("Requesting role closing the intervention."),
            "role_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party fills that seat. Declared by you, because only you know: closing an item is a decision that it has been answered enough, and who made it reads differently depending on what kind of party they were."
            }
        }
    })
}

fn collect_ceremony_evidence_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ceremony_id", "intervention_id", "role_id", "role_kind", "source_id", "query"],
        "properties": {
            "ceremony_id": string_schema("Started ceremony instance id."),
            "intervention_id": string_schema("Open investigation or action intervention receiving the evidence."),
            "role_id": string_schema("Targeted role represented by the configured evidence source."),
            "role_kind": {
                "type": "string",
                "enum": ["human", "agent", "service", "engine"],
                "description": "What kind of party fills that seat. Declared by you, because only you know: this call answers the item as well as fetching what backs the answer, and it is recorded the same way a plain response is."
            },
            "source_id": string_schema("Host-configured evidence source, such as observability."),
            "query": string_schema("Specific read-only evidence request in the participant's words."),
            "details": attributes_schema("Structured query parameters such as time window or service identity.")
        }
    })
}

fn attributes_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "additionalProperties": true,
        "description": description
    })
}

// ---------------------------------------------------------------------------
// Composite schema fragments (kept in sync with `choreo.proto`)
// ---------------------------------------------------------------------------

fn task_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["task_id", "description", "specialty"],
        "properties": {
            "task_id": string_schema("Stable task identifier."),
            "description": string_schema("Free-form prompt the council deliberates over."),
            "specialty": string_schema("Specialty label of the council to dispatch to."),
            "constraints": constraints_schema(),
            "attributes": {
                "type": "object",
                "additionalProperties": true,
                "description": "Opaque per-task attributes. Forwarded to agents and validators."
            },
            "external_context": external_context_bundle_schema(),
            "metadata": task_metadata_schema()
        }
    })
}

fn constraints_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "rubric": {
                "type": "object",
                "additionalProperties": true,
                "description": "Opaque rubric forwarded to agents and validators."
            },
            "rounds": { "type": "integer", "minimum": 0, "description": "Peer-review rounds (0 = adapter default)." },
            "num_agents": { "type": "integer", "minimum": 0, "description": "Requested parallelism (0 = use council size)." },
            "deadline_ms": { "type": "integer", "minimum": 0, "description": "Optional soft deadline in ms (0 = none)." },
            "output_contract": output_contract_schema()
        }
    })
}

fn output_contract_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["contract_id", "format"],
        "properties": {
            "contract_id": string_schema("Stable contract identifier."),
            "format": {
                "type": "string",
                "enum": ["json_object"],
                "description": "Wire format. Only `json_object` is implemented today."
            },
            "fields": {
                "type": "object",
                "additionalProperties": output_field_rule_schema(),
                "description": "Map from field name to its rule."
            },
            "json_schema": {
                "type": "string",
                "description": "Optional embedded JSON Schema (draft 2020-12 or earlier). When non-empty, every proposal output is validated against it via the JsonSchemaValidator. Canonical Report-shape schema lives at api/examples/output-contracts/report.schema.json."
            }
        }
    })
}

fn output_field_rule_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "required": { "type": "boolean" },
            "allowed_string_values": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
}

fn external_context_bundle_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "bundle_id": string_schema("Caller-supplied bundle id."),
            "schema_version": string_schema("Bundle schema version label."),
            "summary": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "text": string_schema("Human-facing summary."),
                    "attributes": {
                        "type": "object",
                        "additionalProperties": true
                    }
                }
            },
            "items": {
                "type": "array",
                "items": context_item_schema()
            },
            "references": {
                "type": "array",
                "items": context_reference_schema()
            },
            "metadata": {
                "type": "object",
                "additionalProperties": true,
                "description": "Application-owned bundle metadata. Choreographer treats this as opaque."
            }
        }
    })
}

fn context_item_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["item_id", "kind"],
        "properties": {
            "item_id": string_schema("Stable item id within the bundle."),
            "kind": string_schema("Caller-defined kind label."),
            "title": { "type": "string" },
            "narrative": { "type": "string" },
            "attributes": { "type": "object", "additionalProperties": true },
            "reference_ids": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
}

fn context_reference_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["reference_id", "uri"],
        "properties": {
            "reference_id": string_schema("Stable reference id within the bundle."),
            "uri": string_schema("Pointer to the referenced artifact."),
            "title": { "type": "string" },
            "media_type": { "type": "string" },
            "attributes": { "type": "object", "additionalProperties": true }
        }
    })
}

fn task_metadata_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "source_event_id": { "type": "string" },
            "causation_id": { "type": "string" },
            "correlation_id": { "type": "string" },
            "council_contract_id": { "type": "string" },
            "output_contract_id": { "type": "string" },
            "execution_profile": {
                "type": "object",
                "additionalProperties": true,
                "description": "Opaque executor hints. Explicit Orchestrate options take precedence on overlap."
            }
        }
    })
}

fn agent_summary_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["agent_id", "specialty", "kind"],
        "properties": {
            "agent_id": string_schema("Stable agent id."),
            "specialty": string_schema("Specialty the agent serves."),
            "kind": string_schema("Adapter-defined agent kind (e.g. noop, vllm, anthropic, openai)."),
            "attributes": {
                "type": "object",
                "additionalProperties": true,
                "description": "Per-agent factory hints (provider.model, provider.endpoint, …)."
            }
        }
    })
}

fn trigger_event_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["event_id", "kind", "source", "requested_specialties"],
        "properties": {
            "event_id": string_schema("Stable producer-side event id."),
            "kind": string_schema("Free-form event kind (e.g. alert.fired, case.opened)."),
            "source": string_schema("Producer identifier."),
            "emitted_at": string_schema("RFC3339 emit timestamp. Server fills in `now` when absent."),
            "requested_specialties": {
                "type": "array",
                "minItems": 1,
                "items": { "type": "string" }
            },
            "task_description_template": { "type": "string" },
            "constraints": constraints_schema(),
            "payload": {
                "type": "object",
                "additionalProperties": true,
                "description": "Opaque domain payload."
            },
            "external_context": external_context_bundle_schema(),
            "correlation_id": { "type": "string" },
            "causation_id": { "type": "string" }
        }
    })
}

// ---------------------------------------------------------------------------
// Primitive helpers
// ---------------------------------------------------------------------------

#[allow(clippy::needless_pass_by_value)] // json! consumes via macro clone — clippy can't see that
fn tool_def(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

fn string_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "description": description,
    })
}

// ---------------------------------------------------------------------------
// Tool-call result shape (MCP spec)
// ---------------------------------------------------------------------------

/// MCP success result: `content[].text` for human consumers +
/// `structuredContent` for machine consumers + `isError: false`.
#[allow(clippy::needless_pass_by_value)] // structured is used twice; consumed by json!
pub(crate) fn tool_success_result(structured: Value) -> Value {
    let text = serde_json::to_string_pretty(&structured).expect("structured JSON should serialize");
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": structured,
        "isError": false,
    })
}

/// MCP tool error: spec says `isError: true` in the *tool result*,
/// **not** as a JSON-RPC `error`.
pub(crate) fn tool_error_result(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

// ---------------------------------------------------------------------------
// JSON-RPC framing
// ---------------------------------------------------------------------------

#[allow(clippy::needless_pass_by_value)] // both args consumed by json!
pub(crate) fn jsonrpc_result(id: Value, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
    .to_string()
}

#[allow(clippy::needless_pass_by_value)] // id consumed by json!
pub(crate) fn jsonrpc_error(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_advertises_protocol_version_and_metadata() {
        let r = initialize_result("host-mcp", "1.2.3", "grpc", "server");
        assert_eq!(r["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(r["serverInfo"]["name"], "host-mcp");
        assert_eq!(r["serverInfo"]["version"], "1.2.3");
        assert_eq!(r["metadata"]["backend"], "grpc");
        assert_eq!(r["metadata"]["grpc_tls"], "server");
    }

    #[test]
    fn tools_catalog_is_derived_one_for_one_from_grpc_service() {
        let catalog_names = grpc_catalog_tool_names();
        let proto_tool_names: Vec<String> = proto_rpc_names()
            .into_iter()
            .map(rpc_name_to_tool_name)
            .collect();
        let supported_tool_names = GRPC_TOOL_NAMES.map(str::to_owned).to_vec();

        assert_eq!(catalog_names, supported_tool_names);
        assert_eq!(
            catalog_names, proto_tool_names,
            "every underpass.choreo.v1 gRPC RPC must have exactly one MCP tool"
        );
    }

    #[test]
    fn grpc_dispatch_and_fixture_cover_every_catalog_tool() {
        let grpc_dispatch_source = include_str!("grpc/tools.rs");
        let fixture_source = include_str!("fixture.rs");

        for tool in grpc_catalog_tool_names() {
            let dispatch_arm = format!("\"{tool}\" =>");
            assert!(
                grpc_dispatch_source.contains(&dispatch_arm),
                "live gRPC backend is missing a dispatch arm for {tool}"
            );
            assert!(
                fixture_source.contains(&dispatch_arm),
                "fixture backend is missing a canned response for {tool}"
            );
        }
    }

    #[test]
    fn incremental_ceremony_tools_are_unique_catalog_extensions() {
        let all_names = catalog_tool_names();
        let unique_names = all_names.iter().collect::<std::collections::BTreeSet<_>>();

        assert_eq!(all_names.len(), 35);
        assert_eq!(unique_names.len(), all_names.len());
        assert!(all_names.contains(&VALIDATE_CEREMONY_DRAFT_TOOL.to_owned()));
        assert!(all_names.contains(&PUBLISH_CEREMONY_DEFINITION_TOOL.to_owned()));
        assert!(all_names.contains(&START_PUBLISHED_CEREMONY_TOOL.to_owned()));
        assert!(all_names.contains(&EXPLAIN_CEREMONY_DRAFT_TOOL.to_owned()));
        assert!(all_names.contains(&START_CEREMONY_TOOL.to_owned()));
        assert!(all_names.contains(&APPROVE_CEREMONY_GUARD_TOOL.to_owned()));
        assert!(all_names.contains(&DEFER_CEREMONY_GUARD_TOOL.to_owned()));
        assert!(all_names.contains(&GET_CEREMONY_INSTANCE_TOOL.to_owned()));
        assert!(all_names.contains(&LIST_CEREMONY_INSTANCES_TOOL.to_owned()));
        assert!(all_names.contains(&REQUEST_CEREMONY_INTERVENTION_TOOL.to_owned()));
        assert!(all_names.contains(&RESPOND_TO_CEREMONY_INTERVENTION_TOOL.to_owned()));
        assert!(all_names.contains(&CLOSE_CEREMONY_INTERVENTION_TOOL.to_owned()));
        assert!(all_names.contains(&COLLECT_CEREMONY_EVIDENCE_TOOL.to_owned()));
    }

    #[test]
    fn task_schema_includes_metadata_and_external_context() {
        let s = task_schema();
        let props = &s["properties"];
        assert!(props.get("task_id").is_some());
        assert!(props.get("description").is_some());
        assert!(props.get("specialty").is_some());
        assert!(props.get("constraints").is_some());
        assert!(props.get("attributes").is_some());
        assert!(props.get("external_context").is_some());
        assert!(props.get("metadata").is_some());
    }

    #[test]
    fn output_contract_format_enum_pins_implemented_modes() {
        let s = output_contract_schema();
        let formats = s["properties"]["format"]["enum"].as_array().unwrap();
        assert_eq!(formats.len(), 1);
        assert_eq!(formats[0], "json_object");
    }

    #[test]
    fn tool_results_carry_both_text_and_structured() {
        let success = tool_success_result(json!({"answer": "yes"}));
        assert_eq!(success["isError"], false);
        assert_eq!(success["structuredContent"]["answer"], "yes");
        assert!(success["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("yes"));

        let error = tool_error_result("nope");
        assert_eq!(error["isError"], true);
        assert_eq!(error["content"][0]["text"], "nope");
    }

    #[test]
    fn jsonrpc_helpers_wrap_results_and_errors() {
        let r = serde_json::from_str::<Value>(&jsonrpc_result(json!(1), json!({"x": 2}))).unwrap();
        assert_eq!(r["jsonrpc"], "2.0");
        assert_eq!(r["id"], 1);
        assert_eq!(r["result"]["x"], 2);

        let e = serde_json::from_str::<Value>(&jsonrpc_error(json!(2), -32601, "no")).unwrap();
        assert_eq!(e["error"]["code"], -32601);
        assert_eq!(e["error"]["message"], "no");
    }

    fn catalog_tool_names() -> Vec<String> {
        let tools = tools_list_result(|_| true);
        tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }

    fn grpc_catalog_tool_names() -> Vec<String> {
        grpc_tool_catalog()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    }

    fn proto_rpc_names() -> Vec<&'static str> {
        const CHOREO_PROTO: &str =
            include_str!("../../choreo-mcp-proto/proto/underpass/choreo/v1/choreo.proto");

        CHOREO_PROTO
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                let rest = trimmed.strip_prefix("rpc ")?;
                rest.split_once('(').map(|(rpc, _)| rpc.trim())
            })
            .collect()
    }

    fn rpc_name_to_tool_name(rpc: &str) -> String {
        let mut snake = String::new();
        for (idx, ch) in rpc.chars().enumerate() {
            if ch.is_uppercase() {
                if idx > 0 {
                    snake.push('_');
                }
                snake.extend(ch.to_lowercase());
            } else {
                snake.push(ch);
            }
        }
        format!("choreo_{snake}")
    }
}
