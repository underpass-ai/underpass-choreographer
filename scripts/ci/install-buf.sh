#!/usr/bin/env bash
set -euo pipefail

BUF_VERSION="${BUF_VERSION:-1.47.2}"

if command -v buf >/dev/null 2>&1; then
  buf --version
  exit 0
fi

OS="Linux"
ARCH="x86_64"
BASE_URL="https://github.com/bufbuild/buf/releases/download/v${BUF_VERSION}"
BIN="buf-${OS}-${ARCH}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

curl --fail --silent --show-error --location --proto "=https" --proto-redir "=https" \
  "${BASE_URL}/${BIN}" -o "${TMP_DIR}/buf"
curl --fail --silent --show-error --location --proto "=https" --proto-redir "=https" \
  "${BASE_URL}/sha256.txt" -o "${TMP_DIR}/sha256.txt"

(
  cd "${TMP_DIR}"
  grep " ${BIN}$" sha256.txt | awk '{print $1"  buf"}' | sha256sum -c -
)

install -m 0755 "${TMP_DIR}/buf" /usr/local/bin/buf
buf --version
