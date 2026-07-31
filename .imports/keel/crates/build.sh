#!/usr/bin/env bash
# Build the native Rust guest(s) to wasm and stage them as test fixtures.
#
# RUSTFLAGS overrides a global `-fuse-ld=mold` (mold can't link the wasm target;
# rust-lld rejects the flag). Needs: rustup target add wasm32-unknown-unknown.
#
# --remap-path-prefix keeps build-machine absolute paths out of the .wasm
# (panic Location strings would otherwise embed them), so fixtures are
# byte-reproducible across checkouts.
#
# CARGO_TARGET_DIR is dedicated: plain `cargo test`/`cargo clippy` runs (no
# remap flag) share the default target/ and cargo may reuse their artifacts
# without honoring the RUSTFLAGS change — which silently re-embeds absolute
# paths. An isolated target dir makes the remap unconditional.
set -euo pipefail
cd "$(dirname "$0")"

TARGET=wasm32-unknown-unknown
FIXTURES=../src/host/native-guest/__tests__/fixtures
# Single source of truth for both the debug (DWARF) and release fixture builds
# below. Since the binary-protocol feature is now DEFAULT ON (issue #45), every
# guest here is binary-capable (op-batch encoder + keel_wire_caps export, canvas
# encoder + getInfo probe) yet still emits JSON at runtime unless the host
# negotiates binary — so canvas-binary-guest / ui-binary-guest build in the SAME
# invocation now (no feature-isolation split needed). ui-guest-json (built
# separately with --no-default-features) is the back-compat, pre-promotion
# JSON-only guest used to test the "guest without keel_wire_caps" host path.
# blur-demo-guest is the SIMD/worker-pool compute demo (issue #57).
GUESTS="kv-guest ui-guest seq-guest event-guest heap-churn-guest canvas-guest canvas-present-guest canvas-gpu-guest canvas-escape-guest asset-guest canvas-binary-guest ui-binary-guest blur-demo-guest"

# Optional debug build: DWARF-carrying guests for source-level debugging inside
# V8's wasm debugger (Chrome DevTools "C/C++ DevTools Support (DWARF)"). Output
# goes to a SEPARATE dir and NEVER overwrites the shipped, DWARF-free release
# fixtures; real source paths are preserved (no --remap-path-prefix here). Runs
# with the default target dir and exits before the reproducible fixtures build
# below. See docs/native-guest-debugging.zh.md.
if [ "${KEEL_GUEST_DEBUG:-}" = "1" ]; then
  DEBUG_OUT=debug-artifacts
  mkdir -p "$DEBUG_OUT"
  # shellcheck disable=SC2086
  RUSTFLAGS=" " cargo build $(printf -- '-p %s ' $GUESTS) --target "$TARGET" --profile debug-wasm
  for lib in $GUESTS; do
    crate="${lib//-/_}"
    cp "target/$TARGET/debug-wasm/$crate.wasm" "$DEBUG_OUT/$lib.wasm"
    echo "debug (DWARF): $DEBUG_OUT/$lib.wasm ($(wc -c < "$DEBUG_OUT/$lib.wasm") bytes)"
  done
  echo "NOTE: debug guests carry DWARF + real source paths; do NOT commit them."
  exit 0
fi

# Dedicated target dir for the reproducible fixtures build (see header note).
export CARGO_TARGET_DIR=target/fixtures

# shellcheck disable=SC2086
RUSTFLAGS="--remap-path-prefix=$(pwd)=." cargo build $(printf -- '-p %s ' $GUESTS) --target "$TARGET" --release

stage() { # <crate-lib-name> <fixture-name>
  cp "$CARGO_TARGET_DIR/$TARGET/release/$1.wasm" "$FIXTURES/$2"
  echo "staged: $FIXTURES/$2 ($(wc -c < "$FIXTURES/$2") bytes)"
}
stage kv_guest kv-guest.wasm
stage seq_guest seq-guest.wasm
stage ui_guest ui-guest.wasm
stage ui_binary_guest ui-binary-guest.wasm
stage event_guest event-guest.wasm
stage heap_churn_guest heap-churn-guest.wasm
stage canvas_guest canvas-guest.wasm
stage canvas_binary_guest canvas-binary-guest.wasm
stage canvas_present_guest canvas-present-guest.wasm
stage canvas_gpu_guest canvas-gpu-guest.wasm
stage canvas_escape_guest canvas-escape-guest.wasm
stage asset_guest asset-guest.wasm
stage blur_demo_guest blur-demo-guest.wasm

# Back-compat fixture: a pre-promotion, JSON-only guest with the binary encoders
# and keel_wire_caps compiled OUT (--no-default-features). SEPARATE invocation
# because cargo unifies features across a single build — bundling it above would
# re-enable the default feature. Staged AFTER the default ui-guest.wasm so it
# does not clobber it. Exercises the host path for a guest that lacks the
# keel_wire_caps export (the caps push must be skipped, JSON must still flow).
RUSTFLAGS="--remap-path-prefix=$(pwd)=." cargo build -p ui-guest --no-default-features --target "$TARGET" --release
stage ui_guest ui-guest-json.wasm
