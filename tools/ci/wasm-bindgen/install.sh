#!/bin/sh
# Install the wasm-bindgen CLI matching the web workspace lockfile.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/versions.env"
. "$script_dir/download.sh"
version="$WASM_BINDGEN_VERSION"
sha256="$WASM_BINDGEN_SHA256"
install_dir=${WASM_BINDGEN_INSTALL_DIR:-${CARGO_HOME:-$HOME/.cargo}/bin}

# Validate the pinned archive version against the calling workspace lockfile.
if [ "$#" -gt 0 ]; then
    locked_version=$(python3 - "$1" <<'PY'
import sys
if sys.version_info < (3, 11):
    raise SystemExit("Python 3.11 or newer is required to validate the web Cargo.lock")
import tomllib
from pathlib import Path

packages = tomllib.loads(Path(sys.argv[1]).read_text())["package"]
versions = {p["version"] for p in packages if p["name"] == "wasm-bindgen"}
if len(versions) != 1:
    raise SystemExit("expected one wasm-bindgen version in the web lockfile")
print(versions.pop())
PY
    )
    if [ "$locked_version" != "$version" ]; then
        echo "Update the wasm-bindgen installer pin and checksum to $locked_version" >&2
        exit 1
    fi
fi

if "$install_dir/wasm-bindgen" --version 2>/dev/null | grep -Fxq "wasm-bindgen $version" &&
    "$install_dir/wasm-bindgen-test-runner" --version 2>/dev/null | grep -Fxq "wasm-bindgen-test-runner $version"; then
    echo "Reusing wasm-bindgen $version"
    exit 0
fi

case "$(uname -s)/$(uname -m)" in
    Linux/x86_64) target=x86_64-unknown-linux-musl ;;
    *) echo "No pinned wasm-bindgen archive for this platform" >&2; exit 1 ;;
esac

download_dir=$(mktemp -d)
trap 'rm -rf "$download_dir"' EXIT
url=$(github_url "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${version}/wasm-bindgen-${version}-${target}.tar.gz")
download_artifact "$url" "$sha256" "$download_dir/archive.tar.gz"
tar -xzf "$download_dir/archive.tar.gz" --strip-components=1 -C "$download_dir"
mkdir -p "$install_dir"
for binary in wasm-bindgen wasm-bindgen-test-runner; do
    install -m 0755 "$download_dir/$binary" "$install_dir/$binary"
done
"$install_dir/wasm-bindgen" --version | grep -Fxq "wasm-bindgen $version"
"$install_dir/wasm-bindgen-test-runner" --version | grep -Fxq "wasm-bindgen-test-runner $version"
