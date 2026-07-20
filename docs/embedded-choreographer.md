# Choreographer Embedded

Status: implemented first slice. The embedded surface currently covers the
ceremony engine. Native embedded facades for the broader council and
deliberation APIs are not claimed yet.

## One engine, two distributions

Choreographer has two consumption modes. They share `choreo-core`,
`choreo-app`, domain invariants and the workspace release version.

| Distribution | Entry point | Owns | Does not require |
|---|---|---|---|
| Deployable | `choreo` binary | process config, gRPC/HTTP servers, optional NATS and Postgres wiring | an embedding host |
| Embedded | `choreo-embedded` library | in-process facade and host adapters | sockets, gRPC, NATS, Postgres or environment configuration |

The embedded crate is not a second ceremony engine and does not duplicate
domain behavior. It calls the same application use cases used by the
deployable composition.

```text
                  choreo-core
            domain + ports + invariants
                       ^
                       |
                   choreo-app
                  use cases
                 /         \
                /           \
       choreo-embedded      choreo
       host callbacks       gRPC/NATS/HTTP
       injected ports       deployment config
```

Both distributions report the same Cargo workspace version. Ceremony
definitions retain their own independent `CeremonyVersion`; release version
and definition version solve different compatibility problems.

## Architectural boundaries

- The domain remains transport-, provider- and product-agnostic.
- The embedded facade tells application use cases what to do; it does not
  mutate aggregates or persistence state itself.
- Every replaceable dependency is a `choreo-core` port.
- Every concrete host integration is an adapter.
- Domain aggregates continue to own state transitions and invariants.
- One production class lives in one source file.
- The host retains ownership of its async runtime and the lifecycle of injected
  resources.

## Default embedded adapters

`EmbeddedChoreographer::default()` deliberately chooses a safe local profile:

| Port | Default adapter |
|---|---|
| ceremony definitions | `InMemoryCeremonyDefinitionRepository` |
| ceremony instances | `InMemoryCeremonyInstanceRepository` |
| ceremony transcript | `InMemoryCeremonyContextStore` |
| step execution | `NoopCeremonyStepHandler` |
| clock | `SystemClock` |
| metrics | `NoopMetricsRecorder` |

These defaults start no service and perform no remote IO. They are suitable for
single-process workflows, tests and hosts that begin with ephemeral state.
They are not a durability claim: a host that must resume after process loss
injects persistent implementations of the same repositories and context port.

## Host callback adapter

`CallbackCeremonyStepHandler` turns an async Rust callback into a
`CeremonyStepHandlerPort`. It is the smallest useful boundary for a host that
wants its own agent runtime, tool system or human interaction to execute a
ceremony step.

```rust,no_run
use choreo_core::value_objects::{StepOutput, StepResult};
use choreo_embedded::EmbeddedChoreographer;

let choreographer = EmbeddedChoreographer::builder()
    .with_step_handler_callback(|request| async move {
        let _kind = request.handler_kind();
        // Delegate to the host's own agent/tool/human subsystem here.
        StepResult::completed(StepOutput::empty())
    })
    .build();
```

For richer integrations the builder accepts `Arc<dyn ...Port>` for:

- definition repository;
- instance repository;
- context store;
- step handler;
- clock;
- metrics recorder.

The host keeps the concrete adapter handle when it needs adapter-specific
administration. The embedded facade does not expose a service locator.

## Ceremony API

The first slice exposes commands and queries required for both one-shot and
human-active execution:

- mount one or more typed definitions, or one YAML definition;
- run a ceremony to completion;
- start a ceremony without advancing it;
- start, run or complete an individual step;
- approve a human guard;
- apply an authorized transition;
- retrieve definitions, an instance and its transcript.

Mounting and queries pass through `choreo-app` use cases. Execution passes
through the existing ceremony use cases; the embedded crate contains no second
state machine.

## Dependency boundary

`choreo-embedded` depends on `choreo-adapters` with default features disabled.
The adapter crate now gates the outbound Runtime gRPC client behind
`runtime-grpc`, separately from the inbound `grpc`, `nats` and `postgres`
features. The embedded dependency tree contains none of `tonic`, `async-nats`
or `sqlx`.

The deployable binary enables `grpc`, `nats`, `postgres` and `runtime-grpc`
explicitly, preserving its existing deployment capabilities.

## Current limits

- The embedded facade currently covers ceremonies, not every public gRPC RPC.
- The default repositories are process-local and ephemeral.
- Callbacks execute on the caller's async runtime; Choreographer does not create
  or hide a runtime.
- Packaging to crates.io and a stable compatibility commitment wait for the
  repository's first public release.
