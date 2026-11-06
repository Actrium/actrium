#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PRODUCT_NAME="protoc-gen-actrframework-swift"
ARCH="arm64"
CONFIGURATION="release"

DIST_DIR="${DIST_DIR:-$ROOT_DIR/dist}"
BUILD_DIR="${BUILD_DIR:-$ROOT_DIR/.build}"

build_options=(--package-path "$ROOT_DIR" --scratch-path "$BUILD_DIR" -c "$CONFIGURATION" --arch "$ARCH")
swift build "${build_options[@]}" --product "$PRODUCT_NAME"
bin_dir=$(swift build "${build_options[@]}" --show-bin-path)
BIN_PATH="$bin_dir/$PRODUCT_NAME"
if [[ ! -x "$BIN_PATH" ]]; then
  echo "Release binary not found at $BIN_PATH." >&2
  exit 1
fi

mkdir -p "$DIST_DIR"
cp "$BIN_PATH" "$DIST_DIR/$PRODUCT_NAME"
chmod +x "$DIST_DIR/$PRODUCT_NAME"

ZIP_NAME="${PRODUCT_NAME}-macos-arm64.zip"
ZIP_PATH="$DIST_DIR/$ZIP_NAME"
rm -f "$ZIP_PATH"

(cd "$DIST_DIR" && zip -q "$ZIP_NAME" "$PRODUCT_NAME")

CHECKSUM_PATH="${ZIP_PATH}.sha256"
( cd "$DIST_DIR" && shasum -a 256 "$ZIP_NAME" ) > "$CHECKSUM_PATH"

echo "Release artifact: $ZIP_PATH"
echo "SHA256 file: $CHECKSUM_PATH"
