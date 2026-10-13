#!/usr/bin/env bash
# Real Issuer HTTP + browser fetch regression. Requires Rust/WASM, wasm-pack and Chrome.
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
test_dir="$(mktemp -d)"
fixture_pid=""
cleanup() {
  if [[ -s "$test_dir/ready" ]]; then
    curl -fsS "$(cat "$test_dir/ready")/stop" >/dev/null 2>&1 || true
  fi
  if [[ -n "$fixture_pid" ]]; then
    kill "$fixture_pid" 2>/dev/null || true
    wait "$fixture_pid" 2>/dev/null || true
  fi
  rm -rf "$test_dir"
}
trap cleanup EXIT
cd "$repo_dir"
cargo test -p actrix-issuer --test integration_test --no-run --locked --message-format=json > "$test_dir/build.json"
fixture_bin="$(python3 - "$test_dir/build.json" <<'PY'
import json,sys
for line in open(sys.argv[1]):
    event=json.loads(line)
    if event.get('reason') == 'compiler-artifact' and event.get('executable') and event['target']['name'] == 'integration_test':
        print(event['executable'])
PY
)"
ACTR_WEB_FIXTURE_READY="$test_dir/ready" "$fixture_bin" --ignored --exact web_issuer_http_fixture --nocapture > "$test_dir/fixture.log" 2>&1 &
fixture_pid=$!
for _ in {1..150}; do
  [[ -s "$test_dir/ready" ]] && break
  if ! kill -0 "$fixture_pid" 2>/dev/null; then cat "$test_dir/fixture.log"; exit 1; fi
  sleep 0.1
done
[[ -s "$test_dir/ready" ]] || { cat "$test_dir/fixture.log"; exit 1; }
export ACTR_WEB_ISSUER_URL="$(cat "$test_dir/ready")"
wasm-pack test --headless --chrome packages/actr-web/crates/common --features live-issuer-tests --test issuer_live
wait "$fixture_pid"
fixture_pid=""
cat "$test_dir/fixture.log"
