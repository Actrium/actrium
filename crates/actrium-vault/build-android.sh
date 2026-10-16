#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ANDROID_MODULE="${ANDROID_MODULE:-$REPO_ROOT/packages/actrium-platform/android}"
KOTLIN_DIR="$ANDROID_MODULE/src/main/kotlin"
JNI_LIBS_DIR="$ANDROID_MODULE/src/main/jniLibs"
UDL_FILE="src/actrium_vault.udl"

cd "$SCRIPT_DIR"

echo "==> Generating UniFFI Kotlin bindings for actrium-vault..."
mkdir -p "$KOTLIN_DIR"
cargo run --bin uniffi-bindgen generate "$UDL_FILE" --language kotlin --out-dir "$KOTLIN_DIR"

echo "==> Building Android native libraries for actrium-vault..."
mkdir -p "$JNI_LIBS_DIR"
cargo ndk \
    -P 26 \
    -t arm64-v8a \
    -t x86_64 \
    -o "$JNI_LIBS_DIR" \
    build --release

for abi in arm64-v8a x86_64; do
    cp "$JNI_LIBS_DIR/$abi/libactrium_vault.so" "$JNI_LIBS_DIR/$abi/libuniffi_actrium_vault.so"
done

echo "==> Done!"
echo "  Kotlin: $KOTLIN_DIR/uniffi/actrium_vault/actrium_vault.kt"
echo "  JNI:    $JNI_LIBS_DIR/{arm64-v8a,x86_64}/lib{actrium_vault,uniffi_actrium_vault}.so"
