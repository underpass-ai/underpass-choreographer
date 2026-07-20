#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

forbidden=()
while IFS= read -r package; do
  name="${package%% *}"
  case "${name}" in
    async-nats|choreo-proto|sqlx|tonic)
      forbidden+=("${name}")
      ;;
  esac
done < <(cargo tree --locked -p choreo-embedded -e normal --prefix none --format '{p}')

if ((${#forbidden[@]} > 0)); then
  echo "choreo-embedded crosses the remote-infrastructure dependency boundary:" >&2
  for name in "${forbidden[@]}"; do
    echo "- ${name}" >&2
  done
  exit 1
fi

echo "choreo-embedded dependency boundary passed"
