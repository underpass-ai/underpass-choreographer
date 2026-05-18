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

# Contract gate dependencies.
bash scripts/ci/install-buf.sh
bash scripts/ci/install-asyncapi.sh

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
export DOCKER_HOST=unix://$(podman info --format '{{.Host.RemoteSocket.Path}}')
test -S "$(podman info --format '{{.Host.RemoteSocket.Path}}')"
```

If that preflight check fails, `testcontainers` will not be able to
start NATS or Postgres. In that case either:

```bash
# Preferred: bring up the user socket through systemd.
systemctl --user start podman.socket

# Fallback: run an explicit API service on a temporary Unix socket.
mkdir -p "${TMPDIR:-/tmp}/podman"
podman system service --time=0 unix://${TMPDIR:-/tmp}/podman/podman.sock
export DOCKER_HOST=unix://${TMPDIR:-/tmp}/podman/podman.sock
```

The integration scripts fail fast with this guidance when no live
Docker-compatible socket is available, instead of letting Rust tests die
later with `SocketNotFoundError`.

## Daily commands

```bash
just                 # list every recipe
just check           # contract + fmt-check + clippy + test + bench-compile
just fmt             # apply rustfmt in-place
just contract        # proto + AsyncAPI gate
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

E2E is run manually from the repository before cutting a release:

```bash
make e2e-compose     # full stack via docker compose + runner
make e2e-kubernetes  # Kubernetes cluster + Helm chart + runner Job
```

For an existing cluster, the standard path matches sibling repos:
push the Choreographer image and runner image to `ghcr.io`, create
an `imagePullSecrets` named `ghcr-pull` in the target namespace, and
point the script at that registry.

```bash
podman login ghcr.io

kubectl create secret docker-registry ghcr-pull \
  --docker-server=ghcr.io \
  --docker-username=<github-user> \
  --docker-password=<github-pat> \
  -n <namespace>

E2E_NAMESPACE=<namespace> \
E2E_IMAGE_REPOSITORY_PREFIX=ghcr.io/underpass-ai \
E2E_IMAGE_PULL_SECRET=ghcr-pull \
E2E_IMAGE_TAG=dev-$(git rev-parse --short HEAD) \
make e2e-kubernetes
```

If `kind` is installed the script still supports `kind load
docker-image`; otherwise it can fall back to a cluster-local
registry, but that path is cluster-specific and not the default
operator story.

### Provider-E2E (vLLM)

Exercises the `agent-vllm` adapter directly against a real vLLM
endpoint — not the full choreographer — so it pins the provider
wire contract. The Kubernetes Job mounts a client certificate and
hits the endpoint via mTLS.

```bash
# Build the runner image and push it where the cluster can pull it.
IMAGE_TAG=<registry>/underpass-choreographer-e2e-provider:dev \
    make build-provider-image

# Edit tests/e2e/kubernetes/provider-vllm-job.yaml to use that tag,
# then:
NAMESPACE=<ns-holding-e2e-client-tls> make e2e-provider-vllm
```

Env vars consumed by the runner (set in the Job's `env` block,
edit the manifest for a different endpoint):

| Var | Required | Notes |
|---|---|---|
| `CHOREO_VLLM_ENDPOINT` | yes | base URL, e.g. `https://llm.underpassai.com` |
| `CHOREO_VLLM_MODEL` | yes | model id, e.g. `google/gemma-4-31B-it` |
| `CHOREO_VLLM_CLIENT_CERT_PATH` | with key | PEM file for mTLS client cert |
| `CHOREO_VLLM_CLIENT_KEY_PATH` | with cert | PEM file for mTLS client key |
| `CHOREO_VLLM_BEARER_TOKEN` | optional | bearer auth |
| `CHOREO_VLLM_MAX_TOKENS` | optional | default `1024`; bump when the model uses a reasoning parser that burns budget before `content` |

The runner validates `generate` + `critique` + `revise` each
return text of at least 20 characters; a failing assertion exits
the Job non-zero and the script surfaces the pod status.

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
# With no external services.
CHOREO_NATS_ENABLED=false just run

# With defaults, the binary expects NATS at nats://nats:4222 and uses
# in-memory persistence unless CHOREO_POSTGRES_URL is set.
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
| `contract` | proto + AsyncAPI validation + blocking proto breaking check | every PR |
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
| `e2e-kubernetes` | kubernetes + chart + runner Job | **manual** |

Every row except the last two gates a PR. E2E stays outside CI — the
per-PR gates already cover the compile-and-unit surface, and E2E is
reserved for manual pre-release validation via `make`.
