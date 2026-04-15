#!/usr/bin/env bash
set -euo pipefail

HELM_VERSION="${HELM_VERSION:-v3.19.0}"

if command -v helm >/dev/null 2>&1; then
  helm version --short
  exit 0
fi

OS="linux"
ARCH="amd64"
ARCHIVE="helm-${HELM_VERSION}-${OS}-${ARCH}.tar.gz"
BASE_URL="https://get.helm.sh"
HTTPS_ONLY="=https"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

curl --fail --silent --show-error --location --proto "${HTTPS_ONLY}" --proto-redir "${HTTPS_ONLY}" \
  "${BASE_URL}/${ARCHIVE}" -o "${TMP_DIR}/${ARCHIVE}"
curl --fail --silent --show-error --location --proto "${HTTPS_ONLY}" --proto-redir "${HTTPS_ONLY}" \
  "${BASE_URL}/${ARCHIVE}.sha256sum" -o "${TMP_DIR}/${ARCHIVE}.sha256sum"

(
  cd "${TMP_DIR}"
  sha256sum -c "${ARCHIVE}.sha256sum"
)

tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "${TMP_DIR}"
install -m 0755 "${TMP_DIR}/${OS}-${ARCH}/helm" /usr/local/bin/helm
helm version --short
