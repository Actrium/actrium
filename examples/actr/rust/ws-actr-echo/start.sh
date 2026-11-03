#!/bin/bash
# Test script for ws-actr-echo example - Echo using WebSocket direct connection
#
# Demo flow:
#   1. Start loran signaling server
#   2. Write realm data to database
#   3. Start ws-echo-server (listen WebSocket on port 9001, register with signaling)
#   4. Start ws-echo-client (discover server WebSocket address from signaling, then direct connect)
#   5. Verify Echo RPC via WebSocket channel
#
# Usage:
#   ./start.sh              # Use default message "WsTest"
#   ./start.sh "Hello World"    # Send custom message

set -e
set -o pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "🔌 Testing ws-actr-echo (WebSocket Direct Connection)"
echo "    Server ↔ Client via WebSocket (no WebRTC)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Determine paths
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ACTOR_RTC_DIR="$(cd "$WORKSPACE_ROOT/../../.." && pwd)"
LORAN_DIR="$ACTOR_RTC_DIR"
WS_ECHO_DIR="$SCRIPT_DIR"
SERVER_DIR="$WS_ECHO_DIR/server"
CLIENT_DIR="$WS_ECHO_DIR/client"
PROTO_DIR="$WS_ECHO_DIR/proto"
LORAN_CONFIG="$WS_ECHO_DIR/loran-config.toml"

# WebSocket service port and database path
WS_LISTEN_PORT=9001
LORAN_PORT=8081
# Database path: relative to WORKSPACE_ROOT (loran CWD)
DB_PATH="$WORKSPACE_ROOT/ws-echo-db/loran.db"

# Switch to workspace root and stay there
cd "$WORKSPACE_ROOT"

# Create logs directory
LOG_DIR="$WORKSPACE_ROOT/logs"
mkdir -p "$LOG_DIR"

# Ensure required CLI tools
source "$WORKSPACE_ROOT/scripts/ensure-tools.sh"

echo ""
echo "🔍 Checking actr.toml config files..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Function: copy config file from example.toml
ensure_toml() {
    local target="$1"
    local example="$2"
    if [ ! -f "$target" ]; then
        if [ -f "$example" ]; then
            echo "📋 Copying $(basename "$example") → $(basename "$target")"
            cp "$example" "$target"
        else
            echo -e "${RED}❌ Cannot find $example${NC}" >&2
            return 1
        fi
    fi
}

ensure_toml "$SERVER_DIR/actr.toml"     "$SERVER_DIR/Actr.example.toml"
ensure_toml "$CLIENT_DIR/actr.toml"     "$CLIENT_DIR/Actr.example.toml"
ensure_toml "$LORAN_CONFIG"            "$WS_ECHO_DIR/loran-config.example.toml"

echo ""
echo "🧰 Checking required CLI tools..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
ensure_cargo_bin "protoc-gen-prost"           "protoc-gen-prost"                  "$LOG_DIR"
ensure_cargo_bin "protoc-gen-actrframework"   "actr-framework-protoc-codegen"     "$LOG_DIR"
ensure_cargo_bin "actr"                       "actr-cli"                          "$LOG_DIR"

# Cleanup function
cleanup() {
    echo ""
    echo "🧹 Cleaning up processes..."

    if [ ! -z "$LORAN_PID" ]; then
        echo "Stopping loran (PID: $LORAN_PID)"
        kill $LORAN_PID 2>/dev/null || true
    fi

    if [ ! -z "$SERVER_PID" ]; then
        echo "Stopping ws-echo-server (PID: $SERVER_PID)"
        kill $SERVER_PID 2>/dev/null || true
    fi

    if [ ! -z "$CLIENT_PID" ]; then
        echo "Stopping ws-echo-client (PID: $CLIENT_PID)"
        kill $CLIENT_PID 2>/dev/null || true
    fi

    wait 2>/dev/null || true
    echo "✅ Cleanup complete"
}

trap cleanup EXIT INT TERM

# ─────────────────────────────────────────────────────────────
# Step 0: Generate server code (protobuf + actor glue)
# ─────────────────────────────────────────────────────────────
echo ""
echo "🛠️ Generating server code (actr gen)..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

ACTR_GEN_CMD=""
if command -v actr > /dev/null 2>&1; then
    ACTR_GEN_CMD="actr"
elif [ -x "$ACTOR_RTC_DIR/actr/target/debug/actr" ]; then
    ACTR_GEN_CMD="$ACTOR_RTC_DIR/actr/target/debug/actr"
elif [ -x "$ACTOR_RTC_DIR/actr/target/release/actr" ]; then
    ACTR_GEN_CMD="$ACTOR_RTC_DIR/actr/target/release/actr"
else
    echo -e "${RED}❌ Cannot find actr tool (ensure 'actr' is in PATH or compiled)${NC}"
    exit 1
fi

cd "$SERVER_DIR"
OUTPUT_FILE="$LOG_DIR/actr-gen-ws-echo-server.log"
$ACTR_GEN_CMD gen --input="$PROTO_DIR" --output=src/generated --clean > "$OUTPUT_FILE" 2>&1 || {
    echo -e "${RED}❌ actr gen (server) failed${NC}"
    cat "$OUTPUT_FILE"
    exit 1
}
cd "$WORKSPACE_ROOT"
echo -e "${GREEN}✅ Server code generated${NC}"

# Step 0b: Generate client code
echo ""
echo "🛠️ Generating client code (actr gen)..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

cd "$CLIENT_DIR"
OUTPUT_FILE="$LOG_DIR/actr-gen-ws-echo-client.log"
$ACTR_GEN_CMD gen --input="$PROTO_DIR" --output=src/generated --clean --no-scaffold > "$OUTPUT_FILE" 2>&1 || {
    echo -e "${RED}❌ actr gen (client) failed${NC}"
    cat "$OUTPUT_FILE"
    exit 1
}
cd "$WORKSPACE_ROOT"
echo -e "${GREEN}✅ Client code generated${NC}"

# ─────────────────────────────────────────────────────────────
# Step 1: Build and install loran
# ─────────────────────────────────────────────────────────────
echo ""
echo "📦 Building and installing loran..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ ! -d "$LORAN_DIR" ]; then
    echo -e "${RED}❌ Cannot find loran directory: $LORAN_DIR${NC}"
    exit 1
fi

cd "$LORAN_DIR"
cargo build --manifest-path "$LORAN_DIR/crates/loran-server/Cargo.toml" --features opentelemetry 2>&1

if [ ! -f "$LORAN_DIR/target/debug/loran" ]; then
    echo -e "${RED}❌ loran build failed${NC}"
    exit 1
fi

echo "Installing loran to ~/.cargo/bin..."
mkdir -p ~/.cargo/bin
cp "$LORAN_DIR/target/debug/loran" ~/.cargo/bin/loran
chmod +x ~/.cargo/bin/loran

cd "$WORKSPACE_ROOT"

if ! command -v loran > /dev/null 2>&1; then
    echo -e "${RED}❌ loran command not found after install, ensure ~/.cargo/bin is in PATH${NC}"
    exit 1
fi

echo -e "${GREEN}✅ loran installed: $(which loran)${NC}"

# ─────────────────────────────────────────────────────────────
# Step 2: Start loran signaling server
# ─────────────────────────────────────────────────────────────
echo ""
echo "🚀 Starting loran signaling server..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

loran --config "$LORAN_CONFIG" > "$LOG_DIR/loran.log" 2>&1 &
LORAN_PID=$!

echo "loran started (PID: $LORAN_PID)"
echo "Waiting for loran to be ready (port $LORAN_PORT)..."

MAX_WAIT=15
COUNTER=0
while [ $COUNTER -lt $MAX_WAIT ]; do
    if ! kill -0 $LORAN_PID 2>/dev/null; then
        echo -e "${RED}❌ loran failed to start${NC}"
        cat "$LOG_DIR/loran.log"
        exit 1
    fi

    if lsof -i:$LORAN_PORT > /dev/null 2>&1 || nc -z localhost $LORAN_PORT 2>/dev/null; then
        echo -e "${GREEN}✅ loran is listening on port $LORAN_PORT${NC}"
        break
    fi

    sleep 1
    COUNTER=$((COUNTER + 1))
done

if [ $COUNTER -eq $MAX_WAIT ]; then
    echo -e "${RED}❌ loran did not listen on port $LORAN_PORT within ${MAX_WAIT} seconds${NC}"
    cat "$LOG_DIR/loran.log"
    exit 1
fi

# ─────────────────────────────────────────────────────────────
# Step 3: Write realm data to database
# ─────────────────────────────────────────────────────────────
echo ""
echo "🔑 Writing realm data to loran database..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Wait for loran to create the database file (may take a few seconds)
echo "Waiting for database file creation..."
COUNTER=0
while [ $COUNTER -lt 10 ]; do
    if [ -f "$DB_PATH" ]; then
        echo -e "${GREEN}✅ Database file exists: $DB_PATH${NC}"
        break
    fi
    sleep 1
    COUNTER=$((COUNTER + 1))
done

if [ ! -f "$DB_PATH" ]; then
    echo -e "${RED}❌ loran database file not created: $DB_PATH${NC}"
    echo "loran log:"
    cat "$LOG_DIR/loran.log"
    exit 1
fi

# Check if sqlite3 is available
if ! command -v sqlite3 > /dev/null 2>&1; then
    echo -e "${RED}❌ sqlite3 command not found, cannot write realm data${NC}"
    echo "Please install sqlite3: brew install sqlite"
    exit 1
fi

# Insert realm data (matching actr.toml config)
REALM=ws-echo@example.com
sqlite3 "$DB_PATH" "INSERT OR IGNORE INTO realm (name, status, expires_at, created_at, updated_at) VALUES ('$REALM', 'Normal', NULL, strftime('%s','now'), strftime('%s','now'))"

# Verify insertion
REALM_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM realm WHERE name = '$REALM'")
if [ "$REALM_COUNT" -ge 1 ]; then
    echo -e "${GREEN}✅ Realm $REALM written to database${NC}"
else
    echo -e "${RED}❌ Realm insertion failed${NC}"
    exit 1
fi

# ─────────────────────────────────────────────────────────────
# Step 4: Build binaries
# ─────────────────────────────────────────────────────────────
echo ""
echo "🔨 Building binaries..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if ! cargo build --bin ws-echo-server --bin ws-echo-client-app 2>&1; then
    echo -e "${RED}❌ Build failed${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Build successful${NC}"

# ─────────────────────────────────────────────────────────────
# Step 5: Start ws-echo-server
# ─────────────────────────────────────────────────────────────
echo ""
echo "🚀 Starting ws-echo-server..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  - Will listen for WebSocket connections on port $WS_LISTEN_PORT"
echo "  - And register ws://127.0.0.1:$WS_LISTEN_PORT with signaling server"

RUST_LOG="${RUST_LOG:-info}" cargo run --bin ws-echo-server > "$LOG_DIR/ws-echo-server.log" 2>&1 &
SERVER_PID=$!

echo "ws-echo-server started (PID: $SERVER_PID)"
echo "Waiting for server to register with signaling server..."

MAX_WAIT=20
COUNTER=0
while [ $COUNTER -lt $MAX_WAIT ]; do
    if ! kill -0 $SERVER_PID 2>/dev/null; then
        echo -e "${RED}❌ ws-echo-server failed to start${NC}"
        cat "$LOG_DIR/ws-echo-server.log"
        exit 1
    fi

    if grep -q "WS Echo Server alreadyfully\|ActrNode startsuccess\|WebSocket addressalreadyreport" "$LOG_DIR/ws-echo-server.log" 2>/dev/null; then
        echo -e "${GREEN}✅ ws-echo-server started and registered${NC}"
        break
    fi

    sleep 1
    COUNTER=$((COUNTER + 1))
done

if [ $COUNTER -eq $MAX_WAIT ]; then
    echo -e "${YELLOW}⚠️  Server may not be fully registered yet, continuing...${NC}"
fi

# Extra wait to ensure WebSocket port is ready
sleep 2

# Verify server WebSocket port is listening
if lsof -i:$WS_LISTEN_PORT > /dev/null 2>&1 || nc -z localhost $WS_LISTEN_PORT 2>/dev/null; then
    echo -e "${GREEN}✅ WebSocket port $WS_LISTEN_PORT is ready${NC}"
else
    echo -e "${YELLOW}⚠️  WebSocket port $WS_LISTEN_PORT not ready yet (may still be initializing)${NC}"
fi

# ─────────────────────────────────────────────────────────────
# Step 6: Run ws-echo-client
# ─────────────────────────────────────────────────────────────
echo ""
echo "🚀 Running ws-echo-client..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ -n "$1" ]; then
    TEST_INPUT="$1"
else
    TEST_INPUT="WsTest"
fi

echo "Sending test message: \"$TEST_INPUT\""

(
    sleep 3
    echo "$TEST_INPUT"
    sleep 3
    echo "quit"
) | RUST_LOG="${RUST_LOG:-info}" cargo run --bin ws-echo-client-app > "$LOG_DIR/ws-echo-client.log" 2>&1 &
CLIENT_PID=$!

# Wait for client to finish (up to 15 seconds)
COUNTER=0
while kill -0 $CLIENT_PID 2>/dev/null && [ $COUNTER -lt 15 ]; do
    sleep 1
    COUNTER=$((COUNTER + 1))
done

if kill -0 $CLIENT_PID 2>/dev/null; then
    echo -e "${YELLOW}⚠️  Client timed out, force killing...${NC}"
    kill $CLIENT_PID 2>/dev/null || true
fi

# ─────────────────────────────────────────────────────────────
# Step 7: Verify output
# ─────────────────────────────────────────────────────────────
echo ""
echo "🔍 Verifying output..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Check if client received WS-Echo response from server
if grep -q "WS-Echo: $TEST_INPUT" "$LOG_DIR/ws-echo-client.log"; then
    echo -e "${GREEN}✅ Test passed: received WebSocket Echo response from server${NC}"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "🎉 ws-actr-echo test completed!"
    echo ""
    echo "📋 Client log summary:"
    grep -E "WS-Echo|WebSocket|discover|connect" "$LOG_DIR/ws-echo-client.log" | tail -10 || true
    echo ""
    echo "📋 Server log summary:"
    grep -E "\[WS\]|WebSocket|register|startsuccess" "$LOG_DIR/ws-echo-server.log" | tail -10 || true
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    exit 0
else
    echo -e "${RED}❌ Test failed: expected WebSocket Echo response not found${NC}"
    echo ""
    echo "📋 Client log (last 30 lines):"
    tail -30 "$LOG_DIR/ws-echo-client.log"
    echo ""
    echo "📋 Server log (last 30 lines):"
    tail -30 "$LOG_DIR/ws-echo-server.log"
    exit 1
fi
