#!/usr/bin/env bash

# WinUI EchoApp E2E — Windows mirror of e2e/swift-echo-app/run.sh.
#
# Flow: build actrix + actr CLI + EchoService (Rust) + the cabi.EchoApp
# workload dll + the WinUI app; start actrix; provision realm/mfr keychain;
# start the EchoService; render the app's actr.toml; launch the WinUI app
# with ACTR_ECHOAPP_AUTO_SEND=1; wait for the ACTR_E2E_RESULT marker on
# stderr and verify the echo reply.
#
# Reuses e2e/package-runtime-echo/lib/common.sh for the actrix / realm /
# keychain / service-package lifecycle. Windows-specific: port cleanup
# (netstat+taskkill instead of lsof), dotnet build for the app, no simulator.
#
# UNVERIFIED on Windows (no Windows SDK on macOS). Run on a Windows runner
# (windows-latest / windows-arm64-larger) with dotnet + the Windows App SDK
# workload installed.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

source "$SCRIPT_DIR/../package-runtime-echo/lib/common.sh"
source "$SCRIPT_DIR/../swift-echo-app/lib/readiness.sh"

HTTP_PORT=8081
ICE_PORT=3478
REALM_ID=""
ADMIN_PASSWORD="e2e-test-password"
MANUFACTURER="actrium"
ACTRIX_BIN="${ACTRIX_BIN:-}"
ACTR_CLI_MANIFEST="$REPO_ROOT/cli/Cargo.toml"
ACTRIX_CONFIG_TEMPLATE="$SCRIPT_DIR/../package-runtime-echo/config/actrix.toml"
E2E_TARGET_ROOT="$REPO_ROOT/target/e2e-cache/winui-echo-app"
ACTR_TARGET_DIR="$E2E_TARGET_ROOT/actr-cli"
TEMP_SERVICE_TARGET_DIR="$E2E_TARGET_ROOT/temp-service"
DEFAULT_MESSAGE="e2e-test-message"
REALM_NAME_PREFIX="winui-echo-app"
TEST_INPUT="$DEFAULT_MESSAGE"

# The cabi workload crate is a standalone workspace; build-actr-cabi-dll.sh
# expects the cdylib under ${WORKSPACE_ROOT}/target, so force that target dir
# for the dll build (actr CLI / actrix builds below override it inline).
export CARGO_TARGET_DIR="$REPO_ROOT/target"

while [[ $# -gt 0 ]]; do
    case "$1" in
        -*) fail "Unknown option: $1" ;;
        *) TEST_INPUT="$1"; shift ;;
    esac
done

for cmd in cargo curl jq sqlite3 rustc dotnet; do
    require_cmd "$cmd"
done

RUN_ID="$(date +%Y%m%d-%H%M%S)-$RANDOM"
RUN_DIR="$SCRIPT_DIR/.tmp/run-$RUN_ID"
STATE_DIR="$RUN_DIR/state"
SQLITE_DIR="$STATE_DIR/sqlite"
LOG_DIR="$RUN_DIR/logs"
DIST_DIR="$RUN_DIR/dist"
TMP_SERVICE_ROOT="$RUN_DIR/workspace"
TMP_SERVICE_DIR="$TMP_SERVICE_ROOT/echo-actr-$RANDOM"
SERVICE_KEYCHAIN="$TMP_SERVICE_DIR/packaging/keys/mfr.keychain.json"
SERVICE_PUBLIC_KEY="$TMP_SERVICE_DIR/public-key.json"
PROVISIONED_KEYCHAIN="$RUN_DIR/mfr.keychain.json"
PROVISIONED_PUBLIC_KEY="$RUN_DIR/mfr-public-key.json"
ACTRIX_CONFIG_PATH="$RUN_DIR/actrix.toml"
SERVER_RUNTIME_PATH="$RUN_DIR/server-runtime.toml"
ECHOAPP_ACTRIX_CONFIG="$RUN_DIR/echoapp-actr.toml"
HOST_TARGET="$(rustc -vV | awk '/host:/ {print $2}')"
ECHOAPP_PACKAGE_MANIFEST="$RUN_DIR/echoapp-package-manifest.toml"
ECHOAPP_MARKER_BINARY="$RUN_DIR/echoapp-linked-identity.bin"
ECHOAPP_PACKAGE="$DIST_DIR/${MANUFACTURER}-EchoApp-0.1.0-${HOST_TARGET}.actr"
APP_STDOUT_LOG="$LOG_DIR/app.stdout.log"
APP_STDERR_LOG="$LOG_DIR/app.stderr.log"
APP_PROCESS_NAME="WinUiEchoApp"
CABI_DLL=""
APP_EXE=""

mkdir -p "$SQLITE_DIR" "$LOG_DIR" "$DIST_DIR" "$TMP_SERVICE_ROOT" "$E2E_TARGET_ROOT"

ACTRIX_PID=""
SERVER_PID=""
ACTR_CLI_BIN=""
ADMIN_TOKEN=""
SERVICE_PACKAGE=""
SERVICE_VERSION=""
REALM_SECRET=""

# ──── Windows port cleanup (override common.sh's lsof-based kill_listener) ────
kill_listener() {
    local protocol="$1" port="$2"
    [ "$protocol" = "tcp" ] || return 0
    local pids
    pids="$(netstat -ano 2>/dev/null \
            | awk -v p=":$port" '$2 ~ p && $4 == "LISTENING" {print $5}' \
            | sort -u)"
    for pid in $pids; do
        [ -n "$pid" ] && MSYS_NO_PATHCONV=1 taskkill //F //PID "$pid" 2>/dev/null || true
    done
}

run_actr() {
    CARGO_TARGET_DIR="$ACTR_TARGET_DIR" "$ACTR_CLI_BIN" "$@"
}

grep_app_logs() { grep -h "$@" "$APP_STDOUT_LOG" "$APP_STDERR_LOG" 2>/dev/null; }
tail_app_logs() {
    echo "App stdout tail:"
    tail -n "$1" "$APP_STDOUT_LOG" >&2 2>/dev/null || true
    echo "App stderr tail:"
    tail -n "$1" "$APP_STDERR_LOG" >&2 2>/dev/null || true
}

cleanup() {
    local status=$?
    if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
    fi
    if [ -n "${ACTRIX_PID:-}" ] && kill -0 "$ACTRIX_PID" 2>/dev/null; then
        kill "$ACTRIX_PID" 2>/dev/null || true
    fi
    MSYS_NO_PATHCONV=1 taskkill //F //IM "${APP_PROCESS_NAME}.exe" 2>/dev/null || true
    wait 2>/dev/null || true
    rm -f "$ECHOAPP_ACTRIX_CONFIG"
    if [ $status -eq 0 ] && [ "${KEEP_TMP:-0}" != "1" ]; then
        rm -rf "$RUN_DIR"
    else
        echo "Artifacts preserved at: $RUN_DIR"
    fi
    exit $status
}
trap cleanup EXIT INT TERM

# ──── Rust / actrix / actr-CLI build (reused from swift-echo-app) ────

build_local_actr_cli() {
    section "🔧 Building local actr CLI"
    CARGO_TARGET_DIR="$ACTR_TARGET_DIR" cargo build --manifest-path "$ACTR_CLI_MANIFEST" --bin actr >/dev/null
    ACTR_CLI_BIN="$ACTR_TARGET_DIR/debug/actr"
    [ -x "$ACTR_CLI_BIN" ] || fail "actr CLI binary missing at $ACTR_CLI_BIN"
    success "actr CLI ready: $ACTR_CLI_BIN"
}

build_local_actrix() {
    if [ -n "${ACTRIX_BIN:-}" ]; then
        local resolved
        resolved="$(command -v "$ACTRIX_BIN" 2>/dev/null || true)"
        [ -n "$resolved" ] || [ -x "$ACTRIX_BIN" ] || fail "ACTRIX_BIN set but not executable: $ACTRIX_BIN"
        ACTRIX_BIN="${resolved:-$ACTRIX_BIN}"
        export ACTRIX_BIN
        success "Using caller-provided actrix: $ACTRIX_BIN"
        return 0
    fi
    section "🔨 Building local actrix from source"
    CARGO_TARGET_DIR="$ACTR_TARGET_DIR" cargo build \
        --manifest-path "$REPO_ROOT/actrix/crates/actrixd/Cargo.toml" \
        --bin actrix >/dev/null
    ACTRIX_BIN="$ACTR_TARGET_DIR/debug/actrix"
    [ -x "$ACTRIX_BIN" ] || fail "actrix binary missing at $ACTRIX_BIN"
    export ACTRIX_BIN
    success "actrix ready: $ACTRIX_BIN"
}

# ──── cabi.EchoApp workload dll ────

build_workload_dll() {
    section "🔨 Building actr_cabi.dll (cabi.EchoApp workload)"
    # build-actr-cabi-dll.sh builds the workload cdylib and copies it to
    # dist/actr_cabi.dll. CARGO_TARGET_DIR is already exported to
    # $REPO_ROOT/target so the script's lib_dir lookup matches.
    ACTR_CABI_WORKLOAD_CRATE="$SCRIPT_DIR/workload" \
    ACTR_CABI_TARGETS="x86_64-pc-windows-msvc" \
    ACTR_BUILD_PROFILE="release" \
        bash "$REPO_ROOT/bindings/cabi/csharp/build-actr-cabi-dll.sh"
    CABI_DLL="$REPO_ROOT/bindings/cabi/csharp/dist/actr_cabi.dll"
    [ -f "$CABI_DLL" ] || fail "actr_cabi.dll not built at $CABI_DLL"
    success "actr_cabi.dll ready: $CABI_DLL"
}

# ──── WinUI app build ────

build_winui_app() {
    section "🏗️  Building WinUI EchoApp"
    local csproj="$SCRIPT_DIR/WinUiEchoApp/WinUiEchoApp.csproj"
    # Place the dll next to the csproj so CopyToOutputDirectory ships it.
    cp "$CABI_DLL" "$SCRIPT_DIR/WinUiEchoApp/actr_cabi.dll"
    dotnet build "$csproj" -c Debug -r win-x64 2>&1 | tee -a "$LOG_DIR/dotnet-build.log"
    APP_EXE="$SCRIPT_DIR/WinUiEchoApp/bin/Debug/net8.0-windows10.0.19041.0/win-x64/${APP_PROCESS_NAME}.exe"
    [ -f "$APP_EXE" ] || fail "WinUiEchoApp.exe not found at $APP_EXE"
    success "WinUI EchoApp built: $APP_EXE"
}

# ──── Service guest scaffolding (reused from swift-echo-app) ────

append_workspace_patch() {
    local cargo_toml="$1"
    grep -q '^\[workspace\]' "$cargo_toml" || cat >>"$cargo_toml" <<'EOF'

[workspace]
EOF
    grep -q '^\[patch\.crates-io\]' "$cargo_toml" && return 0
    cat >>"$cargo_toml" <<EOF

[patch.crates-io]
actr = { path = "$REPO_ROOT" }
actr-config = { path = "$REPO_ROOT/core/config" }
actr-protocol = { path = "$REPO_ROOT/core/protocol" }
actr-framework = { path = "$REPO_ROOT/core/framework" }
actr-hyper = { path = "$REPO_ROOT/core/hyper" }
actr-pack = { path = "$REPO_ROOT/core/pack" }
actr-platform-native = { path = "$REPO_ROOT/core/platform-native" }
actr-platform-traits = { path = "$REPO_ROOT/core/platform-traits" }
actr-runtime = { path = "$REPO_ROOT/core/runtime" }
actr-runtime-mailbox = { path = "$REPO_ROOT/core/runtime-mailbox" }
actr-service-compat = { path = "$REPO_ROOT/core/service-compat" }
EOF
}

pin_workspace_actr_dependencies() {
    local cargo_toml="$1"
    ACTR_FRAMEWORK_PATH="$REPO_ROOT/core/framework" \
    ACTR_PROTOCOL_PATH="$REPO_ROOT/core/protocol" \
        perl -i -pe '
            if (/^actr-framework = /) { $_ = qq{actr-framework = { path = "$ENV{ACTR_FRAMEWORK_PATH}" }\n}; }
            elsif (/^actr-protocol = /) { $_ = qq{actr-protocol = { path = "$ENV{ACTR_PROTOCOL_PATH}" }\n}; }
        ' "$cargo_toml"
}

write_project_keychain_config() {
    local project_dir="$1" keychain_path="$2"
    mkdir -p "$project_dir/.actr"
    cat >"$project_dir/.actr/config.toml" <<EOF
[mfr]
manufacturer = "$MANUFACTURER"
keychain = "$keychain_path"

[network]
signaling_url = "ws://127.0.0.1:${HTTP_PORT}/signaling/ws"
ais_endpoint = "http://127.0.0.1:${HTTP_PORT}/ais"
realm_id = $REALM_ID
realm_secret = "$REALM_SECRET"
EOF
}

scaffold_service_guest() {
    section "🧱 Scaffolding temporary echo service"
    run_actr init -l rust --template echo --role service \
        --signaling "ws://127.0.0.1:${HTTP_PORT}/signaling/ws" \
        --manufacturer "$MANUFACTURER" "$TMP_SERVICE_DIR"
    pin_workspace_actr_dependencies "$TMP_SERVICE_DIR/Cargo.toml"
    append_workspace_patch "$TMP_SERVICE_DIR/Cargo.toml"
    mkdir -p "$(dirname "$SERVICE_KEYCHAIN")"
    cp "$PROVISIONED_KEYCHAIN" "$SERVICE_KEYCHAIN"
    cp "$PROVISIONED_PUBLIC_KEY" "$SERVICE_PUBLIC_KEY"
    write_project_keychain_config "$TMP_SERVICE_DIR" "$SERVICE_KEYCHAIN"
    (
        cd "$TMP_SERVICE_DIR"
        CARGO_TARGET_DIR="$TEMP_SERVICE_TARGET_DIR" run_actr deps install
        CARGO_TARGET_DIR="$TEMP_SERVICE_TARGET_DIR" run_actr gen -l rust
    )
    SERVICE_VERSION="$(awk '
        /^\[package\]/ { in_package = 1; next }
        /^\[/ && in_package { exit }
        in_package && $1 == "version" { gsub(/"/, "", $3); print $3; exit }
    ' "$TMP_SERVICE_DIR/manifest.toml")"
    [ -n "$SERVICE_VERSION" ] || fail "Unable to detect temporary service version"
    success "Temporary echo service ready: version ${SERVICE_VERSION}"
}

build_service_package() {
    section "📦 Building and publishing the server package"
    SERVICE_PACKAGE="$DIST_DIR/${MANUFACTURER}-EchoService-${SERVICE_VERSION}-${HOST_TARGET}.actr"
    (
        cd "$TMP_SERVICE_DIR"
        CARGO_TARGET_DIR="$TEMP_SERVICE_TARGET_DIR" run_actr build \
            --manifest-path manifest.toml --key "$SERVICE_KEYCHAIN" --output "$SERVICE_PACKAGE"
    )
    [ -f "$SERVICE_PACKAGE" ] || fail "Server package missing: $SERVICE_PACKAGE"
    run_actr pkg verify --pubkey "$SERVICE_PUBLIC_KEY" --package "$SERVICE_PACKAGE" >/dev/null
    run_actr registry publish \
        --package "$SERVICE_PACKAGE" --keychain "$SERVICE_KEYCHAIN" \
        --endpoint "http://127.0.0.1:${HTTP_PORT}"
    success "Server package published"
}

publish_echoapp_package_identity() {
    section "📦 Publishing EchoApp package identity marker"
    printf 'linked EchoApp identity marker\n' >"$ECHOAPP_MARKER_BINARY"
    cat >"$ECHOAPP_PACKAGE_MANIFEST" <<EOF
edition = 1

[package]
name = "EchoApp"
manufacturer = "${MANUFACTURER}"
version = "0.1.0"
description = "Actrium EchoApp linked runtime identity marker (WinUI)"

[binary]
path = "${ECHOAPP_MARKER_BINARY}"
target = "${HOST_TARGET}"
EOF
    run_actr build --no-compile \
        --manifest-path "$ECHOAPP_PACKAGE_MANIFEST" \
        --key "$PROVISIONED_KEYCHAIN" --output "$ECHOAPP_PACKAGE"
    run_actr pkg verify --pubkey "$PROVISIONED_PUBLIC_KEY" --package "$ECHOAPP_PACKAGE" >/dev/null
    run_actr registry publish \
        --package "$ECHOAPP_PACKAGE" --keychain "$PROVISIONED_KEYCHAIN" \
        --endpoint "http://127.0.0.1:${HTTP_PORT}"
    success "EchoApp package identity published"
}

render_echoapp_config() {
    section "📝 Rendering EchoApp runtime config"
    render_template "$SCRIPT_DIR/actr.toml.tpl" "$ECHOAPP_ACTRIX_CONFIG" \
        "__HOST__=127.0.0.1" \
        "__HTTP_PORT__=$HTTP_PORT" \
        "__ICE_PORT__=$ICE_PORT" \
        "__REALM_ID__=$REALM_ID" \
        "__REALM_SECRET__=$REALM_SECRET"
    success "EchoApp actr.toml rendered"
}

run_server_host() {
    section "🚀 Starting package-backed server host"
    cat >"$SERVER_RUNTIME_PATH" <<EOF
edition = 1

[package]
path = "${SERVICE_PACKAGE}"

[signaling]
url = "ws://127.0.0.1:${HTTP_PORT}/signaling/ws"

[ais_endpoint]
url = "http://127.0.0.1:${HTTP_PORT}/ais"

[deployment]
realm_id = ${REALM_ID}
realm_secret = "${REALM_SECRET}"

[[trust]]
kind = "registry"
endpoint = "http://127.0.0.1:${HTTP_PORT}/ais"

[discovery]
visible = true

[observability]
filter_level = "info"
tracing_enabled = false
tracing_endpoint = "http://localhost:4317"
tracing_service_name = "winui-echo-app-server"

[webrtc]
force_relay = false
stun_urls = ["stun:127.0.0.1:${ICE_PORT}"]
turn_urls = ["turn:127.0.0.1:${ICE_PORT}"]

[acl]

[[acl.rules]]
permission = "allow"
type = "${MANUFACTURER}:EchoApp:0.1.0"
EOF
    RUST_LOG="${RUST_LOG:-info}" run_actr run -c "$SERVER_RUNTIME_PATH" >"$LOG_DIR/server.log" 2>&1 &
    SERVER_PID=$!
    local attempt=0
    while [ $attempt -lt 30 ]; do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            cat "$LOG_DIR/server.log" >&2 || true
            fail "Server host exited early"
        fi
        if grep -q "Echo Host fully started\|ActrNode started" "$LOG_DIR/server.log" 2>/dev/null; then
            success "Server host is running"
            return 0
        fi
        sleep 1
        attempt=$((attempt + 1))
    done
    warn "Server host readiness log not observed, continuing"
}

check_service_ready() {
    section "🔍 Verifying EchoService readiness"
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        cat "$LOG_DIR/server.log" >&2 || true
        fail "EchoService process died before app launch"
    fi
    if ! curl -fsS "http://127.0.0.1:${HTTP_PORT}/signaling/health" >/dev/null 2>&1; then
        fail "Signaling health check failed before app launch"
    fi
    if ! wait_for_service_registration "$SQLITE_DIR/signaling_cache.db" \
            "$REALM_ID" "$MANUFACTURER" "EchoService" \
            "${SERVICE_READY_TIMEOUT_SECONDS:-60}"; then
        tail -n 120 "$LOG_DIR/server.log" >&2 2>/dev/null || true
        fail "EchoService did not register with signaling within the timeout"
    fi
    success "EchoService readiness check complete"
}

launch_app() {
    section "📲 Launching WinUI EchoApp"
    ACTR_ECHOAPP_AUTO_SEND=1 \
    ACTR_ECHOAPP_TEST_INPUT="$TEST_INPUT" \
    ACTR_ECHOAPP_ACTR_CONFIG="$ECHOAPP_ACTRIX_CONFIG" \
        "$APP_EXE" >"$APP_STDOUT_LOG" 2>"$APP_STDERR_LOG" &
    success "App launched, waiting for echo result"
}

wait_for_echo_result() {
    section "⏳ Waiting for echo result"
    local timeout="${CLIENT_TIMEOUT_SECONDS:-120}"
    local elapsed=0
    while [ $elapsed -lt "$timeout" ]; do
        if grep_app_logs -q "ACTR_E2E_RESULT:"; then
            local result
            result="$(grep_app_logs "ACTR_E2E_RESULT:" | tail -1)"
            echo "Echo result: $result"
            if echo "$result" | grep -q "$TEST_INPUT"; then
                success "End-to-end echo succeeded"
                return 0
            fi
            warn "Echo result does not contain expected message: $TEST_INPUT"
            return 1
        fi
        sleep 2
        elapsed=$((elapsed + 2))
    done
    tail_app_logs 80
    fail "Timed out waiting for echo result after ${timeout}s"
}

# ──── Main ────

section "🧪 WinUI EchoApp E2E"
echo "Run directory: $RUN_DIR"
echo "Message:       $TEST_INPUT"
echo "Actrix binary: ${ACTRIX_BIN:-workspace source (built below)}"
echo "Host target:   $HOST_TARGET"

render_runtime_configs
build_local_actr_cli
build_local_actrix
build_workload_dll
build_winui_app
start_actrix
login_admin
warmup_ais_key
ensure_realm
provision_mfr_keychain

scaffold_service_guest
build_service_package
publish_echoapp_package_identity

render_echoapp_config
run_server_host
check_service_ready

launch_app
wait_for_echo_result

echo ""
success "WinUI EchoApp e2e completed successfully"
