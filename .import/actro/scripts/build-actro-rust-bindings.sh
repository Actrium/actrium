#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
    cat <<'USAGE'
Usage: scripts/build-actro-rust-bindings.sh [runtime|apple|android|windows|all]

Builds generated native bindings and local binary artifacts for Actro Rust
client crates.

Targets:
  runtime  Generate TypeScript runtime contracts from actro-core.
  apple    Build Apple XCFrameworks and Swift bindings.
  android  Build ActroCore/ActroVault Kotlin bindings and JNI libraries.
  windows  Build the ActroCore C ABI DLL used by the Windows WebView2 shell.
  all      Build runtime, apple, and android targets. This is the default.
USAGE
}

target="${1:-all}"

case "$target" in
    runtime|apple|android|windows|all)
        ;;
    -h|--help|help)
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac

build_runtime() {
    cargo run -p actro-core --bin generate-ts-contracts -- \
        "$REPO_ROOT/packages/actro-runtime/src/actr-host/generated/actroCoreContract.ts"
}

build_apple() {
    if [[ "$(uname -s)" != "Darwin" ]]; then
        echo "Apple bindings require macOS with Xcode." >&2
        exit 1
    fi

    "$REPO_ROOT/crates/actro-terminal/build-xcframework.sh"
    "$REPO_ROOT/crates/actro-core/build-xcframework.sh"
    "$REPO_ROOT/crates/actro-vault/build-xcframework.sh"
}

build_android() {
    if ! command -v cargo-ndk >/dev/null 2>&1; then
        echo "cargo-ndk is required. Install it with: cargo install cargo-ndk" >&2
        exit 1
    fi

    cargo run -p actro-core --bin generate-android-contracts -- \
        "$REPO_ROOT/apps/android/actro/android/app/src/main/java/io/actrium/actro/generated/ActroAndroidPersistenceContract.kt"
    "$REPO_ROOT/crates/actro-core/build-android.sh"
    "$REPO_ROOT/crates/actro-vault/build-android.sh"
}

build_windows() {
    case "$(uname -s 2>/dev/null || echo unknown)" in
        MINGW*|MSYS*|CYGWIN*)
            ;;
        *)
            if [[ "${OS:-}" != "Windows_NT" ]]; then
                echo "Windows native artifacts require a Windows Rust host." >&2
                exit 1
            fi
            ;;
    esac

    cargo build -p actro-core --release

    local source_dll="$REPO_ROOT/target/release/actro_core.dll"
    local dest_dir="$REPO_ROOT/apps/windows/actro/native"

    if [[ ! -f "$source_dll" ]]; then
        echo "expected Windows DLL was not produced: $source_dll" >&2
        exit 1
    fi

    mkdir -p "$dest_dir"
    cp "$source_dll" "$dest_dir/actro_core.dll"

    echo "==> Done!"
    echo "  DLL: ${dest_dir}/actro_core.dll"
    echo "  C API header: ${REPO_ROOT}/crates/actro-core/include/actro_core.h"
}

case "$target" in
    runtime)
        build_runtime
        ;;
    apple)
        build_apple
        ;;
    android)
        build_android
        ;;
    windows)
        build_windows
        ;;
    all)
        build_runtime
        build_apple
        build_android
        ;;
esac
