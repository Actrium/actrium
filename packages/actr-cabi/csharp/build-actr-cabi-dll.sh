#!/usr/bin/env bash
set -euo pipefail

# Build a Rust workload crate (cdylib, statically linking actr-cabi rlib +
# actr core) into a per-target actr_cabi native lib (.dll/.dylib/.so) for
# .NET consumers (WinUI/WPF/Console). Mirrors
# bindings/cabi/swift/build-xcframework.sh but produces flat native libs
# instead of an xcframework — .NET P/Invoke loads them at runtime.
#
# Inputs:
#   ACTR_CABI_WORKLOAD_CRATE  path to a Rust workload crate (cdylib, uses
#                             `export_workload!`). Required.
#   ACTR_CABI_TARGETS         space-separated rust targets (default: host).
#                             e.g. "x86_64-pc-windows-msvc aarch64-apple-darwin"
#   ACTR_BUILD_PROFILE         debug | release (default release)
# Outputs:
#   dist/actr_cabi.{dll,dylib,so}  per target + dist/actr-cabi.h

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${ROOT_DIR}/../../.." && pwd)"

if [[ -z "${ACTR_CABI_WORKLOAD_CRATE:-}" ]]; then
  echo "error: ACTR_CABI_WORKLOAD_CRATE must point at a workload crate" >&2
  echo "hint: ACTR_CABI_WORKLOAD_CRATE=../../e2e/cabi-app/workload $0" >&2
  exit 1
fi
WORKLOAD_CRATE="$(cd "${ACTR_CABI_WORKLOAD_CRATE}" && pwd)"

DIST_DIR="${ROOT_DIR}/dist"
BUILD_PROFILE="${ACTR_BUILD_PROFILE:-release}"
HOST_TARGET="$(rustc -vV | awk '/host:/ {print $2}')"
TARGETS="${ACTR_CABI_TARGETS:-$HOST_TARGET}"
TARGET_DIR="${CARGO_TARGET_DIR:-${WORKSPACE_ROOT}/target}"
mkdir -p "$DIST_DIR"
mkdir -p "$TARGET_DIR"
TARGET_DIR="$(cd "$TARGET_DIR" && pwd)"

case "$BUILD_PROFILE" in
  debug) CARGO_PROFILE_ARGS=() ;;
  release) CARGO_PROFILE_ARGS=(--release) ;;
  *)
    echo "error: ACTR_BUILD_PROFILE must be 'debug' or 'release'" >&2
    exit 1
    ;;
esac

require_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "error: missing $1" >&2; exit 1; }; }
require_cmd cargo
require_cmd rustc

for target in $TARGETS; do
  rustup target add "$target" 2>/dev/null || true
  echo "→ building $target (${BUILD_PROFILE})"
  (cd "${WORKSPACE_ROOT}" && env -u SDKROOT CARGO_TARGET_DIR="${TARGET_DIR}" \
    cargo build --manifest-path "${WORKLOAD_CRATE}/Cargo.toml" \
      "${CARGO_PROFILE_ARGS[@]}" --target "${target}")

  # The workload crate's cdylib bundles actr-cabi + all deps.
  lib_dir="${TARGET_DIR}/${target}/${BUILD_PROFILE}"
  lib="$(find "${lib_dir}" -maxdepth 1 \( -name '*.dll' -o -name '*.dylib' -o -name '*.so' \) -print -quit)"
  [[ -n "${lib}" ]] || { echo "error: no cdylib found in ${lib_dir}" >&2; exit 1; }
  ext="${lib##*.}"
  cp "${lib}" "${DIST_DIR}/actr_cabi.${ext}"
  echo "  ✅ ${DIST_DIR}/actr_cabi.${ext}"
done

cp "${WORKSPACE_ROOT}/crates/actr-cabi/actr-cabi.h" "${DIST_DIR}/actr-cabi.h"

echo ""
echo "✅ Built native libs at: ${DIST_DIR}"
echo "   C# app: copy actr_cabi.{dll,dylib,so} next to the .exe, reference ActrSharp."
echo "   Linker: add /WHOLEARCHIVE:actr_cabi (MSBuild) or -Wl,-all_load (macOS) to"
echo "   keep export_workload! inventory entries (else workload_create returns null)."
