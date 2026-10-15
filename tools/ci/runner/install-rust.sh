#!/bin/sh
# Install only the requested stable pin, MSRV and coverage toolchains.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/versions/rust.env"
. "$script_dir/runner-install-lib.sh"
if ! command -v rustup >/dev/null 2>&1; then
    bootstrap_dir=$(mktemp -d)
    bootstrap="$bootstrap_dir/rustup-init"
    trap 'rm -rf "$bootstrap_dir"' EXIT
    download_artifact \
        "https://static.rust-lang.org/rustup/archive/$RUSTUP_VERSION/x86_64-unknown-linux-gnu/rustup-init" \
        "$RUSTUP_SHA256" "$bootstrap"
    chmod 0755 "$bootstrap"
    "$bootstrap" -y --no-modify-path --default-toolchain none --profile minimal
    rm -rf "$bootstrap_dir"
    trap - EXIT
fi
# Reuse an inherited toolchain and add only missing components. Updating a
# floating nightly in place also replaces lower-layer directories on OverlayFS.
ensure_toolchain() {
    requested="$1"
    components="$2"
    present=false
    for installed in $(rustup toolchain list | cut -d ' ' -f 1); do
        case "$installed" in "$requested"-*) present=true ;; esac
    done
    if [ "$present" = true ]; then
        installed_components=$(rustup component list --toolchain "$requested" --installed)
        for component in $(printf '%s' "$components" | tr ',' ' '); do
            component_name=${component%-preview}
            if ! printf '%s\n' "$installed_components" | grep -Eq "^$component_name(-|$)"; then
                rustup component add --toolchain "$requested" "$component"
            fi
        done
        installed_targets=$(rustup target list --toolchain "$requested" --installed)
        for target in wasm32-unknown-unknown wasm32-wasip2; do
            if ! printf '%s\n' "$installed_targets" | grep -Fxq "$target"; then
                rustup target add --toolchain "$requested" "$target"
            fi
        done
        echo "Reusing installed $requested toolchain"
    else
        set -- "$requested" --profile minimal --no-self-update
        for component in $(printf '%s' "$components" | tr ',' ' '); do
            set -- "$@" --component "$component"
        done
        rustup toolchain install "$@" \
            --target wasm32-unknown-unknown --target wasm32-wasip2
    fi
}
ensure_toolchain "$RUST_VERSION" clippy,rustfmt,rust-src,llvm-tools-preview
ensure_toolchain "$MSRV_VERSION" clippy,rustfmt
ensure_toolchain "$NIGHTLY_VERSION" llvm-tools-preview
rustup default "$RUST_VERSION"
rustup toolchain uninstall stable
test "$(rustc --version | cut -d ' ' -f 2)" = "$RUST_VERSION"
test "$(cargo --version | cut -d ' ' -f 2)" = "$RUST_VERSION"
