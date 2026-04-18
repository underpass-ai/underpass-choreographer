# syntax=docker/dockerfile:1.7
#
# Provider-E2E runner container.
#
# Builds the `choreo-e2e-provider` binary and ships it in a minimal,
# non-root image. Exercises the `agent-vllm` adapter against a real
# vLLM endpoint — not the full choreographer stack — so it has zero
# NATS / Postgres / gRPC dependencies at runtime.

ARG RUST_VERSION=1.90.0
ARG DEBIAN_RELEASE=bookworm

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

RUN --mount=type=cache,id=cargo-registry-e2e-provider,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-target-e2e-provider,target=/src/target \
    cargo build --release --locked --bin choreo-e2e-provider \
 && install -Dm 0755 target/release/choreo-e2e-provider /out/runner

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

LABEL org.opencontainers.image.title="underpass-choreographer-e2e-provider" \
      org.opencontainers.image.description="Drives the vLLM agent adapter against a real vLLM endpoint. Not shipped." \
      org.opencontainers.image.vendor="Underpass AI" \
      org.opencontainers.image.licenses="Apache-2.0"

COPY --from=builder /out/runner /usr/local/bin/choreo-e2e-provider

USER nonroot:nonroot

ENTRYPOINT ["/usr/local/bin/choreo-e2e-provider"]
