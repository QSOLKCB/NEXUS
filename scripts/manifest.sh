#!/usr/bin/env sh
# Regenerate MANIFEST.sha256 over every source file (relative paths).
set -eu
cd "$(dirname "$0")/.."
find . \( -path ./target -o -path ./.git -o -path ./.openai -o -name __pycache__ -o -path './nexus-verify-*' \) -prune -o -type f ! -name MANIFEST.sha256 ! -name '*.zip' ! -name '*.pyc' -print0 \
  | sort -z | xargs -0 sha256sum > MANIFEST.sha256
wc -l MANIFEST.sha256
