#!/usr/bin/env bash
set -euo pipefail

# Build a Rust workload crate (staticlib, statically linking actr-cabi rlib +
# actr core) into an ActrCabi.xcframework for iOS / iOS-sim / macOS, packaged
# with the actr-cabi.h header + modulemap.
#
# Simpler than packages/actr-swift/build-xcframework.sh: no UniFFI, no generated
# Actr.swift, no actrFFI.c stub — the Swift wrapper (ActrCabi.swift) is
# hand-written and lives in the SPM package.
#
# Inputs:
#   ACTR_CABI_WORKLOAD_CRATE  path to a Rust workload crate (staticlib, uses
#                             `export_workload!`). Required (no default —
#                             actr-cabi alone has no registered workload).
#   ACTR_BUILD_PROFILE         debug | release (default release)
# Outputs:
#   dist/ActrCabi.xcframework

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${ROOT_DIR}/../../.." && pwd)"

if [[ -z "${ACTR_CABI_WORKLOAD_CRATE:-}" ]]; then
  echo "error: ACTR_CABI_WORKLOAD_CRATE must point at a workload crate" >&2
  echo "hint: ACTR_CABI_WORKLOAD_CRATE=$WORKSPACE_ROOT/path/to/staticlib-workload $0" >&2
  exit 1
fi
WORKLOAD_CRATE_DIR="$(cd "${ACTR_CABI_WORKLOAD_CRATE}" && pwd)"

FRAMEWORK_NAME="ActrCabi"
TARGET_DIR="${WORKSPACE_ROOT}/target"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-15.0}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
BUILD_PROFILE="${ACTR_BUILD_PROFILE:-release}"
PROFILE_DIR="${BUILD_PROFILE}"

case "$BUILD_PROFILE" in
  debug) CARGO_PROFILE_ARGS=() ;;
  release) CARGO_PROFILE_ARGS=(--release) ;;
  *)
    echo "error: ACTR_BUILD_PROFILE must be 'debug' or 'release'" >&2
    exit 1
    ;;
esac

HEADERS_DIR="${ROOT_DIR}/Sources/ActrCabi/include"
DIST_DIR="${ROOT_DIR}/dist"
XCFRAMEWORK_DIR="${DIST_DIR}/${FRAMEWORK_NAME}.xcframework"

require_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "error: missing $1" >&2; exit 1; }; }
require_cmd cargo
require_cmd xcodebuild
require_cmd rustc

echo "[1/3] Preparing headers"
mkdir -p "${HEADERS_DIR}"
cp "${WORKSPACE_ROOT}/crates/actr-cabi/actr-cabi.h" "${HEADERS_DIR}/actr-cabi.h"
# module.modulemap is checked into Sources/ActrCabi/include already.

echo "[2/3] Building Rust static libraries (iOS + sim + macOS, ARM64)"
mkdir -p "${TARGET_DIR}/actr-cabi-xcframework"

IOS_TARGET=aarch64-apple-ios
IOS_SIM_TARGET=aarch64-apple-ios-sim
MACOS_TARGET=aarch64-apple-darwin

build_one() {
  local target="$1"
  local target_dir="${TARGET_DIR}/actr-cabi-xcframework/${target}"
  echo "  → ${target}"
  (cd "${WORKSPACE_ROOT}" && env -u SDKROOT CARGO_TARGET_DIR="${target_dir}" \
    cargo build --manifest-path "${WORKLOAD_CRATE_DIR}/Cargo.toml" \
      "${CARGO_PROFILE_ARGS[@]}" --target "${target}")
}

build_one "${IOS_TARGET}"
build_one "${IOS_SIM_TARGET}"
build_one "${MACOS_TARGET}"

# The workload crate's staticlib bundles actr-cabi + all deps.
discover_lib() {
  local target="$1"
  local dir="${TARGET_DIR}/actr-cabi-xcframework/${target}/${target}/${PROFILE_DIR}"
  # workload crate lib name: lib<crate_name_with_underscores>.a
  local lib="$(find "${dir}" -maxdepth 1 -name 'lib*.a' -print -quit)"
  [[ -n "${lib}" ]] || { echo "error: no .a found in ${dir}" >&2; exit 1; }
  echo "${lib}"
}

IOS_LIB="$(discover_lib "${IOS_TARGET}")"
IOS_SIM_LIB="$(discover_lib "${IOS_SIM_TARGET}")"
MACOS_LIB="$(discover_lib "${MACOS_TARGET}")"

echo "[3/3] Creating XCFramework"
rm -rf "${XCFRAMEWORK_DIR}"
mkdir -p "${DIST_DIR}"
xcodebuild -create-xcframework \
  -library "${IOS_LIB}"      -headers "${HEADERS_DIR}" \
  -library "${IOS_SIM_LIB}"  -headers "${HEADERS_DIR}" \
  -library "${MACOS_LIB}"    -headers "${HEADERS_DIR}" \
  -output "${XCFRAMEWORK_DIR}"

echo ""
echo "✅ ${FRAMEWORK_NAME}.xcframework built at: ${XCFRAMEWORK_DIR}"
echo "   SwiftPM: ACTR_CABI_BINARY_PATH=dist/${FRAMEWORK_NAME}.xcframework swift build"
