#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ANDROID_MODULE="${ANDROID_MODULE:-$REPO_ROOT/packages/actrium-platform/android}"
KOTLIN_DIR="$ANDROID_MODULE/src/main/kotlin"
JNI_LIBS_DIR="$ANDROID_MODULE/src/main/jniLibs"
UDL_FILE="src/actrium_core.udl"
GENERATED_KOTLIN="$KOTLIN_DIR/uniffi/actrium_core/actrium_core.kt"

cd "$SCRIPT_DIR"

echo "==> Generating UniFFI Kotlin bindings for actrium-core..."
mkdir -p "$KOTLIN_DIR"
cargo run --bin uniffi-bindgen generate "$UDL_FILE" --language kotlin --out-dir "$KOTLIN_DIR"
perl -pi -e 's/[ \t]+$//' "$GENERATED_KOTLIN"
if [[ -f "$GENERATED_KOTLIN" ]]; then
    perl -i -pe 's/[ \t]+$//' "$GENERATED_KOTLIN"
fi

echo "==> Building Android native libraries for actrium-core..."
mkdir -p "$JNI_LIBS_DIR"
cargo ndk \
    -P 26 \
    -t arm64-v8a \
    -t x86_64 \
    -o "$JNI_LIBS_DIR" \
    build --release

for abi in arm64-v8a x86_64; do
    cp "$JNI_LIBS_DIR/$abi/libactrium_core.so" "$JNI_LIBS_DIR/$abi/libuniffi_actrium_core.so"
done

echo "==> Done!"
echo "  Kotlin: $GENERATED_KOTLIN"
echo "  JNI:    $JNI_LIBS_DIR/{arm64-v8a,x86_64}/lib{actrium_core,uniffi_actrium_core}.so"
