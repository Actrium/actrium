#!/usr/bin/env bash

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
checksum_file="${repo_root}/.github/actrium-vendored-assets.sha256"

cd "${repo_root}"

if command -v sha256sum >/dev/null 2>&1; then
  sha256sum --check "${checksum_file}"
else
  shasum -a 256 --check "${checksum_file}"
fi
