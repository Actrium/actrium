#!/usr/bin/env bash
set -euo pipefail

if [ -z "${VCPKG_ROOT:-}" ]; then
  echo "VCPKG_ROOT is required, for example: VCPKG_ROOT=/Users/example-dev-2/vcpkg" >&2
  exit 1
fi

"$VCPKG_ROOT/vcpkg" install \
  libyuv:arm64-osx \
  libvpx:arm64-osx \
  aom:arm64-osx
