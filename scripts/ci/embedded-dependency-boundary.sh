#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

assert_embedded_boundary() {
  local label="$1"
  shift
  local forbidden=()
  local package
  local name

  while IFS= read -r package; do
    name="${package%% *}"
    case "${name}" in
      async-nats|choreo-mcp-proto|choreo-proto|prost|prost-types|sqlx|tonic)
        forbidden+=("${name}")
        ;;
    esac
  done < <(cargo tree --locked "$@" -e normal --prefix none --format '{p}')

  if ((${#forbidden[@]} > 0)); then
    echo "${label} crosses the remote-infrastructure dependency boundary:" >&2
    for name in "${forbidden[@]}"; do
      echo "- ${name}" >&2
    done
    exit 1
  fi

  echo "${label} dependency boundary passed"
}

assert_embedded_boundary "choreo-embedded" -p choreo-embedded
assert_embedded_boundary \
  "choreo-mcp embedded backend" \
  -p choreo-mcp \
  --no-default-features \
  --features embedded
