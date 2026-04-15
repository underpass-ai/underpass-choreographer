#!/usr/bin/env bash
set -euo pipefail

if command -v protoc >/dev/null 2>&1; then
  protoc --version
  exit 0
fi

if ! command -v apt-get >/dev/null 2>&1; then
  echo "protoc is required but apt-get is unavailable on this runner" >&2
  exit 1
fi

export DEBIAN_FRONTEND=noninteractive

if command -v sudo >/dev/null 2>&1; then
  sudo apt-get update
  sudo apt-get install -y protobuf-compiler
else
  apt-get update
  apt-get install -y protobuf-compiler
fi

protoc --version
