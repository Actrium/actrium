#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ANDROID_MODULE="${ANDROID_MODULE:-$REPO_ROOT/packages/actro-native-kotlin}"
KOTLIN_DIR="$ANDROID_MODULE/src/main/kotlin"
JNI_LIBS_DIR="$ANDROID_MODULE/src/main/jniLibs"
UDL_FILE="src/actro_core.udl"
GENERATED_KOTLIN="$KOTLIN_DIR/uniffi/actro_core/actro_core.kt"

cd "$SCRIPT_DIR"

echo "==> Generating UniFFI Kotlin bindings for actro-core..."
mkdir -p "$KOTLIN_DIR"
cargo run --bin uniffi-bindgen generate "$UDL_FILE" --language kotlin --out-dir "$KOTLIN_DIR"
if [[ -f "$GENERATED_KOTLIN" ]]; then
    perl -i -pe 's/[ \t]+$//' "$GENERATED_KOTLIN"
fi

echo "==> Building Android native libraries for actro-core..."
mkdir -p "$JNI_LIBS_DIR"
cargo ndk \
    -P 26 \
    -t arm64-v8a \
    -t x86_64 \
    -o "$JNI_LIBS_DIR" \
    build --release

for abi in arm64-v8a x86_64; do
    cp "$JNI_LIBS_DIR/$abi/libactro_core.so" "$JNI_LIBS_DIR/$abi/libuniffi_actro_core.so"
done

echo "==> Done!"
echo "  Kotlin: $GENERATED_KOTLIN"
echo "  JNI:    $JNI_LIBS_DIR/{arm64-v8a,x86_64}/lib{actro_core,uniffi_actro_core}.so"
