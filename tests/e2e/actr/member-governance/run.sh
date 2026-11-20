#!/usr/bin/env bash
set -euo pipefail
platform="${1:?usage: run.sh swift|kotlin}"
case "$platform" in swift|kotlin) ;; *) echo 'expected swift or kotlin' >&2; exit 2 ;; esac
task_root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
task_dir="$(mktemp -d "${TMPDIR:-/tmp}/actr-member-governance.XXXXXX")"
fixture_pid=""
cleanup() {
    touch "$task_dir/fixture.json.stop"
    if [ -n "$fixture_pid" ]; then wait "$fixture_pid" || true; fi
    rm -f "$task_dir/fixture.json"
}
trap cleanup EXIT
cd "$task_root"
fixture_bin="${ACTR_GOVERNANCE_FIXTURE_BIN:-}"
if [ -z "$fixture_bin" ]; then
    cargo test --locked -p loran-issuer --test integration_test --no-run --message-format=json > "$task_dir/build.json"
    fixture_bin="$(python3 - "$task_dir/build.json" <<'PY'
import json, sys
for line in open(sys.argv[1]):
    item = json.loads(line)
    if item.get('target', {}).get('name') == 'integration_test' and item.get('executable'):
        print(item['executable'])
PY
)"
fi
test -x "$fixture_bin"
ACTR_GOVERNANCE_FIXTURE_READY="$task_dir/fixture.json" "$fixture_bin" --ignored \
    --exact device_membership::governance::governance_issuer_http_fixture > "$task_dir/issuer.log" 2>&1 &
fixture_pid=$!
for _ in $(seq 1 300); do
    [ -s "$task_dir/fixture.json" ] && break
    kill -0 "$fixture_pid" || { cat "$task_dir/issuer.log"; exit 1; }
    sleep 0.1
done
test -s "$task_dir/fixture.json"
export ACTR_GOVERNANCE_FIXTURE="$task_dir/fixture.json"
case "$platform" in
    swift)
        cd "$task_root/packages/actr-swift"
        swift test --filter RealmGovernanceTests
        ;;
    kotlin)
        : "${ACTR_GOVERNANCE_LIBRARY:?set the absolute path to the matching host libactr dynamic library}"
        cd "$task_root/packages/actr-kotlin"
        ./gradlew :actr-kotlin:testDebugUnitTest --tests io.actrium.actr.RealmGovernanceJvmTest --rerun-tasks --no-daemon
        ;;
esac
echo "Member governance acceptance passed ($platform). Evidence: $task_dir"
