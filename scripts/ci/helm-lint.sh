#!/usr/bin/env bash
set -euo pipefail

CHART_PATH="${1:-charts/choreographer}"
DEV_VALUES="${CHART_PATH}/values.dev.yaml"
TMP_DIR="${TMPDIR:-/tmp}"
DEFAULT_ERR="${TMP_DIR}/choreographer-helm-default.err"
HARDENED_OUT="${TMP_DIR}/choreographer-helm-hardened.yaml"
HARDENED_ERR="${TMP_DIR}/choreographer-helm-hardened.err"

helm lint "${CHART_PATH}" -f "${DEV_VALUES}"
helm template choreographer "${CHART_PATH}" -f "${DEV_VALUES}" >/tmp/choreographer-helm-template.yaml

# --- Gate 1: default render refuses to produce a manifest without a
# pinned image. Keeps ":latest" accidents out of production.
if helm template choreographer "${CHART_PATH}" > /dev/null 2>"${DEFAULT_ERR}"; then
  echo "default chart render unexpectedly succeeded" >&2
  exit 1
fi
grep -q "set image.tag or image.digest" "${DEFAULT_ERR}"

# --- Gate 2: persistence.postgres.enabled without any URL source
# must fail loudly. Mis-configured persistence should never silently
# install a broken pod.
if helm template choreographer "${CHART_PATH}" \
  --set image.tag=v0 \
  --set persistence.postgres.enabled=true \
  > /dev/null 2>"${HARDENED_ERR}"; then
  echo "postgres-enabled-without-url render unexpectedly succeeded" >&2
  exit 1
fi
grep -q "persistence.postgres.enabled=true requires" "${HARDENED_ERR}"

# --- Gate 3: full hardened render (every knob turned on) must
# produce a valid manifest that carries every hardening feature.
helm template choreographer "${CHART_PATH}" \
  --set image.tag=v0 \
  --set networkPolicy.enabled=true \
  --set persistence.postgres.enabled=true \
  --set persistence.postgres.urlFromSecret.name=pg-dsn \
  --set persistence.postgres.urlFromSecret.key=url \
  --set pdb.enabled=true \
  > "${HARDENED_OUT}"

# Required items in the hardened manifest. Each assertion pins a
# specific guarantee operators will rely on; a rename anywhere in
# the chart breaks CI.
required_markers=(
  "kind: NetworkPolicy"
  "kind: PodDisruptionBudget"
  "automountServiceAccountToken: false"
  "readOnlyRootFilesystem: true"
  "emptyDir:"
  "mountPath: /tmp"
  "secretKeyRef:"
  'name: "pg-dsn"'
  'key: "url"'
)
for marker in "${required_markers[@]}"; do
  if ! grep -qF -- "${marker}" "${HARDENED_OUT}"; then
    echo "hardened chart manifest missing required marker: ${marker}" >&2
    exit 1
  fi
done
