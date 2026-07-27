# syntax=docker/dockerfile:1.7
#
# Underpass Choreographer — multi-stage build.
# Works identically under docker and podman. Produces a minimal
# distroless-style runtime image that runs as a non-root user.

ARG RUST_VERSION=1.97.1
ARG DEBIAN_RELEASE=bookworm

# ---------------------------------------------------------------------------
# Builder
# ---------------------------------------------------------------------------
FROM docker.io/library/rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS builder

ENV CARGO_INCREMENTAL=0 \
    CARGO_TERM_COLOR=always \
    RUSTFLAGS="-C strip=symbols"

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      protobuf-compiler \
      libprotobuf-dev \
      ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY rust-toolchain.toml ./
COPY crates ./crates

# `agent-openai` and `agent-vllm` are enabled so the
# `DispatchingAgentFactory` can materialise both kinds. The compose
# E2E registers an OpenAI-kind agent in scenario 8 and a vLLM-kind
# agent in scenario 9, both pointing at the same `stub-llm` sidecar
# (which speaks `POST /v1/chat/completions` — a body shape both
# adapters use). `agent-anthropic` stays off to keep the production
# image minimal; operators that need it build a downstream image
# that flips the corresponding flag.
#
# `otel` is compiled in but dormant: it exports spans over OTLP only
# when `CHOREO_OTLP_ENDPOINT` is set, so an image without that env is
# byte-for-byte the same behaviour as before (JSON spans to stdout).
RUN --mount=type=cache,id=cargo-registry-choreo,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-target-choreo,target=/src/target \
    cargo build --release --locked --bin choreo \
        --features choreo-adapters/agent-openai \
        --features choreo-adapters/agent-vllm \
        --features choreo/otel \
 && install -Dm 0755 target/release/choreo /out/choreo

# ---------------------------------------------------------------------------
# Runtime
# ---------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

LABEL org.opencontainers.image.title="underpass-choreographer" \
      org.opencontainers.image.description="Event-driven coordinator of specialist agent councils. Use-case agnostic." \
      org.opencontainers.image.vendor="Underpass AI" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.source="https://github.com/underpass-ai/underpass-choreographer"

COPY --from=builder /out/choreo /usr/local/bin/choreo

USER nonroot:nonroot

EXPOSE 50055

ENTRYPOINT ["/usr/local/bin/choreo"]
