#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

source "$SCRIPT_DIR/lib/common.sh"

HTTP_PORT=8081
ICE_PORT=3478
REALM_ID=""
ADMIN_PASSWORD="e2e-test-password"
MANUFACTURER="e2e.actrium.test"
# The client guest is a realm consumer, not a verified publisher: it keeps a
# non-dotted manufacturer so its linked (realm-secret) registrations satisfy
# AIS-01, which reserves DNS-style names for package identities.
CLIENT_MANUFACTURER="e2e-actrium-local"
CLIENT_GUEST_VERSION="0.1.0"
ACTRIX_BIN="${ACTRIX_BIN:-}"
ACTR_CLI_MANIFEST="$REPO_ROOT/crates/actr-cli/Cargo.toml"
E2E_TARGET_ROOT="$REPO_ROOT/target/e2e-cache/package-runtime-echo"
ACTR_TARGET_DIR="$E2E_TARGET_ROOT/actr-cli"
ACTRIX_TARGET_DIR="$E2E_TARGET_ROOT/actrix-bin"
WORKSPACE_TARGET_DIR="$E2E_TARGET_ROOT/workspace"
TEMP_SERVICE_TARGET_DIR="$E2E_TARGET_ROOT/temp-service"
DEFAULT_MESSAGE="TmpFlow"

BACKEND="cdylib"
TEST_INPUT="$DEFAULT_MESSAGE"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --backend)
            [[ $# -lt 2 ]] && fail "Missing value for --backend"
            BACKEND="$2"
            shift 2
            ;;
        --backend=*)
            BACKEND="${1#--backend=}"
            shift
            ;;
        -*)
            fail "Unknown option: $1"
            ;;
        *)
            TEST_INPUT="$1"
            shift
            ;;
    esac
done

if [[ "$BACKEND" != "cdylib" ]]; then
    fail "Only --backend cdylib is supported in this scenario"
fi

# MODE selects which registration path is exercised:
#   path1                (default) published package + runner proof, Path 1, full echo round-trip
#   path1-with-keychain  published package + configured mfr.keychain (Path 1 with runner proof)
#   path2-happy          unpublished package + runner proof → Path 2 success
#   path2-no-proof       unpublished package, no keychain → Path 2 rejection
#   path2-wrong-key      unpublished package, runner proof signed by unrelated key → rejection
#   rotation             unpublished package, MFR key retired (pass) then revoked (reject)
MODE="${MODE:-path1}"
case "$MODE" in
    path1|path1-with-keychain|path2-happy|path2-no-proof|path2-wrong-key|rotation) ;;
    *) fail "Unknown MODE: $MODE" ;;
esac

for cmd in cargo curl jq sqlite3 python3 perl rustc lsof; do
    require_cmd "$cmd"
done

# Both embedded manifest parsers read TOML via the stdlib. Require Python
# 3.11+ (the tomllib floor) instead of shipping a tomli fallback that no CI
# runner ever exercises.
python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)' ||
    fail "Python 3.11+ is required for stdlib tomllib (found $(python3 -V 2>&1))"

RUN_ID="$(date +%Y%m%d-%H%M%S)-$RANDOM"
RUN_DIR="$SCRIPT_DIR/.tmp/run-$RUN_ID"
STATE_DIR="$RUN_DIR/state"
SQLITE_DIR="$STATE_DIR/sqlite"
LOG_DIR="$RUN_DIR/logs"
DIST_DIR="$RUN_DIR/dist"
TMP_SERVICE_ROOT="$RUN_DIR/workspace"
TMP_SERVICE_DIR="$TMP_SERVICE_ROOT/echo-actr-$RANDOM"
ACTRIX_CONFIG_PATH="$RUN_DIR/actrix.toml"
SERVER_RUNTIME_PATH="$RUN_DIR/server-runtime.toml"
CLIENT_RUNTIME_PATH="$RUN_DIR/client-runtime.toml"
ACTRIX_DB="$SQLITE_DIR/actrix.db"
SERVICE_KEYCHAIN="$TMP_SERVICE_DIR/packaging/keys/mfr.keychain.json"
SERVICE_PUBLIC_KEY="$TMP_SERVICE_DIR/public-key.json"
PROVISIONED_KEYCHAIN="$RUN_DIR/mfr.keychain.json"
PROVISIONED_PUBLIC_KEY="$RUN_DIR/mfr-public-key.json"
CLIENT_GUEST_PACKAGE="$DIST_DIR/${CLIENT_MANUFACTURER}-pkg-runtime-echo-client-guest-${CLIENT_GUEST_VERSION}-cdylib.actr"
CLIENT_GUEST_PUBLIC_KEY="$DIST_DIR/public-key.json"

mkdir -p "$SQLITE_DIR" "$LOG_DIR" "$DIST_DIR" "$TMP_SERVICE_ROOT" "$E2E_TARGET_ROOT"

ACTRIX_PID=""
SERVER_PID=""
CLIENT_PID=""
ACTR_CLI_BIN=""
ADMIN_TOKEN=""
SERVICE_PACKAGE=""
SERVICE_VERSION=""
REALM_SECRET=""
HOST_TARGET="$(rustc -vV | awk '/host:/ {print $2}')"

cleanup() {
    local status=$?

    if [ -n "$CLIENT_PID" ] && kill -0 "$CLIENT_PID" 2>/dev/null; then
        kill "$CLIENT_PID" 2>/dev/null || true
    fi
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
    fi
    if [ -n "$ACTRIX_PID" ] && kill -0 "$ACTRIX_PID" 2>/dev/null; then
        kill "$ACTRIX_PID" 2>/dev/null || true
    fi
    wait 2>/dev/null || true

    if [ $status -eq 0 ] && [ "${KEEP_TMP:-0}" != "1" ]; then
        rm -rf "$RUN_DIR"
    else
        echo ""
        echo "Artifacts preserved at: $RUN_DIR"
    fi
}
trap cleanup EXIT INT TERM

run_actr() {
    CARGO_TARGET_DIR="$ACTR_TARGET_DIR" "$ACTR_CLI_BIN" "$@"
}

build_local_actr_cli() {
    section "🔧 Building local actr CLI"
    CARGO_TARGET_DIR="$ACTR_TARGET_DIR" cargo build --manifest-path "$ACTR_CLI_MANIFEST" --bin actr >/dev/null
    ACTR_CLI_BIN="$ACTR_TARGET_DIR/debug/actr"
    [ -x "$ACTR_CLI_BIN" ] || fail "actr CLI binary missing at $ACTR_CLI_BIN"
    success "actr CLI ready: $ACTR_CLI_BIN"
}

# Build actrix from the branch source into a cache dir and use it, rather than
# trusting an `actrix` found on PATH (which may be a stale install that doesn't
# serve the /mfr public routes or accept the current config schema). A caller
# (CI) may pre-set ACTRIX_BIN to skip the build.
build_local_actrix() {
    if [ -n "${ACTRIX_BIN:-}" ] && [ -x "$ACTRIX_BIN" ]; then
        success "Using caller-provided actrix: $ACTRIX_BIN"
        return 0
    fi
    section "🔧 Building local actrix from source"
    CARGO_TARGET_DIR="$ACTRIX_TARGET_DIR" cargo build \
        --manifest-path "$REPO_ROOT/crates/loran-server/Cargo.toml" \
        --bin actrix >/dev/null
    ACTRIX_BIN="$ACTRIX_TARGET_DIR/debug/actrix"
    [ -x "$ACTRIX_BIN" ] || fail "actrix binary missing at $ACTRIX_BIN"
    export ACTRIX_BIN
    success "actrix ready: $ACTRIX_BIN"
}

render_runtime_configs() {
    render_template \
        "$SCRIPT_DIR/config/actrix.toml" \
        "$ACTRIX_CONFIG_PATH" \
        "__SQLITE_DIR__=$SQLITE_DIR" \
        "__HTTP_PORT__=$HTTP_PORT" \
        "__ICE_PORT__=$ICE_PORT"
}

start_actrix() {
    section "🚀 Starting local actrix"
    kill_listener tcp "$HTTP_PORT"
    kill_listener udp "$ICE_PORT"

    # ACTRIX_RUST_LOG lets path2 modes capture the debug Path 2 marker.
    RUST_LOG="${ACTRIX_RUST_LOG:-${RUST_LOG:-info}}" \
        "$ACTRIX_BIN" --config "$ACTRIX_CONFIG_PATH" >"$LOG_DIR/actrix.log" 2>&1 &
    ACTRIX_PID=$!

    if ! wait_for_http_ok "http://127.0.0.1:${HTTP_PORT}/signaling/health" 120; then
        cat "$LOG_DIR/actrix.log" >&2 || true
        fail "actrix did not become healthy on port $HTTP_PORT"
    fi
    success "actrix is healthy on http://127.0.0.1:${HTTP_PORT}"
}

login_admin() {
    section "🔐 Logging into Admin API"
    local response_file="$RUN_DIR/admin-login.json"
    curl -fsS \
        -X POST \
        "http://127.0.0.1:${HTTP_PORT}/admin/api/auth/login" \
        -H 'Content-Type: application/json' \
        -d "{\"password\":\"${ADMIN_PASSWORD}\"}" \
        >"$response_file"
    ADMIN_TOKEN="$(json_field "$response_file" '.token')"
    success "Admin API login succeeded"
}

warmup_ais_key() {
    section "🔑 Warming up AIS signing key"
    [ -n "${ADMIN_TOKEN:-}" ] || login_admin
    local current_key_file="$RUN_DIR/ais-current-key.json"
    local rotate_file="$RUN_DIR/ais-rotate-key.json"
    local attempt=0

    while [ $attempt -lt 60 ]; do
        if curl -fsS "http://127.0.0.1:${HTTP_PORT}/ais/current-key" >"$current_key_file" 2>/dev/null \
            && [ "$(jq -r '.status // "missing"' "$current_key_file" 2>/dev/null)" = "success" ]; then
            success "AIS signing key is ready"
            return 0
        fi

        curl -fsS -X POST \
            -H "Authorization: Bearer ${ADMIN_TOKEN}" \
            "http://127.0.0.1:${HTTP_PORT}/admin/api/services/ais/keys/rotate" \
            >"$rotate_file" 2>/dev/null || true
        sleep 1
        attempt=$((attempt + 1))
    done

    fail "AIS signing key warmup timed out"
}

ensure_realm() {
    section "🪪 Creating realm via Admin API"
    local create_file="$RUN_DIR/realm-create.json"
    local realm_name="package-runtime-echo-${RUN_ID}"
    curl -fsS \
        -X POST \
        "http://127.0.0.1:${HTTP_PORT}/admin/api/realms" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        -H 'Content-Type: application/json' \
        -d "{\"name\":\"${realm_name}\",\"enabled\":true,\"expires_at\":0}" \
        >"$create_file"

    REALM_ID="$(json_field "$create_file" '.realm.realm_id')"
    REALM_SECRET="$(json_field "$create_file" '.realm_secret')"

    [ -n "$REALM_ID" ] || fail "Realm creation returned an empty realm id"
    [ -n "$REALM_SECRET" ] || fail "Realm creation returned an empty realm secret"
    # The secret is interpolated into TOML string literals below; URL-safe
    # base64 stays inside that grammar, so reject anything else before it can
    # make .actr/config.toml unparseable.
    case "$REALM_SECRET" in
        *[!A-Za-z0-9_-]*) fail "realm_secret has characters outside URL-safe base64" ;;
    esac
    success "Realm ${REALM_ID} created"
}

append_workspace_patch() {
    local cargo_toml="$1"
    local repo_path="$REPO_ROOT"

    if ! grep -q '^\[workspace\]' "$cargo_toml"; then
        cat >>"$cargo_toml" <<'EOF'

[workspace]
EOF
    fi

    if grep -q '^\[patch\.crates-io\]' "$cargo_toml"; then
        return 0
    fi

    cat >>"$cargo_toml" <<EOF

[patch.crates-io]
actr = { path = "$repo_path/crates/actr" }
actr-config = { path = "$repo_path/crates/actr-config" }
actr-protocol = { path = "$repo_path/crates/actr-protocol" }
actr-framework = { path = "$repo_path/crates/actr-framework" }
actr-hyper = { path = "$repo_path/crates/actr-hyper" }
actr-pack = { path = "$repo_path/crates/actr-pack" }
actr-platform-native = { path = "$repo_path/crates/actr-platform-native" }
actr-platform-traits = { path = "$repo_path/crates/actr-platform-traits" }
actr-runtime = { path = "$repo_path/crates/actr-runtime" }
actr-runtime-mailbox = { path = "$repo_path/crates/actr-runtime-mailbox" }
actr-service-compat = { path = "$repo_path/crates/actr-service-compat" }
EOF
}

write_project_keychain_config() {
    local project_dir="$1"
    local keychain_path="$2"
    mkdir -p "$project_dir/.actr"
    cat >"$project_dir/.actr/config.toml" <<EOF
[mfr]
keychain = "$keychain_path"
EOF
}

provision_mfr_keychain() {
    provision_e2e_publisher
}

scaffold_service_guest() {
    section "🧱 Scaffolding temporary echo service"
    run_actr init \
        -l rust \
        --template echo \
        --role service \
        --signaling "ws://127.0.0.1:${HTTP_PORT}/signaling/ws" \
        --manufacturer "$MANUFACTURER" \
        "$TMP_SERVICE_DIR"

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

    SERVICE_VERSION="$(
        awk '
            /^\[package\]/ { in_package = 1; next }
            /^\[/ && in_package { exit }
            in_package && $1 == "version" {
                gsub(/"/, "", $3)
                print $3
                exit
            }
        ' "$TMP_SERVICE_DIR/manifest.toml"
    )"

    [ -n "$SERVICE_VERSION" ] || fail "Unable to detect temporary service version"
    success "Temporary echo service ready: version ${SERVICE_VERSION}"
}

build_service_package() {
    section "📦 Building the server package"
    SERVICE_PACKAGE="$DIST_DIR/${MANUFACTURER}-EchoService-${SERVICE_VERSION}-${HOST_TARGET}.actr"

    (
        cd "$TMP_SERVICE_DIR"
        CARGO_TARGET_DIR="$TEMP_SERVICE_TARGET_DIR" run_actr build \
            --manifest-path manifest.toml \
            --key "$SERVICE_KEYCHAIN" \
            --output "$SERVICE_PACKAGE"
    )

    [ -f "$SERVICE_PACKAGE" ] || fail "Server package missing: $SERVICE_PACKAGE"
    run_actr pkg verify --pubkey "$SERVICE_PUBLIC_KEY" --package "$SERVICE_PACKAGE" >/dev/null
    success "Server package built: $SERVICE_PACKAGE"
}

publish_service_package() {
    section "📣 Publishing the server package to the registry"
    run_actr registry publish \
        --package "$SERVICE_PACKAGE" \
        --keychain "$SERVICE_KEYCHAIN" \
        --endpoint "http://127.0.0.1:${HTTP_PORT}"
    success "Server package published"
}

client_guest_library_path() {
    case "$(uname)" in
        Darwin)
            printf '%s\n' "$WORKSPACE_TARGET_DIR/debug/libpackage_runtime_echo_client_guest.dylib"
            ;;
        Linux)
            printf '%s\n' "$WORKSPACE_TARGET_DIR/debug/libpackage_runtime_echo_client_guest.so"
            ;;
        *)
            printf '%s\n' "$WORKSPACE_TARGET_DIR/debug/package_runtime_echo_client_guest.dll"
            ;;
    esac
}

build_client_guest_package() {
    section "📦 Building client guest package"

    CARGO_TARGET_DIR="$WORKSPACE_TARGET_DIR" cargo build --manifest-path "$SCRIPT_DIR/client-guest/Cargo.toml" >/dev/null

    local client_guest_binary
    client_guest_binary="$(client_guest_library_path)"
    [ -f "$client_guest_binary" ] || fail "Client guest library missing: $client_guest_binary"

    local client_guest_dir client_guest_manifest
    client_guest_dir="$RUN_DIR/client-guest-package"
    client_guest_manifest="$client_guest_dir/manifest.toml"
    mkdir -p "$client_guest_dir/.actr"
    cp "$SCRIPT_DIR/client-guest/manifest.toml" "$client_guest_manifest"
    # Package builds now require a lock that exactly matches every declared
    # remote dependency. The CLI reads discovery settings from its project-level
    # .actr/config.toml, independently of the runtime actr.toml schema.
    cat >"$client_guest_dir/.actr/config.toml" <<EOF
version = 1

[network]
signaling_url = "ws://127.0.0.1:${HTTP_PORT}/signaling/ws"
ais_endpoint = "http://127.0.0.1:${HTTP_PORT}/ais"
realm_id = $REALM_ID
realm_secret = "$REALM_SECRET"
EOF
    (
        cd "$client_guest_dir"
        run_actr deps install
    )
    cat >>"$client_guest_manifest" <<EOF

[binary]
path = "$client_guest_binary"
target = "$HOST_TARGET"
EOF

    run_actr build \
        --no-compile \
        --manifest-path "$client_guest_manifest" \
        --key "$PROVISIONED_KEYCHAIN" \
        --target "$HOST_TARGET" \
        --output "$CLIENT_GUEST_PACKAGE"

    [ -f "$CLIENT_GUEST_PACKAGE" ] || fail "Client guest package missing: $CLIENT_GUEST_PACKAGE"

    cp "$PROVISIONED_PUBLIC_KEY" "$CLIENT_GUEST_PUBLIC_KEY"
    run_actr pkg verify --pubkey "$CLIENT_GUEST_PUBLIC_KEY" --package "$CLIENT_GUEST_PACKAGE" >/dev/null
    success "Client guest package ready"
}

seed_client_registry_state() {
    section "🗂️  Seeding client registry metadata"
    python3 - "$ACTRIX_DB" "$CLIENT_GUEST_PACKAGE" "$CLIENT_GUEST_PUBLIC_KEY" <<'PY'
import base64
import hashlib
import json
import sqlite3
import sys
import time
import zipfile

import tomllib

db_path, package_path, public_key_path = sys.argv[1:]
now = int(time.time())
key_expires_at = now + 365 * 24 * 3600

with open(public_key_path, "r", encoding="utf-8") as fh:
    public_key = json.load(fh)["public_key"]
# Same key-id convention as the CLI/server (actr_pack::compute_key_id).
key_bytes = base64.b64decode(public_key, validate=True)
key_id = "actr-pub-" + hashlib.sha256(key_bytes).hexdigest()[:16]

with zipfile.ZipFile(package_path, "r") as zf:
    manifest = zf.read("manifest.toml").decode("utf-8")
    signature = base64.b64encode(zf.read("manifest.sig")).decode("ascii")

manifest_data = tomllib.loads(manifest)
manufacturer = manifest_data["publisher"]
name = manifest_data["name"]
version = manifest_data["version"]
target = manifest_data.get("binary", {}).get("target", "cdylib")
type_str = f"{manufacturer}:{name}:{version}"

conn = sqlite3.connect(db_path)
try:
    cur = conn.cursor()
    cur.execute(
        """
        INSERT OR IGNORE INTO mfr
            (name, public_key, key_id, contact, status, created_at, updated_at, verified_at, key_expires_at)
        VALUES (?, ?, ?, ?, 'active', ?, ?, ?, ?)
        """,
        (manufacturer, public_key, key_id, "e2e@local.actr", now, now, now, key_expires_at),
    )
    cur.execute(
        """
        UPDATE mfr
           SET public_key = ?,
               key_id = ?,
               status = 'active',
               updated_at = ?,
               verified_at = ?,
               key_expires_at = ?,
               suspended_at = NULL,
               revoked_at = NULL
         WHERE name = ?
        """,
        (public_key, key_id, now, now, key_expires_at, manufacturer),
    )
    cur.execute("SELECT id FROM mfr WHERE name = ?", (manufacturer,))
    mfr_id = cur.fetchone()[0]
    cur.execute(
        """
        INSERT INTO mfr_package
            (mfr_id, manufacturer, name, version, type_str, target, manifest, signature, status, published_at, revoked_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL)
        ON CONFLICT(manufacturer, name, version, target) DO UPDATE SET
            mfr_id = excluded.mfr_id,
            type_str = excluded.type_str,
            manifest = excluded.manifest,
            signature = excluded.signature,
            status = 'active',
            published_at = excluded.published_at,
            revoked_at = NULL
        """,
        (mfr_id, manufacturer, name, version, type_str, target, manifest, signature, now),
    )
    conn.commit()
finally:
    conn.close()
PY

    success "Client registry state seeded"
}

render_client_runtime_config() {
    render_template \
        "$SCRIPT_DIR/config/client-runtime.toml.tpl" \
        "$CLIENT_RUNTIME_PATH" \
        "__REALM_ID__=$REALM_ID" \
        "__ECHO_SERVICE_VERSION__=$SERVICE_VERSION" \
        "__REALM_SECRET__=$REALM_SECRET"
}

run_client_and_assert() {
    section "🚀 Running client host"
    render_client_runtime_config
    CARGO_TARGET_DIR="$WORKSPACE_TARGET_DIR" cargo build --manifest-path "$SCRIPT_DIR/client/Cargo.toml" >/dev/null

    # The client host registers its packaged workload with AIS, which (AIS-02)
    # requires a runner possession proof of the signing key — pass the same
    # publisher keychain that signed the client package. The isolated HOME only
    # keeps the hyper data dir away from the developer's ~/.actr.
    local client_home="$RUN_DIR/home-client"
    mkdir -p "$client_home"

    (
        sleep 3
        echo "$TEST_INPUT"
        sleep 2
        echo "quit"
    ) | \
        HOME="$client_home" \
        ECHO_ACTR_VERSION="$SERVICE_VERSION" \
        CLIENT_RUNTIME_CONFIG_PATH="$CLIENT_RUNTIME_PATH" \
        CLIENT_GUEST_PACKAGE_PATH="$CLIENT_GUEST_PACKAGE" \
        CLIENT_GUEST_PUBLIC_KEY_PATH="$CLIENT_GUEST_PUBLIC_KEY" \
        CLIENT_GUEST_KEYCHAIN_PATH="$PROVISIONED_KEYCHAIN" \
        RUST_LOG="${ACTR_E2E_RUST_LOG:-info}" \
        "$WORKSPACE_TARGET_DIR/debug/package-runtime-echo-client" \
        >"$LOG_DIR/client.log" 2>&1 &
    CLIENT_PID=$!

    local timeout="${CLIENT_TIMEOUT_SECONDS:-40}"
    local attempt=0
    while kill -0 "$CLIENT_PID" 2>/dev/null && [ $attempt -lt "$timeout" ]; do
        sleep 1
        attempt=$((attempt + 1))
    done

    if kill -0 "$CLIENT_PID" 2>/dev/null; then
        kill "$CLIENT_PID" 2>/dev/null || true
        fail "Client host timed out after ${timeout}s"
    fi

    if grep -q "\[Received reply\].*Echo: ${TEST_INPUT}" "$LOG_DIR/client.log"; then
        success "End-to-end echo succeeded"
        grep "Received reply" "$LOG_DIR/client.log" || true
        return 0
    fi

    echo ""
    echo "Client log:"
    cat "$LOG_DIR/client.log" || true
    echo ""
    echo "Server log:"
    cat "$LOG_DIR/server.log" || true
    echo ""
    echo "Actrix log:"
    cat "$LOG_DIR/actrix.log" || true
    fail "Expected echo reply not found"
}

# ============================================================================
# Runner-signature / Path 2 mode helpers (P0 isolation + assertions)
# ============================================================================

# Per-mode isolated HOME and hyper data dir, so actr run never touches the real
# ~/.actr/config.toml or shares local state across modes/phases.
mk_isolated_env() {
    local label="$1"
    local home_dir="$RUN_DIR/home-$label"
    local hyper_dir="$RUN_DIR/hyper-$label"
    mkdir -p "$home_dir" "$hyper_dir"
    printf '%s\t%s\n' "$home_dir" "$hyper_dir"
}

# Write the mfr.keychain into the GLOBAL config of an isolated HOME (CWD-independent,
# since resolve_effective_cli_config reads ~/.actr/config.toml via dirs::home_dir()).
write_global_keychain_config() {
    local home_dir="$1"
    local keychain_path="$2"
    mkdir -p "$home_dir/.actr"
    cat >"$home_dir/.actr/config.toml" <<EOF
[mfr]
keychain = "$keychain_path"
EOF
}

# E2: assert neither the isolated HOME nor the CWD carries a keychain config,
# so build_runner_auth_provider() is deterministically None.
assert_no_keychain_config() {
    local home_dir="$1"
    if [ -f "$home_dir/.actr/config.toml" ]; then
        fail "E2 precondition violated: $home_dir/.actr/config.toml exists"
    fi
    if [ -f ".actr/config.toml" ]; then
        fail "E2 precondition violated: CWD .actr/config.toml exists"
    fi
    success "Confirmed no keychain config present (provider will be None)"
}

# P0-2: assert no active mfr_package row for type_str+target, so the request
# cannot accidentally pass via Path 1.
assert_no_active_package() {
    local type_str="$1"
    local target="$2"
    local count
    count="$(sqlite3 "$ACTRIX_DB" \
        "SELECT COUNT(*) FROM mfr_package WHERE type_str='$type_str' AND target='$target' AND status='active';" 2>/dev/null || echo 0)"
    if [ "$count" != "0" ]; then
        fail "Precondition violated: $count active mfr_package row(s) for type_str=$type_str target=$target (Path 1 would shadow Path 2)"
    fi
    success "Confirmed no active published package for $type_str / $target"
}

# Render the server runtime config (reuses the existing template).
render_server_runtime() {
    render_template \
        "$SCRIPT_DIR/config/server-runtime.toml.tpl" \
        "$SERVER_RUNTIME_PATH" \
        "__PACKAGE_PATH__=$SERVICE_PACKAGE" \
        "__REALM_ID__=$REALM_ID" \
        "__REALM_SECRET__=$REALM_SECRET"
}

# Launch actr run fully isolated — independent HOME and --hyper-dir.
# $1=home_dir $2=hyper_dir $3=log_name.
# Sets SERVER_PID and writes to $LOG_DIR/<log_name>.
start_server_isolated() {
    local home_dir="$1"
    local hyper_dir="$2"
    local log_name="${3:-server.log}"
    render_server_runtime
    HOME="$home_dir" \
        RUST_LOG="${ACTR_E2E_RUST_LOG:-info}" \
        CARGO_TARGET_DIR="$ACTR_TARGET_DIR" \
        "$ACTR_CLI_BIN" run -c "$SERVER_RUNTIME_PATH" --hyper-dir "$hyper_dir" \
        >"$LOG_DIR/$log_name" 2>&1 &
    SERVER_PID=$!
}

# Poll for successful startup (Path 2 success surfaces as "ActrNode started").
expect_server_started() {
    local log_name="${1:-server.log}"
    local attempt=0
    while [ $attempt -lt 30 ]; do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            cat "$LOG_DIR/$log_name" >&2 || true
            fail "Server host exited before becoming ready (expected success)"
        fi
        if grep -q "ActrNode started" "$LOG_DIR/$log_name" 2>/dev/null; then
            success "Server host started (AIS registration succeeded)"
            return 0
        fi
        sleep 1
        attempt=$((attempt + 1))
    done
    fail "Server host did not report ready within 30s"
}

# Poll for expected rejection: server must exit non-zero and log a rejection.
# Rejection can happen at two stages: AIS registration ("Failed to register with
# AIS") for missing/wrong runner proof, or earlier at package attach
# ("Failed to attach package: untrusted manufacturer") when the MFR key is
# revoked and cert_cache cannot fetch the pubkey.
expect_server_rejected() {
    local log_name="${1:-server.log}"
    local attempt=0
    while [ $attempt -lt 30 ]; do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            if grep -qE "Failed to register with AIS|Failed to attach package|untrusted manufacturer" "$LOG_DIR/$log_name" 2>/dev/null; then
                success "Server host rejected as expected"
                return 0
            fi
            cat "$LOG_DIR/$log_name" >&2 || true
            fail "Server host exited but rejection marker not found"
        fi
        sleep 1
        attempt=$((attempt + 1))
    done
    # Still running after 30s — registration did not fail as expected.
    kill "$SERVER_PID" 2>/dev/null || true
    cat "$LOG_DIR/$log_name" >&2 || true
    fail "Server host was not rejected within 30s (expected rejection)"
}

stop_server() {
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    SERVER_PID=""
}

# Assert actrix.log carries the Path 2 success marker (and NOT the Path 1 marker),
# proving registration really went through Path 2. Path 2 logs
# "Publisher Registry manifest and manufacturer signature verification passed"
# (issuer.rs); Path 1 logs "Publisher Registry table lookup passed", so the two
# markers are mutually exclusive.
assert_path2_taken() {
    if ! grep -q "manifest and manufacturer signature verification passed" "$LOG_DIR/actrix.log" 2>/dev/null; then
        fail "Path 2 success marker not found in actrix.log (did it take Path 1?)"
    fi
    if grep -q "Publisher Registry table lookup passed" "$LOG_DIR/actrix.log" 2>/dev/null; then
        fail "Path 1 marker found in actrix.log (should have taken Path 2)"
    fi
    success "Confirmed registration went through Path 2"
}

# Symmetric counterpart to assert_path2_taken: assert registration went through
# Path 1 (published package, registry table lookup) and NOT Path 2. This catches
# a silent Path 1 -> Path 2 fallback: if the registry lookup were to miss (e.g.
# a target/manifest-hash matching regression), a published package with a
# configured keychain would still succeed via Path 2 and the mode would stay
# green without actually exercising Path 1. Path 1 logs "Publisher Registry
# table lookup passed" (issuer.rs); Path 2 logs "...manifest and manufacturer
# signature verification passed", so the two markers are mutually exclusive.
assert_path1_taken() {
    if ! grep -q "Publisher Registry table lookup passed" "$LOG_DIR/actrix.log" 2>/dev/null; then
        fail "Path 1 success marker not found in actrix.log (did registration fall through to Path 2, or was the registry lookup missed?)"
    fi
    if grep -q "Publisher Registry manifest and manufacturer signature verification passed" "$LOG_DIR/actrix.log" 2>/dev/null; then
        fail "Path 2 marker found in actrix.log (should have taken Path 1; the runner proof must resolve through the registry lookup, not the Path 2 signature branch)"
    fi
    success "Confirmed registration went through Path 1"
}

stop_actrix() {
    if [ -n "$ACTRIX_PID" ] && kill -0 "$ACTRIX_PID" 2>/dev/null; then
        kill "$ACTRIX_PID" 2>/dev/null || true
        wait "$ACTRIX_PID" 2>/dev/null || true
    fi
    ACTRIX_PID=""
}

# ============================================================================
# Mode flows
# ============================================================================

mode_path1_full_echo() {
    # Original happy path: publish → run server → client echo round-trip.
    # AIS-02: a published package registers via Path 1 only with a runner
    # possession proof of the publisher's current signing key, so the server
    # host receives the keychain through its isolated HOME.
    publish_service_package
    local env
    env="$(mk_isolated_env "p1")"
    local home_dir hyper_dir
    IFS=$'\t' read -r home_dir hyper_dir <<<"$env"
    write_global_keychain_config "$home_dir" "$SERVICE_KEYCHAIN"
    start_server_isolated "$home_dir" "$hyper_dir" "server.log"
    expect_server_started "server.log"
    # Dependency installation resolves the service over signaling, so the
    # published service must also be online before the client guest lockfile is
    # generated. Reuse this server for the final echo round-trip.
    build_client_guest_package
    seed_client_registry_state
    run_client_and_assert
    stop_server
}

mode_path1_with_keychain() {
    section "🧪 MODE=path1-with-keychain (Path 1 with runner proof)"
    publish_service_package
    local env
    env="$(mk_isolated_env "p1kc")"
    local home_dir hyper_dir
    IFS=$'\t' read -r home_dir hyper_dir <<<"$env"
    write_global_keychain_config "$home_dir" "$SERVICE_KEYCHAIN"
    start_server_isolated "$home_dir" "$hyper_dir" "server.log"
    # AIS-02: a published package succeeds via Path 1 only with the runner
    # possession proof. Assert Path 1 was actually taken (not a silent fallback
    # to Path 2, which would also succeed here because the keychain holds key A).
    expect_server_started "server.log"
    assert_path1_taken
    stop_server
}

mode_path2_happy() {
    section "🧪 MODE=path2-happy (unpublished package + runner proof → Path 2 success)"
    local type_str="${MANUFACTURER}:EchoService:${SERVICE_VERSION}"
    assert_no_active_package "$type_str" "$HOST_TARGET"
    local env
    env="$(mk_isolated_env "p2h")"
    local home_dir hyper_dir
    IFS=$'\t' read -r home_dir hyper_dir <<<"$env"
    write_global_keychain_config "$home_dir" "$SERVICE_KEYCHAIN"
    start_server_isolated "$home_dir" "$hyper_dir" "server.log"
    expect_server_started "server.log"
    assert_path2_taken
    stop_server
}

mode_path2_no_proof() {
    section "🧪 MODE=path2-no-proof (unpublished package, no keychain → Path 2 rejection)"
    local type_str="${MANUFACTURER}:EchoService:${SERVICE_VERSION}"
    assert_no_active_package "$type_str" "$HOST_TARGET"
    local env
    env="$(mk_isolated_env "p2np")"
    local home_dir hyper_dir
    IFS=$'\t' read -r home_dir hyper_dir <<<"$env"
    assert_no_keychain_config "$home_dir"
    start_server_isolated "$home_dir" "$hyper_dir" "server.log"
    expect_server_rejected "server.log"
    # The no-proof rejection is deterministic: with no keychain the runtime
    # sends manifest_raw + mfr_signature but no manufacturer_auth_* triple, so
    # Path 2 must emit this marker before rejecting. Make it a hard assertion so
    # a rejection at the wrong stage (e.g. attach-time) cannot pass this mode.
    if ! grep -q "unpublished package requires" "$LOG_DIR/actrix.log" 2>/dev/null; then
        fail "Expected 'unpublished package requires ...' rejection marker in actrix.log (Path 2 no-proof rejection did not fire as expected)"
    fi
    stop_server
}

mode_path2_wrong_key() {
    section "🧪 MODE=path2-wrong-key (runner proof signed by unrelated key → rejection)"
    local type_str="${MANUFACTURER}:EchoService:${SERVICE_VERSION}"
    assert_no_active_package "$type_str" "$HOST_TARGET"

    local env
    env="$(mk_isolated_env "p2wk")"
    local home_dir hyper_dir
    IFS=$'\t' read -r home_dir hyper_dir <<<"$env"

    # Generate an unrelated key B with an ISOLATED HOME: `actr pkg keygen`
    # writes the keychain path into ~/.actr/config.toml, so it must not touch
    # the real home (would pollute other modes that read the real config).
    local wrong_keychain="$RUN_DIR/wrong-key.json"
    HOME="$home_dir" run_actr pkg keygen --output "$wrong_keychain" --force >/dev/null
    write_global_keychain_config "$home_dir" "$wrong_keychain"

    start_server_isolated "$home_dir" "$hyper_dir" "server.log"
    expect_server_rejected "server.log"
    stop_server
}

mode_rotation() {
    section "🧪 MODE=rotation (retired key passes, revoked key rejects)"
    local type_str="${MANUFACTURER}:EchoService:${SERVICE_VERSION}"
    assert_no_active_package "$type_str" "$HOST_TARGET"

    # The manifest's signing_key_id (key A) — needed to find the history_id after rotation.
    local signing_key_id
    signing_key_id="$(python3 - "$SERVICE_PACKAGE" <<'PY'
import sys, tomllib, zipfile
with zipfile.ZipFile(sys.argv[1]) as zf:
    m = tomllib.loads(zf.read("manifest.toml").decode())
print(m["signing_key_id"])
PY
)"
    [ -n "$signing_key_id" ] || fail "Could not read signing_key_id from manifest"

    # Look up the publisher fixture row.
    local mfr_id
    mfr_id="$(sqlite3 "$ACTRIX_DB" "SELECT id FROM mfr WHERE name='${MANUFACTURER}';")"
    [ -n "$mfr_id" ] || fail "Could not resolve mfr_id for ${MANUFACTURER}"

    # --- Phase 1: rotate A→B (A archived as retired), then run with keychain A. ---
    section "🔄 Phase 1: rotate key (A → retired), expect Path 2 success"
    local rotation_keychain="$RUN_DIR/mfr-rotation-b.keychain.json"
    run_actr pkg keygen --output "$rotation_keychain" --force >/dev/null
    python3 - "$ACTRIX_DB" "$MANUFACTURER" "$rotation_keychain" <<'PY'
import base64
import hashlib
import json
import sqlite3
import sys
import time

db_path, publisher, keychain_path = sys.argv[1:]
with open(keychain_path, encoding="utf-8") as stream:
    public_key = json.load(stream)["public_key"]
key_id = "actr-pub-" + hashlib.sha256(base64.b64decode(public_key, validate=True)).hexdigest()[:16]
now = int(time.time())
connection = sqlite3.connect(db_path, timeout=30)
connection.execute("PRAGMA foreign_keys = ON")
with connection:
    row = connection.execute(
        "SELECT id, key_id, public_key, COALESCE(verified_at, created_at) FROM mfr WHERE name = ?",
        (publisher,),
    ).fetchone()
    if row is None:
        raise SystemExit(f"publisher fixture not found: {publisher}")
    mfr_id, old_key_id, old_public_key, old_created_at = row
    connection.execute(
        """
        INSERT INTO mfr_key_history
            (mfr_id, key_id, public_key, status, created_at, retired_at)
        VALUES (?, ?, ?, 'retired', ?, ?)
        """,
        (mfr_id, old_key_id, old_public_key, old_created_at, now),
    )
    connection.execute(
        """
        UPDATE mfr
        SET public_key = ?, key_id = ?, key_expires_at = ?, updated_at = ?
        WHERE id = ?
        """,
        (public_key, key_id, now + 365 * 86400, now, mfr_id),
    )
connection.close()
PY

    local env1
    env1="$(mk_isolated_env "rot1")"
    local home1 hyper1
    IFS=$'\t' read -r home1 hyper1 <<<"$env1"
    write_global_keychain_config "$home1" "$SERVICE_KEYCHAIN"
    start_server_isolated "$home1" "$hyper1" "server-phase1.log"
    expect_server_started "server-phase1.log"
    assert_path2_taken
    stop_server

    # --- Phase 2: revoke key A by history_id (matched by signing_key_id, not array order). ---
    section "🚫 Phase 2: revoke retired key A, expect rejection"
    local revoked_rows
    revoked_rows="$(sqlite3 "$ACTRIX_DB" \
        "UPDATE mfr_key_history SET status='revoked' WHERE mfr_id=${mfr_id} AND key_id='${signing_key_id}'; SELECT changes();")"
    [ "$revoked_rows" = "1" ] || fail "Could not revoke historical key ${signing_key_id}"

    # Re-confirm no published package row, so rejection comes from KeyRevoked, not Path 1.
    assert_no_active_package "$type_str" "$HOST_TARGET"

    local env2
    env2="$(mk_isolated_env "rot2")"
    local home2 hyper2
    IFS=$'\t' read -r home2 hyper2 <<<"$env2"
    write_global_keychain_config "$home2" "$SERVICE_KEYCHAIN"
    start_server_isolated "$home2" "$hyper2" "server-phase2.log"
    expect_server_rejected "server-phase2.log"
    stop_server
}

# ============================================================================
# Main
# ============================================================================

section "🧪 Package Runtime Echo E2E (MODE=${MODE})"
echo "Run directory: $RUN_DIR"
echo "Backend:       $BACKEND"
echo "Message:       $TEST_INPUT"
echo "Actrix binary: $ACTRIX_BIN"

render_runtime_configs
build_local_actr_cli
build_local_actrix
# Modes that assert which path was taken need the debug-level Path 1 / Path 2
# verification markers ("Publisher Registry table lookup passed" /
# "...manifest and manufacturer signature verification passed") in actrix.log.
case "$MODE" in
    path1-with-keychain|path2-happy|rotation) export ACTRIX_RUST_LOG="info,actrix::observability=debug" ;;
esac
start_actrix
login_admin
warmup_ais_key
ensure_realm
provision_mfr_keychain
scaffold_service_guest
build_service_package

case "$MODE" in
    path1)               mode_path1_full_echo ;;
    path1-with-keychain) mode_path1_with_keychain ;;
    path2-happy)         mode_path2_happy ;;
    path2-no-proof)      mode_path2_no_proof ;;
    path2-wrong-key)     mode_path2_wrong_key ;;
    rotation)            mode_rotation ;;
esac

echo ""
success "Package runtime echo E2E (MODE=${MODE}) completed successfully"
