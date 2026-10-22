#!/usr/bin/env bash
# Build the Rust library for Android targets and generate UniFFI Kotlin bindings.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${ROOT_DIR}/../.." && pwd)"
CRATE_DIR="${WORKSPACE_ROOT}/crates/actr-ffi"
MODULE_DIR="${ROOT_DIR}/actr-kotlin"
KOTLIN_SRC_DIR="${MODULE_DIR}/src/main/kotlin"
GENERATED_PACKAGE_DIR="${KOTLIN_SRC_DIR}/io/actrium/actr"
OLD_GENERATED_DIR="${MODULE_DIR}/src/main/kotlin/io/actor""_rtc/actr"
LIBRARY_JNILIBS_DIR="${MODULE_DIR}/src/main/jniLibs"
DEMO_JNILIBS_DIR="${ROOT_DIR}/demo/src/main/jniLibs"
TARGET_DIR="${WORKSPACE_ROOT}/target"
NDK_VERSION="${NDK_VERSION:-25.2.9519653}"
ANDROID_API_LEVEL="${ANDROID_API_LEVEL:-21}"
PROTOC_PATH="${PROTOC:-$(command -v protoc || true)}"
HOST_TARGET="$(rustc -vV | awk -F': ' '/^host:/{print $2}')"
ACTR_ANDROID_TARGETS="${ACTR_ANDROID_TARGETS:-aarch64-linux-android x86_64-linux-android}"
ACTR_BUILD_ANDROID_NATIVE="${ACTR_BUILD_ANDROID_NATIVE:-true}"
ACTR_BUILD_HOST_LIBRARY="${ACTR_BUILD_HOST_LIBRARY:-true}"
ACTR_GENERATE_KOTLIN_BINDINGS="${ACTR_GENERATE_KOTLIN_BINDINGS:-true}"
ACTR_COPY_DEMO_JNILIBS="${ACTR_COPY_DEMO_JNILIBS:-true}"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "error: missing required command: $1" >&2
        exit 1
    fi
}

require_dir() {
    if [[ ! -d "$1" ]]; then
        echo "error: missing required directory: $1" >&2
        exit 1
    fi
}

require_file() {
    if [[ ! -f "$1" ]]; then
        echo "error: missing required file: $1" >&2
        exit 1
    fi
}

resolve_android_sdk_root() {
    local candidate
    for candidate in \
        "${ANDROID_SDK_ROOT:-}" \
        "${ANDROID_HOME:-}" \
        "${HOME}/Android/Sdk" \
        "${HOME}/Library/Android/sdk"
    do
        if [[ -n "${candidate}" && -d "${candidate}" ]]; then
            printf '%s\n' "${candidate}"
            return 0
        fi
    done

    return 1
}

resolve_ndk_path() {
    local sdk_root=$1
    local candidate
    for candidate in \
        "${ANDROID_NDK_ROOT:-}" \
        "${sdk_root}/ndk/${NDK_VERSION}" \
        "${sdk_root}/ndk-bundle"
    do
        if [[ -n "${candidate}" && -d "${candidate}" ]]; then
            printf '%s\n' "${candidate}"
            return 0
        fi
    done

    return 1
}

resolve_toolchain_path() {
    local ndk_root=$1
    local candidates=(
        "${ndk_root}/toolchains/llvm/prebuilt/linux-x86_64"
        "${ndk_root}/toolchains/llvm/prebuilt/darwin-arm64"
        "${ndk_root}/toolchains/llvm/prebuilt/darwin-x86_64"
    )
    local candidate
    for candidate in "${candidates[@]}"; do
        if [[ -d "${candidate}" ]]; then
            printf '%s\n' "${candidate}"
            return 0
        fi
    done

    return 1
}

host_library_path() {
    local ext
    case "${HOST_TARGET}" in
        *apple-darwin) ext="dylib" ;;
        *windows-msvc) ext="dll" ;;
        *) ext="so" ;;
    esac
    printf '%s/%s/debug/libactr.%s\n' "${TARGET_DIR}" "${HOST_TARGET}" "${ext}"
}

target_abi_for() {
    case "$1" in
        aarch64-linux-android) printf 'arm64-v8a\n' ;;
        x86_64-linux-android) printf 'x86_64\n' ;;
        *) echo "error: unsupported Android target: $1" >&2; exit 1 ;;
    esac
}

target_enabled() {
    local target=$1
    local enabled_target
    for enabled_target in ${ACTR_ANDROID_TARGETS}; do
        if [[ "${enabled_target}" == "${target}" ]]; then
            return 0
        fi
    done
    return 1
}

copy_target_if_dir_exists() {
    local source_dir=$1
    local target_dir=$2
    local target=$3
    local abi
    abi=$(target_abi_for "${target}")

    if [[ ! -d "${source_dir}" ]]; then
        return 0
    fi

    mkdir -p "${target_dir}/${abi}"
    cp "${TARGET_DIR}/${target}/release/libactr.so" "${target_dir}/${abi}/"

    # Opus is statically linked by opusic-sys; remove stale shared copies.
    rm -f "${target_dir}/${abi}/libopus.so"
}

require_cmd cargo
require_cmd rustc
if [[ "${ACTR_GENERATE_KOTLIN_BINDINGS}" == true ]]; then
    require_cmd uniffi-bindgen
fi
require_cmd protoc
require_dir "${CRATE_DIR}"
require_dir "${MODULE_DIR}"
require_file "${CRATE_DIR}/Cargo.toml"
require_file "${CRATE_DIR}/uniffi.toml"

if [[ -z "${PROTOC_PATH}" ]]; then
    echo "error: protoc not found on PATH" >&2
    exit 1
fi

export PROTOC="${PROTOC_PATH}"
if [[ "${ACTR_BUILD_ANDROID_NATIVE}" == true ]]; then
    ANDROID_SDK_ROOT="$(resolve_android_sdk_root)" || {
        echo "error: Android SDK not found. Set ANDROID_SDK_ROOT or ANDROID_HOME." >&2
        exit 1
    }
    NDK_PATH="$(resolve_ndk_path "${ANDROID_SDK_ROOT}")" || {
        echo "error: Android NDK not found. Expected version ${NDK_VERSION} under ${ANDROID_SDK_ROOT}/ndk." >&2
        exit 1
    }
    TOOLCHAIN_PATH="$(resolve_toolchain_path "${NDK_PATH}")" || {
        echo "error: Android NDK LLVM toolchain not found under ${NDK_PATH}" >&2
        exit 1
    }

    # opusic-sys selects the NDK CMake toolchain and ABI through ANDROID_NDK_HOME.
    export ANDROID_NDK_ROOT="${NDK_PATH}"
    export ANDROID_NDK_HOME="${NDK_PATH}"
    export ANDROID_PLATFORM="android-${ANDROID_API_LEVEL}"

    export PATH="${TOOLCHAIN_PATH}/bin:${PATH}"
    export CC_aarch64_linux_android="${TOOLCHAIN_PATH}/bin/aarch64-linux-android${ANDROID_API_LEVEL}-clang"
    export AR_aarch64_linux_android="${TOOLCHAIN_PATH}/bin/llvm-ar"
    export RANLIB_aarch64_linux_android="${TOOLCHAIN_PATH}/bin/llvm-ranlib"
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${CC_aarch64_linux_android}"
    export CC_x86_64_linux_android="${TOOLCHAIN_PATH}/bin/x86_64-linux-android${ANDROID_API_LEVEL}-clang"
    export AR_x86_64_linux_android="${TOOLCHAIN_PATH}/bin/llvm-ar"
    export RANLIB_x86_64_linux_android="${TOOLCHAIN_PATH}/bin/llvm-ranlib"
    export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="${CC_x86_64_linux_android}"
    export ANDROID_TOOLCHAIN_PATH="${TOOLCHAIN_PATH}"
else
    ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-}"
    NDK_PATH="${ANDROID_NDK_ROOT:-}"
    TOOLCHAIN_PATH=""
fi

echo "========================================"
echo "Building ACTR Android Native Libraries"
echo "========================================"
echo ""
echo "Workspace: ${WORKSPACE_ROOT}"
echo "libactr crate: ${CRATE_DIR}"
echo "Android SDK: ${ANDROID_SDK_ROOT:-skipped}"
echo "Android NDK: ${NDK_PATH:-skipped}"
echo "Output (library): ${LIBRARY_JNILIBS_DIR}"
if [[ -d "${ROOT_DIR}/demo" ]]; then
    echo "Output (demo): ${DEMO_JNILIBS_DIR}"
fi
echo ""

HOST_LIBRARY_PATH="$(host_library_path)"
if [[ "${ACTR_BUILD_HOST_LIBRARY}" == true || "${ACTR_GENERATE_KOTLIN_BINDINGS}" == true ]]; then
    echo "Building host library for Kotlin UniFFI bindings..."
    (cd "${WORKSPACE_ROOT}" && cargo build -p libactr --target "${HOST_TARGET}")
    require_file "${HOST_LIBRARY_PATH}"
fi

# opusic-sys builds bundled libopus.a with the NDK toolchain and links it
# into libactr.so. No archive repacking or shared-library relink is needed.
if [[ "${ACTR_BUILD_ANDROID_NATIVE}" == true ]]; then
    echo ""
    echo "Building Android native libraries..."

    for target in ${ACTR_ANDROID_TARGETS}; do
        target_abi_for "${target}" >/dev/null
        echo ""
        echo "==> Building for ${target}..."
        (cd "${WORKSPACE_ROOT}" && cargo build -p libactr --release --target "${target}")
    done

    echo ""
    echo "Copying native libraries..."
    for target in ${ACTR_ANDROID_TARGETS}; do
        copy_target_if_dir_exists "${MODULE_DIR}" "${LIBRARY_JNILIBS_DIR}" "${target}"
        if [[ "${ACTR_COPY_DEMO_JNILIBS}" == true ]]; then
            copy_target_if_dir_exists "${ROOT_DIR}/demo" "${DEMO_JNILIBS_DIR}" "${target}"
        fi
    done
fi

if [[ "${ACTR_GENERATE_KOTLIN_BINDINGS}" == true ]]; then
    echo ""
    echo "Generating Kotlin bindings..."
    mkdir -p "${KOTLIN_SRC_DIR}"
    rm -f "${GENERATED_PACKAGE_DIR}/actr.kt"
    rm -rf "${GENERATED_PACKAGE_DIR}/io" "${OLD_GENERATED_DIR}"
    (cd "${CRATE_DIR}" && uniffi-bindgen generate --library "${HOST_LIBRARY_PATH}" --language kotlin --out-dir "${KOTLIN_SRC_DIR}")
    python3 "${WORKSPACE_ROOT}/scripts/actr/patch-error-messages.py" kotlin "${GENERATED_PACKAGE_DIR}/actr.kt"
fi

echo ""
echo "========================================"
echo "Build completed successfully!"
echo "========================================"
echo ""
echo "Library sizes (library module):"
if compgen -G "${LIBRARY_JNILIBS_DIR}/*/*.so" >/dev/null; then
    ls -lh "${LIBRARY_JNILIBS_DIR}"/*/*.so
else
    echo "No native libraries present."
fi
echo ""
echo "Next steps:"
echo "  1. Build the Android project: ./gradlew :actr-kotlin:assembleRelease"
if [[ -d "${ROOT_DIR}/demo" ]]; then
    echo "  2. Build demo app: ./gradlew :demo:assembleDebug"
fi
echo ""
