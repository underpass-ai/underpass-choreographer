#!/usr/bin/env bash
set -euo pipefail

if command -v asyncapi >/dev/null 2>&1; then
  asyncapi --version
  exit 0
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "npm is required to install @asyncapi/cli" >&2
  exit 1
fi

if command -v sudo >/dev/null 2>&1; then
  sudo npm install -g @asyncapi/cli@2
else
  npm install -g @asyncapi/cli@2
fi

asyncapi --version
