#!/usr/bin/env bash
# Focused native host checks that need neither Pods nor the Rust frameworks.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/actrium-host-tests.XXXXXX")"
trap 'rm -rf "${TEST_DIR}"' EXIT

bash -n "${SCRIPT_DIR}/build-app.sh" "${SCRIPT_DIR}/sign-app.sh" "${SCRIPT_DIR}/package-dmg.sh"
python3 "${SCRIPT_DIR}/tests/test_sign_app.py"
xcrun swiftc -swift-version 5 -framework AppKit -framework QuartzCore \
  "${PROJECT_ROOT}/ActriumViewService/RendererLayerHosting.swift" \
  "${PROJECT_ROOT}/Tests/RendererLayerHostingRegression.swift" \
  -o "${TEST_DIR}/renderer-layer-hosting"
"${TEST_DIR}/renderer-layer-hosting"
