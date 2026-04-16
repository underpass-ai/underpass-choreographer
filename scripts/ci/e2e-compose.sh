#!/usr/bin/env bash
set -euo pipefail

# End-to-end validation via compose.
#
# Runtime-agnostic: autodetects `docker compose` or `podman compose`
# in that order. Override with `CONTAINER_RUNTIME=podman|docker|auto`.
#
# Brings up Choreographer + NATS + the e2e runner container and runs
# the runner against the stack. The runner drives scenarios over the
# public gRPC / AsyncAPI contract — no access to internals.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
COMPOSE_FILE="${ROOT_DIR}/tests/e2e/docker-compose.e2e.yaml"
: "${CONTAINER_RUNTIME:=auto}"

cd "${ROOT_DIR}"

if [ ! -f "${COMPOSE_FILE}" ]; then
  echo "::warning::${COMPOSE_FILE} not present yet; E2E placeholder"
  exit 0
fi

select_compose() {
  case "${CONTAINER_RUNTIME}" in
    auto)
      if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
        echo "docker compose"
        return 0
      fi
      if command -v podman >/dev/null 2>&1 && podman compose --version >/dev/null 2>&1; then
        echo "podman compose"
        return 0
      fi
      if command -v podman-compose >/dev/null 2>&1; then
        echo "podman-compose"
        return 0
      fi
      echo "no supported compose runtime found (need 'docker compose', 'podman compose', or 'podman-compose')" >&2
      return 1
      ;;
    docker)
      command -v docker >/dev/null 2>&1 || { echo "docker not installed" >&2; return 1; }
      docker compose version >/dev/null 2>&1 || { echo "docker compose plugin missing" >&2; return 1; }
      echo "docker compose" ;;
    podman)
      if command -v podman >/dev/null 2>&1 && podman compose --version >/dev/null 2>&1; then
        echo "podman compose"
      elif command -v podman-compose >/dev/null 2>&1; then
        echo "podman-compose"
      else
        echo "podman compose support not found (need podman>=4.3 with compose, or podman-compose)" >&2
        return 1
      fi ;;
    *)
      echo "unsupported CONTAINER_RUNTIME=${CONTAINER_RUNTIME}; expected auto, docker, or podman" >&2
      return 1 ;;
  esac
}

# Read as an array so the command composes correctly whether it's a
# single word (`podman-compose`) or two (`docker compose`).
read -r -a COMPOSE <<<"$(select_compose)"
echo ">>> using compose runtime: ${COMPOSE[*]}" >&2

cleanup() {
  "${COMPOSE[@]}" -f "${COMPOSE_FILE}" logs --no-color > tests/e2e/compose.log 2>&1 || true
  "${COMPOSE[@]}" -f "${COMPOSE_FILE}" down --volumes --remove-orphans || true
}
trap cleanup EXIT

"${COMPOSE[@]}" -f "${COMPOSE_FILE}" up --build --abort-on-container-exit --exit-code-from e2e-runner
