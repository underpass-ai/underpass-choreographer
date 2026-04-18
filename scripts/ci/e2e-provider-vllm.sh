#!/usr/bin/env bash
set -euo pipefail

# Launches the vLLM provider-E2E Job, waits for completion, tails
# logs, and cleans up. Namespace defaults to `underpass-runtime`
# (where the e2e-client-tls secret lives); override with $NAMESPACE.
#
# The container image must already be built and pushed. Reuses the
# E2E compose image tag by convention (see `just build-provider-image`).

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NAMESPACE="${NAMESPACE:-underpass-runtime}"
JOB_NAME="${JOB_NAME:-choreographer-e2e-provider-vllm}"
MANIFEST="${ROOT_DIR}/tests/e2e/kubernetes/provider-vllm-job.yaml"
TIMEOUT="${TIMEOUT:-180s}"

cleanup() {
    kubectl -n "${NAMESPACE}" delete job "${JOB_NAME}" --ignore-not-found --wait=false >/dev/null 2>&1 || true
}

# Make the cleanup idempotent — a prior failed Job would block apply.
cleanup
trap cleanup EXIT

kubectl apply -n "${NAMESPACE}" -f "${MANIFEST}"

# Poll for the Job's pod to be scheduled, then tail its logs.
echo ">>> waiting for pod"
for _ in $(seq 1 30); do
    pod=$(kubectl -n "${NAMESPACE}" get pod \
        -l "job-name=${JOB_NAME}" \
        -o=jsonpath='{.items[0].metadata.name}' 2>/dev/null || true)
    if [ -n "${pod:-}" ]; then
        break
    fi
    sleep 2
done

if [ -z "${pod:-}" ]; then
    echo "error: Job ${JOB_NAME} produced no pod" >&2
    kubectl -n "${NAMESPACE}" get job "${JOB_NAME}" -o yaml >&2 || true
    exit 1
fi

echo ">>> tailing ${pod}"
kubectl -n "${NAMESPACE}" logs -f "${pod}" || true

# Surface the final Job status so callers can script on exit code.
if kubectl -n "${NAMESPACE}" wait --for=condition=complete \
    --timeout="${TIMEOUT}" "job/${JOB_NAME}" >/dev/null 2>&1; then
    echo ">>> Job completed successfully"
    exit 0
fi

echo ">>> Job did not complete within ${TIMEOUT} — dumping status" >&2
kubectl -n "${NAMESPACE}" describe job "${JOB_NAME}" >&2 || true
kubectl -n "${NAMESPACE}" describe pod "${pod}" >&2 || true
exit 1
