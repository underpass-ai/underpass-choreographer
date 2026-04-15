#!/usr/bin/env bash
set -euo pipefail

# End-to-end validation via docker-compose.
#
# Brings up Choreographer + NATS + a fake agent harness and runs the
# e2e runner container against the stack. The runner drives scenarios
# over the public gRPC/AsyncAPI contract — no access to internals.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
COMPOSE_FILE="${ROOT_DIR}/tests/e2e/docker-compose.e2e.yaml"

cd "${ROOT_DIR}"

if [ ! -f "${COMPOSE_FILE}" ]; then
  echo "::warning::${COMPOSE_FILE} not present yet; E2E placeholder"
  exit 0
fi

cleanup() {
  docker compose -f "${COMPOSE_FILE}" logs --no-color > tests/e2e/compose.log || true
  docker compose -f "${COMPOSE_FILE}" down --volumes --remove-orphans || true
}
trap cleanup EXIT

docker compose -f "${COMPOSE_FILE}" up --build --abort-on-container-exit --exit-code-from e2e-runner
