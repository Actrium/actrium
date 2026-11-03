#!/bin/bash
set -euo pipefail

echo "Building Service Worker Host..."

# Build from the sw-host crate directory with its own workspace manifest.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Use the repository's canonical target directory when the helper is installed.
if [ -x /usr/local/bin/rust-target-env ]; then
  eval "$(/usr/local/bin/rust-target-env)"
fi

# Avoid wasm-pack's downloader on the Linux fleet's proxied egress.
if [ "$(uname -s)/$(uname -m)" = "Linux/x86_64" ]; then
  bash "$SCRIPT_DIR/../../../../tools/ci/runner/install-wasm-bindgen.sh" ../../Cargo.lock
fi

# Ensure a writable temp directory for wasm-bindgen install.
TMPDIR="${TMPDIR:-$(pwd)/.tmp}"
export TMPDIR
mkdir -p "$TMPDIR"

rm -rf ../../dist/sw

# Use the committed wasm32 flags instead of the runner's native RUSTFLAGS.
# Preserve the release + bindgen pipeline without an extra Binaryen pass.
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS wasm-pack build \
  --target no-modules \
  --out-dir ../../dist/sw \
  --out-name actr_sw_host \
  --release \
  --no-opt \
  -- --locked

# Generate npm package metadata
cat > ../../dist/sw/package.json << EOF
{
  "name": "@actor-rtc/sw-host",
  "version": "0.1.0",
  "description": "Actor-RTC Service Worker Host (Component Model bridge + runtime)",
  "main": "actr_sw_host.js",
  "types": "actr_sw_host.d.ts",
  "files": [
    "actr_sw_host_bg.wasm",
    "actr_sw_host.js",
    "actr_sw_host.d.ts"
  ]
}
EOF

echo "✓ Service Worker Host built successfully"
echo "  Output: dist/sw/"
ls -lh ../../dist/sw/

# Sync the freshly built wasm + JS glue into crates/actr-cli/assets/web-runtime/, where
# the actr CLI embeds them via include_bytes!. Without this step, edits to
# sw-host source silently drift from what `actr run --web` serves — see
# packages/actr-web/docs/tech-debt.zh.md TD-002 for context.
SYNC_SCRIPT="$SCRIPT_DIR/../../scripts/sync-cli-assets.sh"
if [ "${ACTR_SKIP_CLI_ASSET_SYNC:-0}" = "1" ]; then
  echo
  echo "(skip sync: ACTR_SKIP_CLI_ASSET_SYNC=1)"
elif [ -x "$SYNC_SCRIPT" ]; then
  echo
  bash "$SYNC_SCRIPT"
else
  echo "(skip sync: $SYNC_SCRIPT not executable)"
fi
