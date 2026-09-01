#!/usr/bin/env bash
# Reproducibly rebuild only the Host API v0.1 Rust E2E fixture.
set -euo pipefail
cd "$(dirname "$0")"

TARGET=wasm32-unknown-unknown
TARGET_DIR=target/fixtures-v01-e2e
FIXTURE=../src/host/native-guest/__tests__/fixtures/host-api-v01-guest.wasm

RUSTFLAGS="--remap-path-prefix=$(pwd)=." \
  CARGO_TARGET_DIR="$TARGET_DIR" \
  cargo build -p host-api-v01-guest --target "$TARGET" --release
cp "$TARGET_DIR/$TARGET/release/host_api_v01_guest.wasm" "$FIXTURE"
echo "staged: $FIXTURE ($(wc -c < "$FIXTURE") bytes)"
