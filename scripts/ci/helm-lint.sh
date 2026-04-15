#!/usr/bin/env bash
set -euo pipefail

CHART_PATH="${1:-charts/choreographer}"
DEV_VALUES="${CHART_PATH}/values.dev.yaml"
DEFAULT_ERR="${TMPDIR:-/tmp}/choreographer-helm-default.err"

helm lint "${CHART_PATH}" -f "${DEV_VALUES}"
helm template choreographer "${CHART_PATH}" -f "${DEV_VALUES}" >/tmp/choreographer-helm-template.yaml

# Default render (no values) must refuse — we require an explicit image reference,
# mirroring the kernel's chart discipline (no mutable :latest in production).
if helm template choreographer "${CHART_PATH}" > /dev/null 2>"${DEFAULT_ERR}"; then
  echo "default chart render unexpectedly succeeded" >&2
  exit 1
fi

grep -q "set image.tag or image.digest" "${DEFAULT_ERR}"
