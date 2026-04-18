#!/usr/bin/env bash
set -euo pipefail

# Build the provider-E2E runner image. The image isn't part of the
# product distribution, so it doesn't get published by the
# publish-distribution workflow — operators build it locally (or in
# a pre-release step) and push to whatever registry the cluster can
# pull from. Defaults to a local tag so `kubectl` + a cluster with
# the registry mirrored (kind, podman-desktop, etc.) can pull it.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE_TAG="${IMAGE_TAG:-underpass-choreographer-e2e-provider:dev}"
DOCKERFILE="${ROOT_DIR}/tests/e2e/provider-runner.Dockerfile"

cd "${ROOT_DIR}"

if command -v podman >/dev/null 2>&1; then
    build_cmd=(podman build)
elif command -v docker >/dev/null 2>&1; then
    build_cmd=(docker build)
else
    echo "error: neither podman nor docker is available" >&2
    exit 1
fi

echo ">>> building ${IMAGE_TAG} via ${build_cmd[*]}"
"${build_cmd[@]}" -f "${DOCKERFILE}" -t "${IMAGE_TAG}" .
echo ">>> done. Tag as needed + push to a registry reachable by your cluster."
