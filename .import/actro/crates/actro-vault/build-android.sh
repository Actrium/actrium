#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ANDROID_MODULE="${ANDROID_MODULE:-$REPO_ROOT/packages/actro-native-kotlin}"
KOTLIN_DIR="$ANDROID_MODULE/src/main/kotlin"
JNI_LIBS_DIR="$ANDROID_MODULE/src/main/jniLibs"
UDL_FILE="src/actro_vault.udl"

cd "$SCRIPT_DIR"

echo "==> Generating UniFFI Kotlin bindings for actro-vault..."
mkdir -p "$KOTLIN_DIR"
cargo run --bin uniffi-bindgen generate "$UDL_FILE" --language kotlin --out-dir "$KOTLIN_DIR"

echo "==> Building Android native libraries for actro-vault..."
mkdir -p "$JNI_LIBS_DIR"
cargo ndk \
    -P 26 \
    -t arm64-v8a \
    -t x86_64 \
    -o "$JNI_LIBS_DIR" \
    build --release

for abi in arm64-v8a x86_64; do
    cp "$JNI_LIBS_DIR/$abi/libactro_vault.so" "$JNI_LIBS_DIR/$abi/libuniffi_actro_vault.so"
done

echo "==> Done!"
echo "  Kotlin: $KOTLIN_DIR/uniffi/actro_vault/actro_vault.kt"
echo "  JNI:    $JNI_LIBS_DIR/{arm64-v8a,x86_64}/lib{actro_vault,uniffi_actro_vault}.so"
