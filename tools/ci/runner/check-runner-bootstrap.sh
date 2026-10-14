#!/bin/sh
# Validate the common bootstrap and inherited toolcache before publishing.
set -eu
[ "$#" -eq 5 ] || { echo "Expected Python 3.11, Python 3.13, Node, JDK and Gradle URL pins" >&2; exit 1; }
py311="$1"
py313="$2"
node_version="$3"
jdk_version="$4"
gradle_url="$5"
rustc --version
cargo --version
rustfmt --version
cargo-clippy --version
mold --version | grep -Fq 'mold 2.40.4'
sccache --version | grep -Fxq 'sccache 0.18.0'
zstd --version | grep -Fq 'v1.5.7,'
wasm-pack --version | grep -Fxq 'wasm-pack 0.15.0'
wasm-component-ld --version | grep -Fxq 'wasm-component-ld 0.5.26'
wasm-tools --version | grep -Eq '^wasm-tools 1\.253\.0( \(.*\))?$'
wasm-bindgen --version | grep -Fxq 'wasm-bindgen 0.2.126'
wasm-bindgen-test-runner --version | grep -Fxq 'wasm-bindgen-test-runner 0.2.126'
wasm-opt --version | grep -Fq 'version 117'
python3 -c 'import tomllib'
for version in "$py311" "$py313"; do
    test -f "/opt/hostedtoolcache/Python/$version/x64.complete"
    "/opt/hostedtoolcache/Python/$version/x64/bin/python3" --version
done
test -f "/opt/hostedtoolcache/node/$node_version/x64.complete"
"/opt/hostedtoolcache/node/$node_version/x64/bin/node" --version
test -f "/opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/$jdk_version/x64.complete"
"/opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/$jdk_version/x64/bin/java" -version
python3 /usr/local/bin/prepare-gradle-cache.py --distribution-url "$gradle_url" --verify-only
test "$(id -u)" = 1001
test -x /usr/local/bin/runner-entrypoint.sh
sh -n /usr/local/bin/runner-entrypoint.sh
python3 - <<'PY_CHECK'
from pathlib import Path
worker = Path('/home/runner/actions-runner/bin/Runner.Worker.dll').read_bytes()
assert worker.count('ACTIONS_RESULTS_URL'.encode('utf-16-le')) == 0, 'cache patch missing'
assert worker.count('ACTIONS_RESULTS_ORL'.encode('utf-16-le')) == 1, 'cache patch malformed'
print('Runner cache patch verified')
PY_CHECK
