#!/usr/bin/env bash
set -euo pipefail

# End-to-end validation on Kubernetes.
#
# 1. Builds the Choreographer container image and loads it into kind.
# 2. Installs the Helm chart with dev values.
# 3. Submits an E2E runner Kubernetes Job that drives the scenarios
#    over the public gRPC/AsyncAPI contract and writes a report.
# 4. Streams Job logs and fails the script if the Job does not succeed.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHART_PATH="${ROOT_DIR}/charts/choreographer"
JOB_MANIFEST="${ROOT_DIR}/tests/e2e/kubernetes/runner-job.yaml"
IMAGE="underpass-choreographer:e2e"

cd "${ROOT_DIR}"

if [ ! -f "${JOB_MANIFEST}" ]; then
  echo "::warning::${JOB_MANIFEST} not present yet; E2E placeholder"
  exit 0
fi

echo ">>> building image"
bash scripts/ci/container-image.sh "${IMAGE}"

echo ">>> loading image into kind"
kind load docker-image "${IMAGE}" --name choreographer-e2e

echo ">>> installing chart"
helm upgrade --install choreographer "${CHART_PATH}" \
  --values "${CHART_PATH}/values.dev.yaml" \
  --set "image.repository=underpass-choreographer" \
  --set "image.tag=e2e" \
  --set "image.pullPolicy=Never" \
  --wait --timeout 3m

echo ">>> submitting e2e runner job"
kubectl apply -f "${JOB_MANIFEST}"

echo ">>> waiting for job completion"
kubectl wait --for=condition=complete --timeout=10m job/choreographer-e2e-runner || {
  kubectl logs job/choreographer-e2e-runner || true
  exit 1
}

kubectl logs job/choreographer-e2e-runner
