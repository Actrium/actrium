#!/usr/bin/env bash
# Focused native host checks that need neither Pods nor the Rust frameworks.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/actrium-host-tests.XXXXXX")"
trap 'rm -rf "${TEST_DIR}"' EXIT

for script in build-app.sh sign-app.sh package-dmg.sh; do
  bash -n "${SCRIPT_DIR}/${script}"
done
python3 "${SCRIPT_DIR}/tests/test_sign_app.py"
xcrun swiftc -swift-version 5 -framework AppKit -framework QuartzCore \
  "${PROJECT_ROOT}/ActriumViewService/RendererLayerHosting.swift" \
  "${PROJECT_ROOT}/Tests/RendererLayerHostingRegression.swift" \
  -o "${TEST_DIR}/renderer-layer-hosting"
"${TEST_DIR}/renderer-layer-hosting"
xcrun swiftc -swift-version 5 -framework AppKit \
  "${PROJECT_ROOT}/ActriumViewService/RendererMouseTracking.swift" \
  "${PROJECT_ROOT}/Tests/RendererMouseTrackingRegression.swift" \
  -o "${TEST_DIR}/renderer-mouse-tracking"
"${TEST_DIR}/renderer-mouse-tracking"

# Exercise the Realm refresh models without building the full macOS app.
REALM_TEST_DIR="${TEST_DIR}/RealmModels"
mkdir -p "${REALM_TEST_DIR}/Sources/Actrium" "${REALM_TEST_DIR}/Tests/ActriumTests"
cp "${PROJECT_ROOT}"/Actrium/Models/{ActrLinkInvite,RealmFrontTrustStore,RealmFrontUpdate}.swift \
  "${REALM_TEST_DIR}/Sources/Actrium/"
cp "${PROJECT_ROOT}"/ActriumTests/{ActrLinkInviteTests,RealmFrontTrustStoreTests,RealmFrontUpdateTests}.swift \
  "${REALM_TEST_DIR}/Tests/ActriumTests/"
cat > "${REALM_TEST_DIR}/Package.swift" <<'SWIFT'
// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "RealmModels",
    platforms: [.macOS(.v15)],
    targets: [
        .target(name: "Actrium"),
        .testTarget(name: "ActriumTests", dependencies: ["Actrium"])
    ]
)
SWIFT
xcrun swift test --package-path "${REALM_TEST_DIR}"
