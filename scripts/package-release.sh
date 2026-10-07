#!/usr/bin/env bash
set -euo pipefail

version=${1:?"usage: package-release.sh VERSION TARGET"}
target=${2:?"usage: package-release.sh VERSION TARGET"}
binary="target/${target}/release/vkpl"

if [[ ! -f "$binary" ]]; then
  echo "Release binary not found: $binary" >&2
  exit 1
fi

mkdir -p dist
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT

install -m 755 "$binary" "$staging/vkpl"
archive="dist/vkpl-${version}-${target}.tar.gz"
tar -C "$staging" -czf "$archive" vkpl

if command -v sha256sum >/dev/null; then
  sha256sum "$archive" > "${archive}.sha256"
else
  shasum -a 256 "$archive" > "${archive}.sha256"
fi

printf 'Created %s\n' "$archive"
