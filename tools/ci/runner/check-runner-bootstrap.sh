#!/bin/sh
# Validate installed outputs against the same pins used by both installers.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/versions/rust.env"
. "$script_dir/versions/wasm.env"
. "$script_dir/versions/native.env"
. "$script_dir/versions/toolcache.env"
. "$script_dir/versions/agent.env"

check_version() {
    actual=$("$1" --version)
    printf '%s\n' "$actual"
    test "$(printf '%s\n' "$actual" | cut -d ' ' -f 1-2)" = "$2"
}
check_version rustc "rustc $RUST_VERSION"
check_version cargo "cargo $RUST_VERSION"
rustfmt --version
cargo-clippy --version
rustup component list --installed | grep -q '^llvm-tools'
rustup component list --installed | grep -q '^rust-src'
rustup target list --installed | grep -Fxq wasm32-unknown-unknown
rustup target list --installed | grep -Fxq wasm32-wasip2
rustup run "$MSRV_VERSION" rustc --version | grep -Fq "rustc $MSRV_VERSION "
rustup run "$NIGHTLY_VERSION" rustc --version
mold --version | grep -Fq "mold $MOLD_VERSION"
protoc --version | grep -Fxq "libprotoc $PROTOC_VERSION"
check_version sccache "sccache $SCCACHE_VERSION"
zstd --version | grep -Fq "v$ZSTD_VERSION,"
check_version wasm-pack "wasm-pack $WASM_PACK_VERSION"
check_version wasm-component-ld "wasm-component-ld $WASM_COMPONENT_LD_VERSION"
check_version wasm-tools "wasm-tools $WASM_TOOLS_VERSION"
check_version wasm-bindgen "wasm-bindgen $WASM_BINDGEN_VERSION"
check_version wasm-bindgen-test-runner "wasm-bindgen-test-runner $WASM_BINDGEN_VERSION"
wasm-opt --version | grep -Fq 'version 117'
python3 -c 'import tomllib'
for version in "$PY311" "$PY313"; do
    test -f "/opt/hostedtoolcache/Python/$version/x64.complete"
    "/opt/hostedtoolcache/Python/$version/x64/bin/python3" --version
done
test -f "/opt/hostedtoolcache/node/$NODE_VERSION/x64.complete"
"/opt/hostedtoolcache/node/$NODE_VERSION/x64/bin/node" --version
test -f "/opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/$JDK_VERSION/x64.complete"
"/opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/$JDK_VERSION/x64/bin/java" -version
python3 /usr/local/bin/prepare-gradle-cache.py \
    --distribution-url "https://services.gradle.org/distributions/gradle-$GRADLE_VERSION-bin.zip" --verify-only
test -d "$GRADLE_USER_HOME/caches/modules-2/files-2.1/com.pinterest.ktlint/ktlint-cli/$KTLINT_VERSION"
test -d "$GRADLE_USER_HOME/caches/modules-2/files-2.1/org.jlleitschuh.gradle/ktlint-gradle/$KTLINT_GRADLE_VERSION"
test -d "$ANDROID_SDK_ROOT/platforms/android-34"
test -d "$ANDROID_SDK_ROOT/build-tools/34.0.0"
test -d "$ANDROID_SDK_ROOT/build-tools/35.0.0"
test -x "$ANDROID_SDK_ROOT/platform-tools/adb"
test "$(id -u)" = 1001
test -w "$HOME/.cargo"
test -w "$GRADLE_USER_HOME"
test -w "$RUNNER_TOOL_CACHE"
test -w "$ANDROID_SDK_ROOT"
test -x /usr/local/bin/runner-entrypoint.sh
sh -n /usr/local/bin/runner-entrypoint.sh
# A local version check needs no proxy and must not log operator proxy settings.
env -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u NO_PROXY \
    -u http_proxy -u https_proxy -u all_proxy -u no_proxy \
    "$HOME/actions-runner/bin/Runner.Listener" --version | grep -Fxq "$RUNNER_VERSION"
python3 - <<'PY_CHECK'
from pathlib import Path
worker = Path('/home/runner/actions-runner/bin/Runner.Worker.dll').read_bytes()
assert worker.count('ACTIONS_RESULTS_URL'.encode('utf-16-le')) == 0, 'cache patch missing'
assert worker.count('ACTIONS_RESULTS_ORL'.encode('utf-16-le')) == 1, 'cache patch malformed'
print('Runner cache patch verified')
PY_CHECK
