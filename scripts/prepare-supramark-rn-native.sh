#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SUPRAMARK_ROOT="${ROOT}/packages/supramark"
GRAPHVIZ_ROOT="${SUPRAMARK_ROOT}/crates/graphviz-anywhere"

IOS_MIN_VERSION="${IOS_MIN_VERSION:-15.1}"
ANDROID_API="${ANDROID_API:-23}"

usage() {
  cat <<'EOF'
Usage: scripts/prepare-supramark-rn-native.sh [--ios|--android|--all]

Build and stage Supramark React Native native libraries.

Options:
  --ios      Build only iOS libraries.
  --android  Build only Android libraries.
  --all      Build both iOS and Android libraries (default).
  -h, --help Show this help message.

Environment:
  IOS_MIN_VERSION      Minimum iOS version (default: 15.1).
  ANDROID_API          Android API level (default: 23).
  SKIP_GRAPHVIZ_BUILD  Set to 1 to skip Graphviz builds.
EOF
}

BUILD_IOS=0
BUILD_ANDROID=0

if [[ $# -eq 0 ]]; then
  BUILD_IOS=1
  BUILD_ANDROID=1
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ios)
      BUILD_IOS=1
      ;;
    --android)
      BUILD_ANDROID=1
      ;;
    --all)
      BUILD_IOS=1
      BUILD_ANDROID=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

find_android_ndk() {
  if [[ -n "${ANDROID_NDK_HOME:-}" && -f "${ANDROID_NDK_HOME}/source.properties" ]]; then
    printf '%s\n' "${ANDROID_NDK_HOME}"
    return
  fi

  local bases=(
    "${HOME}/Library/Android/sdk/ndk"
    "${HOME}/Android/Sdk/ndk"
  )
  if [[ -n "${ANDROID_HOME:-}" ]]; then
    bases+=("${ANDROID_HOME}/ndk")
  fi

  local base
  for base in "${bases[@]}"; do
    [[ -d "${base}" ]] || continue
    local found
    found="$(
      find "${base}" -mindepth 1 -maxdepth 1 -type d \
        -exec test -f "{}/source.properties" \; -print | sort | tail -n 1
    )"
    if [[ -n "${found}" ]]; then
      printf '%s\n' "${found}"
      return
    fi
  done
}

if [[ "${BUILD_ANDROID}" == "1" ]]; then
  ANDROID_NDK_HOME="$(find_android_ndk)"
  if [[ -z "${ANDROID_NDK_HOME}" || ! -d "${ANDROID_NDK_HOME}" ]]; then
    echo "ANDROID_NDK_HOME is required. Install Android NDK or export ANDROID_NDK_HOME." >&2
    exit 1
  fi
  export ANDROID_NDK_HOME
  export ANDROID_NDK_ROOT="${ANDROID_NDK_HOME}"

  if ! cargo ndk --version >/dev/null 2>&1; then
    echo "cargo-ndk is required. Install with: cargo install cargo-ndk" >&2
    exit 1
  fi
fi

echo "==> Rust targets"
rust_targets=()
if [[ "${BUILD_IOS}" == "1" ]]; then
  rust_targets+=(
    aarch64-apple-ios
    aarch64-apple-ios-sim
    x86_64-apple-ios
  )
fi
if [[ "${BUILD_ANDROID}" == "1" ]]; then
  rust_targets+=(
    aarch64-linux-android
    armv7-linux-androideabi
    x86_64-linux-android
    i686-linux-android
  )
fi
rustup target add "${rust_targets[@]}"

if [[ "${SKIP_GRAPHVIZ_BUILD:-0}" != "1" ]]; then
  echo "==> Graphviz native libraries"
  (
    cd "${GRAPHVIZ_ROOT}"
    if [[ "${BUILD_IOS}" == "1" ]]; then
      IOS_MIN_VERSION="${IOS_MIN_VERSION}" ./scripts/build-ios.sh
    fi
    if [[ "${BUILD_ANDROID}" == "1" ]]; then
      ANDROID_API="${ANDROID_API}" ./scripts/build-android.sh
    fi
    node packages/react-native/scripts/prepare-native.js
  )
else
  echo "==> Graphviz native libraries: skipped"
fi

echo "==> Supramark React Native native crates"
(
  cd "${SUPRAMARK_ROOT}"
  export GRAPHVIZ_ANYWHERE_ALLOW_DOWNLOAD=0
  export IPHONEOS_DEPLOYMENT_TARGET="${IOS_MIN_VERSION}"
  export IOS_DEPLOYMENT_TARGET="${IOS_MIN_VERSION}"

  pkgs=(
    -p supramark-markdown-native
    -p supramark-d2-native
    -p supramark-mermaid-native
    -p supramark-plantuml-native
  )

  if [[ "${BUILD_IOS}" == "1" ]]; then
    for target in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
      echo "== cargo build ${target}"
      cargo build --release --target "${target}" "${pkgs[@]}"
    done

    mkdir -p target/ios-sim-universal/release
    for lib in \
      libsupramark_markdown_native.a \
      libsupramark_d2_native.a \
      libsupramark_mermaid_native.a \
      libsupramark_plantuml_native.a; do
      echo "== lipo ${lib}"
      lipo -create \
        "target/aarch64-apple-ios-sim/release/${lib}" \
        "target/x86_64-apple-ios/release/${lib}" \
        -output "target/ios-sim-universal/release/${lib}"
    done

    scripts/build-ios-xcframework.sh \
      supramark-markdown-native \
      crates/supramark-markdown/packages/native/include \
      libsupramark_markdown_native.a \
      target/ios-xcframeworks/SupramarkMarkdown.xcframework
    scripts/build-ios-xcframework.sh \
      supramark-d2-native \
      crates/d2-little/packages/native/include \
      libsupramark_d2_native.a \
      target/ios-xcframeworks/SupramarkD2.xcframework
    scripts/build-ios-xcframework.sh \
      supramark-mermaid-native \
      crates/mermaid-little/packages/native/include \
      libsupramark_mermaid_native.a \
      target/ios-xcframeworks/SupramarkMermaid.xcframework
    scripts/build-ios-xcframework.sh \
      supramark-plantuml-native \
      crates/plantuml-little/packages/native/include \
      libsupramark_plantuml_native.a \
      target/ios-xcframeworks/SupramarkPlantuml.xcframework
  fi

  if [[ "${BUILD_ANDROID}" == "1" ]]; then
    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 build --release "${pkgs[@]}"
  fi

  node crates/supramark-markdown/packages/react-native/scripts/prepare-native.js
  node crates/d2-little/packages/react-native/scripts/prepare-native.js
  node crates/mermaid-little/packages/react-native/scripts/prepare-native.js
  node crates/plantuml-little/packages/react-native/scripts/prepare-native.js
)

echo "prepare-supramark-rn-native: done"
