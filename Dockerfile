# syntax=docker/dockerfile:1.7
#
# Underpass Choreographer — multi-stage build.
# Works identically under docker and podman. Produces a minimal
# distroless-style runtime image that runs as a non-root user.

ARG RUST_VERSION=1.90.0
ARG DEBIAN_RELEASE=bookworm

# ---------------------------------------------------------------------------
# Builder
# ---------------------------------------------------------------------------
FROM docker.io/library/rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS builder

ENV CARGO_INCREMENTAL=0 \
    CARGO_TERM_COLOR=always \
    RUSTFLAGS="-C strip=symbols"

RUN apt-get update \
 && apt-get install -y --no-install-recommends protobuf-compiler ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY rust-toolchain.toml ./
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin choreo \
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
