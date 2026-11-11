#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

CRATE_NAME="actrium_core"
FRAMEWORK_NAME="ActriumCore"
UDL_FILE="src/actrium_core.udl"
GENERATED_DIR="generated"
STAGING_DIR="xcframework-staging"
TARGET_DIR="${CARGO_TARGET_DIR:-$(cd "$SCRIPT_DIR/../.." && pwd)/target}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"
APPLE_PACKAGE_SWIFT="${SCRIPT_DIR}/../../packages/actrium-platform/apple/${FRAMEWORK_NAME}/Sources/${FRAMEWORK_NAME}/${CRATE_NAME}.swift"
MACOS_ONLY=0
if [[ "${1:-}" == "--macos-only" ]]; then
    MACOS_ONLY=1
fi

echo "==> Building for aarch64-apple-darwin..."
cargo build --release --lib --target aarch64-apple-darwin

echo "==> Building for x86_64-apple-darwin..."
cargo build --release --lib --target x86_64-apple-darwin

if [[ "$MACOS_ONLY" == "0" ]]; then
    echo "==> Building for aarch64-apple-ios..."
    cargo build --release --lib --target aarch64-apple-ios

    echo "==> Building for aarch64-apple-ios-sim..."
    cargo build --release --lib --target aarch64-apple-ios-sim

    echo "==> Building for x86_64-apple-ios..."
    cargo build --release --lib --target x86_64-apple-ios
fi

echo "==> Generating UniFFI Swift bindings..."
mkdir -p "$GENERATED_DIR"
cargo run --bin uniffi-bindgen generate "$UDL_FILE" --language swift --out-dir "$GENERATED_DIR"
perl -pi -e 's/[ \t]+$//' \
    "${GENERATED_DIR}/${CRATE_NAME}.swift" \
    "${GENERATED_DIR}/${CRATE_NAME}FFI.h"
perl -0777 -pi -e 's/\n+\z/\n/' "${GENERATED_DIR}/${CRATE_NAME}FFI.h"
mkdir -p "$(dirname "$APPLE_PACKAGE_SWIFT")"
cp "${GENERATED_DIR}/${CRATE_NAME}.swift" "$APPLE_PACKAGE_SWIFT"

echo "==> Creating universal static library..."
rm -rf "$STAGING_DIR"
mkdir -p "${STAGING_DIR}/macos"
lipo -create \
    "${TARGET_DIR}/aarch64-apple-darwin/release/lib${CRATE_NAME}.a" \
    "${TARGET_DIR}/x86_64-apple-darwin/release/lib${CRATE_NAME}.a" \
    -output "${STAGING_DIR}/macos/lib${CRATE_NAME}.a"

if [[ "$MACOS_ONLY" == "0" ]]; then
    mkdir -p "${STAGING_DIR}/ios" "${STAGING_DIR}/ios-simulator"
    cp "${TARGET_DIR}/aarch64-apple-ios/release/lib${CRATE_NAME}.a" \
        "${STAGING_DIR}/ios/lib${CRATE_NAME}.a"

    lipo -create \
        "${TARGET_DIR}/aarch64-apple-ios-sim/release/lib${CRATE_NAME}.a" \
        "${TARGET_DIR}/x86_64-apple-ios/release/lib${CRATE_NAME}.a" \
        -output "${STAGING_DIR}/ios-simulator/lib${CRATE_NAME}.a"
fi

echo "==> Creating XCFramework..."
rm -rf "${FRAMEWORK_NAME}.xcframework"
mkdir -p "${STAGING_DIR}/Headers/${CRATE_NAME}FFI"

cp "${GENERATED_DIR}/${CRATE_NAME}FFI.h" "${STAGING_DIR}/Headers/${CRATE_NAME}FFI/"
cp "${GENERATED_DIR}/${CRATE_NAME}FFI.modulemap" "${STAGING_DIR}/Headers/${CRATE_NAME}FFI/module.modulemap"

if [[ "$MACOS_ONLY" == "1" ]]; then
    xcodebuild -create-xcframework \
        -library "${STAGING_DIR}/macos/lib${CRATE_NAME}.a" \
        -headers "${STAGING_DIR}/Headers" \
        -output "${FRAMEWORK_NAME}.xcframework"
else
    xcodebuild -create-xcframework \
        -library "${STAGING_DIR}/macos/lib${CRATE_NAME}.a" \
        -headers "${STAGING_DIR}/Headers" \
        -library "${STAGING_DIR}/ios/lib${CRATE_NAME}.a" \
        -headers "${STAGING_DIR}/Headers" \
        -library "${STAGING_DIR}/ios-simulator/lib${CRATE_NAME}.a" \
        -headers "${STAGING_DIR}/Headers" \
        -output "${FRAMEWORK_NAME}.xcframework"
fi

rm -rf "$STAGING_DIR"

echo "==> Done!"
echo "  XCFramework: ${FRAMEWORK_NAME}.xcframework/"
echo "  Swift file:  ${APPLE_PACKAGE_SWIFT}"
