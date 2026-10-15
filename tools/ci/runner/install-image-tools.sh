#!/bin/sh
# Install the same pinned artifacts in full and incremental images.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/runner-install-lib.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

install_binary() {
    name="$1"
    url="$2"
    sha="$3"
    if [ -x "$CARGO_HOME/bin/$name" ] &&
        [ "$("$CARGO_HOME/bin/$name" --version | cut -d ' ' -f 1-2)" = "$4" ]; then
        return
    fi
    mkdir -p "$work/$name" "$CARGO_HOME/bin"
    download_artifact "$(github_url "$url")" "$sha" "$work/$name.tar.gz"
    tar -xzf "$work/$name.tar.gz" --strip-components=1 -C "$work/$name"
    install -m 0755 "$work/$name/$name" "$CARGO_HOME/bin/$name"
}

case "$1" in
    wasm)
        . "$script_dir/versions/wasm.env"
        install_binary wasm-pack \
            "https://github.com/wasm-bindgen/wasm-pack/releases/download/v$WASM_PACK_VERSION/wasm-pack-v$WASM_PACK_VERSION-x86_64-unknown-linux-musl.tar.gz" \
            "$WASM_PACK_SHA256" "wasm-pack $WASM_PACK_VERSION"
        install_binary wasm-component-ld \
            "https://github.com/bytecodealliance/wasm-component-ld/releases/download/v$WASM_COMPONENT_LD_VERSION/wasm-component-ld-v$WASM_COMPONENT_LD_VERSION-x86_64-linux.tar.gz" \
            "$WASM_COMPONENT_LD_SHA256" "wasm-component-ld $WASM_COMPONENT_LD_VERSION"
        install_binary wasm-tools \
            "https://github.com/bytecodealliance/wasm-tools/releases/download/v$WASM_TOOLS_VERSION/wasm-tools-$WASM_TOOLS_VERSION-x86_64-linux.tar.gz" \
            "$WASM_TOOLS_SHA256" "wasm-tools $WASM_TOOLS_VERSION"
        install_binary sccache \
            "https://github.com/mozilla/sccache/releases/download/v$SCCACHE_VERSION/sccache-v$SCCACHE_VERSION-x86_64-unknown-linux-musl.tar.gz" \
            "$SCCACHE_SHA256" "sccache $SCCACHE_VERSION"
        if ! "$CARGO_HOME/bin/wasm-opt" --version 2>/dev/null | grep -Fq 'version 117'; then
            download_artifact \
                "$(github_url "https://github.com/WebAssembly/binaryen/releases/download/$BINARYEN_VERSION/binaryen-$BINARYEN_VERSION-x86_64-linux.tar.gz")" \
                "$BINARYEN_SHA256" "$work/binaryen.tar.gz"
            mkdir -p "$HOME/binaryen"
            tar -xzf "$work/binaryen.tar.gz" --strip-components=1 -C "$HOME/binaryen"
            ln -sf "$HOME/binaryen/bin/wasm-opt" "$CARGO_HOME/bin/wasm-opt"
        fi
        sh "$script_dir/install-wasm-bindgen.sh"
        ;;
    native)
        . "$script_dir/versions/native.env"
        if ! protoc --version 2>/dev/null | grep -Fxq "libprotoc $PROTOC_VERSION"; then
            download_artifact \
                "$(github_url "https://github.com/protocolbuffers/protobuf/releases/download/v$PROTOC_VERSION/protoc-$PROTOC_VERSION-linux-x86_64.zip")" \
                "$PROTOC_SHA256" "$work/protoc.zip"
            unzip -oq "$work/protoc.zip" -d /usr/local
        fi
        if ! mold --version 2>/dev/null | grep -Fq "mold $MOLD_VERSION"; then
            download_artifact \
                "$(github_url "https://github.com/rui314/mold/releases/download/v$MOLD_VERSION/mold-$MOLD_VERSION-x86_64-linux.tar.gz")" \
                "$MOLD_SHA256" "$work/mold.tar.gz"
            tar -xzf "$work/mold.tar.gz" --strip-components=1 -C /usr/local
        fi
        sh "$script_dir/install-zstd.sh"
        ;;
    *) echo 'Expected native or wasm tool group' >&2; exit 1 ;;
esac
