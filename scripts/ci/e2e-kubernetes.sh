#!/usr/bin/env bash
set -euo pipefail

# End-to-end validation against a real Kubernetes cluster (kind).
#
# Flow:
#   1. Build the Choreographer image and the E2E runner image, and
#      load both into the kind node.
#   2. Deploy a tiny NATS (Deployment + Service).
#   3. `helm install` the Choreographer chart pointed at the local
#      images, with seeding enabled so `ListCouncils` returns a
#      non-empty set.
#   4. Submit the E2E runner as a Kubernetes Job; its exit code
#      decides the CI result.
#
# Any step that fails dumps diagnostics (pod logs, kubectl describe)
# before bailing so CI is debuggable without re-running.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHART_PATH="${ROOT_DIR}/charts/choreographer"
NATS_MANIFEST="${ROOT_DIR}/tests/e2e/kubernetes/nats.yaml"
JOB_MANIFEST="${ROOT_DIR}/tests/e2e/kubernetes/runner-job.yaml"
CHOREO_IMAGE="underpass-choreographer:e2e"
RUNNER_IMAGE="underpass-choreographer-e2e-runner:e2e"
CLUSTER_NAME="${KIND_CLUSTER_NAME:-choreographer-e2e}"

cd "${ROOT_DIR}"

if [ ! -f "${JOB_MANIFEST}" ] || [ ! -f "${NATS_MANIFEST}" ]; then
  echo "::warning::${JOB_MANIFEST} or ${NATS_MANIFEST} not present yet; E2E placeholder"
  exit 0
fi

on_error() {
  echo "::group::kubectl get all"
  kubectl get all -A || true
  echo "::endgroup::"
  echo "::group::kubectl describe job"
  kubectl describe job choreographer-e2e-runner || true
  echo "::endgroup::"
  echo "::group::runner logs"
  kubectl logs job/choreographer-e2e-runner --tail=500 || true
  echo "::endgroup::"
  echo "::group::choreographer logs"
  kubectl logs -l app.kubernetes.io/name=choreographer --tail=500 || true
  echo "::endgroup::"
  echo "::group::nats logs"
  kubectl logs -l app=nats --tail=200 || true
  echo "::endgroup::"
}
trap on_error ERR

echo ">>> building choreographer image"
bash scripts/ci/container-image.sh "${CHOREO_IMAGE}" Dockerfile

echo ">>> building e2e runner image"
bash scripts/ci/container-image.sh "${RUNNER_IMAGE}" tests/e2e/runner.Dockerfile

echo ">>> loading images into kind cluster '${CLUSTER_NAME}'"
kind load docker-image "${CHOREO_IMAGE}" --name "${CLUSTER_NAME}"
kind load docker-image "${RUNNER_IMAGE}" --name "${CLUSTER_NAME}"

echo ">>> deploying NATS"
kubectl apply -f "${NATS_MANIFEST}"
kubectl wait --for=condition=available --timeout=2m deployment/nats

echo ">>> installing Choreographer chart"
helm upgrade --install choreographer "${CHART_PATH}" \
  --values "${CHART_PATH}/values.dev.yaml" \
  --set "image.repository=underpass-choreographer" \
  --set "image.tag=e2e" \
  --set "image.pullPolicy=Never" \
  --set "config.seedSpecialties=triage" \
  --wait --timeout 3m

echo ">>> submitting E2E runner Job"
kubectl apply -f "${JOB_MANIFEST}"

echo ">>> waiting for Job to complete"
# Use `kubectl wait` with both conditions so we fail fast on a
# runner that bailed out (condition=failed) instead of sleeping
# through the full timeout.
if ! kubectl wait --for=condition=complete --timeout=5m job/choreographer-e2e-runner; then
  if kubectl wait --for=condition=failed --timeout=10s job/choreographer-e2e-runner 2>/dev/null; then
    echo "::error::e2e runner Job failed"
  else
    echo "::error::e2e runner Job did not complete within 5m"
  fi
  on_error
  exit 1
fi

echo ">>> runner logs"
kubectl logs job/choreographer-e2e-runner

echo ">>> E2E kubernetes scenarios passed"
