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
IPC_SOURCES="${PROJECT_ROOT}/../packages/actrium-platform/apple/ActriumIPC/Sources/ActriumIPC"
xcrun swiftc -module-cache-path "${TEST_DIR}/clang" -swift-version 5 -parse-as-library \
  -module-name ActriumIPC -emit-module -emit-module-path "${TEST_DIR}/ActriumIPC.swiftmodule" \
  -whole-module-optimization -emit-object -o "${TEST_DIR}/attachment-ipc.o" \
  "${IPC_SOURCES}/RendererAttachmentDirectory.swift" "${IPC_SOURCES}/GuestFileImportCall.swift"
xcrun swiftc -module-cache-path "${TEST_DIR}/clang" -swift-version 5 -I "${TEST_DIR}" \
  "${TEST_DIR}/attachment-ipc.o" \
  "${PROJECT_ROOT}/Actrium/MultiProcess/RendererAttachmentThumbnail.swift" \
  "${PROJECT_ROOT}/ActriumViewService/RendererAttachmentCache.swift" \
  "${PROJECT_ROOT}/Tests/RendererAttachmentCacheRegression.swift" \
  -o "${TEST_DIR}/renderer-attachment-cache"
"${TEST_DIR}/renderer-attachment-cache"

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
