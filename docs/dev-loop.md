# Developer loop

Honest recipes for iterating on the Underpass Choreographer. Each
command mirrors a CI gate one-for-one — when CI is red, the same
command produces the same failure locally.

## Setup

```bash
# Rust toolchain pinned at the workspace minimum.
rustup toolchain install 1.90.0
rustup default 1.90.0

# Optional but recommended — installs command aliases from justfile.
cargo install just --locked

# Protoc for tonic code generation.
# (Debian/Ubuntu: apt install protobuf-compiler; Fedora: dnf install protobuf-compiler)
protoc --version

# Container runtime for integration / E2E suites. Either docker or
# podman works. testcontainers-rs auto-detects DOCKER_HOST.
docker version  # or: podman version
```

Podman users need the user-level socket running:

```bash
systemctl --user start podman.socket
export DOCKER_HOST=unix:///run/user/$(id -u)/podman/podman.sock
```

## Daily commands

```bash
just                 # list every recipe
just check           # fmt-check + clippy + test + bench-compile
just fmt             # apply rustfmt in-place
just clippy          # warnings-as-errors on the full provider matrix
just test            # unit + in-process integration tests
just helm-lint       # helm lint + chart hardening assertions
```

Before opening a PR:

```bash
just check && just helm-lint
```

This is exactly what the per-PR CI gates run. If `just check`
passes, the PR will pass (excluding the container-backed gates,
which need Docker/podman).

## Container-backed checks

```bash
just integration     # integration-nats + integration-postgres
just integration-nats
just integration-postgres
```

Each spins testcontainers for the real service (NATS 2, Postgres 16)
via the system container runtime.

## End-to-end (manual only)

E2E workflows run on `workflow_dispatch` in CI (see
`.github/workflows/e2e.yml`). Run locally before cutting a release:

```bash
just e2e-compose     # full stack via docker compose + runner
just e2e-kubernetes  # kind cluster + Helm chart + runner Job
```

## Benchmarks

```bash
just bench-compile           # keep criterion benches compiling (CI gate)
just bench-trace             # TraceContext parse / format / generate
just bench-deliberate        # DeliberateUseCase end-to-end
just bench-experiment-001    # reproduce docs/experiments/001
```

Criterion is compile-gated on every PR but not run — the signal-to-
noise is wrong for a per-PR check. Record numbers under
`docs/experiments/NNN-*/results/` when running intentionally (see
[`docs/experiments/README.md`](experiments/README.md) for the
lab-notebook contract).

## Running the binary

```bash
# With defaults (no NATS, no Postgres).
just run

# With the OTLP exporter compiled in. At runtime, set
# CHOREO_OTLP_ENDPOINT to actually ship spans somewhere.
CHOREO_OTLP_ENDPOINT=http://localhost:4317 just run-otel
```

Full configuration surface — see the table in
[`crates/choreo-adapters/src/config.rs`](../crates/choreo-adapters/src/config.rs)
and
[`charts/choreographer/values.yaml`](../charts/choreographer/values.yaml).

## Adding a new port

1. Define the trait in `choreo-core/src/ports/<name>.rs` and
   re-export from `ports/mod.rs`. Only domain types; no IO, no
   vendor vocabulary.
2. Add adapter implementations under
   `choreo-adapters/src/{memory,nats,postgres,grpc,…}/`.
3. Wire the adapter through `choreo/src/compose.rs` (typically in
   `wire_persistence` / `wire_messaging` or next to them).
4. Add unit tests with an in-process stub.
5. Add an integration test when the adapter has external
   behaviour (e.g. Postgres schema, NATS subjects, gRPC wire
   format). See
   [`crates/choreo-tests-integration/tests/`](../crates/choreo-tests-integration/tests/)
   for shape.

## Adding a new use case

1. Create `choreo-app/src/usecases/<name>.rs` exposing a struct
   with constructor-injected ports + an `async fn execute`.
2. Add `#[tracing::instrument(name = "...", skip_all,
   fields(...))]` on `execute` with the domain fields operators
   will query by.
3. Re-export from `usecases/mod.rs`.
4. Thread through `choreo/src/compose.rs`.
5. If the use case exposes a gRPC surface, wire the handler in
   `choreo-adapters/src/grpc/service.rs` and call
   `link_span_to_metadata(&request)` at the top of the handler
   body so W3C tracecontext propagation keeps working.

## Adding a new provider adapter

Provider adapters (LLMs, rule engines, humans-in-the-loop) live
behind their own Cargo feature in `choreo-adapters/Cargo.toml`. See
`agent-anthropic`, `agent-openai`, `agent-vllm` for the pattern. No
provider is privileged — every one is a peer behind its flag.

## Release

See [`docs/release.md`](release.md).

## What the CI gates actually check

| Gate | Command | Runs on |
|---|---|---|
| `rustfmt` | `cargo fmt --all -- --check` | every PR |
| `contract` | proto + AsyncAPI breaking-change check | every PR |
| `clippy` | `cargo clippy` with `-D warnings` on full provider matrix | every PR |
| `test` | `cargo test` on full provider matrix | every PR |
| `benches-compile` | `cargo bench --workspace --no-run` | every PR |
| `integration-nats` | testcontainers NATS tests | every PR |
| `integration-postgres` | testcontainers Postgres tests | every PR |
| `helm-chart` | `helm lint` + hardened-render assertions | every PR |
| `container-image` | image builds from `Dockerfile` | every PR |
| `dependency-review` | GitHub dependency-review-action | every PR |
| `sonarcloud` | coverage + quality gate | every PR (if token set) |
| `e2e-compose` | full stack via docker compose + runner | **manual** |
| `e2e-kubernetes` | kind + chart + runner Job | **manual** |

Every row except the last two gates a PR. E2E is on-demand via
`gh workflow run e2e.yml` — the per-PR gates already cover the
compile-and-unit surface, and E2E is reserved for pre-release
validation.
