#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

provider_features=(
  --features choreo-adapters/agent-anthropic
  --features choreo-adapters/agent-openai
  --features choreo-adapters/agent-vllm
)

bash scripts/ci/contract-gate.sh
cargo fmt --all -- --check
bash scripts/ci/embedded-dependency-boundary.sh
cargo clippy --workspace --all-targets --locked "${provider_features[@]}" -- -D warnings
cargo test --workspace --locked "${provider_features[@]}"
bash scripts/ci/bench-compile.sh
