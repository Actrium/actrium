#!/bin/bash
# Rill macOS E2E Test Runner
# Full automation: deps -> init -> build -> run -> parse results

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RILL_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$SCRIPT_DIR"

# CocoaPods cache: Use system default ~/Library/Caches/CocoaPods/ to share cache
# across projects (hermes-engine alone is 1.4GB).
# Only override if explicitly set via environment variable.
if [ -n "${CP_CACHE_DIR:-}" ]; then
    export CP_CACHE_DIR
fi
# Opt into a repo-scoped home if your environment blocks writing to ~/.cocoapods:
#   RILL_E2E_LOCAL_COCOAPODS_HOME=1
if [ "${RILL_E2E_LOCAL_COCOAPODS_HOME:-0}" = "1" ]; then
    COCOAPODS_WORK_DIR="$SCRIPT_DIR/macos/.cocoapods"
    export CP_HOME_DIR="${CP_HOME_DIR:-$COCOAPODS_WORK_DIR/home}"
fi

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step() { echo -e "\n${BLUE}==>${NC} $1"; }

# Trim leading/trailing whitespace
trim_ws() {
    local s="$1"
    # leading
    s="${s#"${s%%[![:space:]]*}"}"
    # trailing
    s="${s%"${s##*[![:space:]]}"}"
    echo "$s"
}

# ============================================
# Crash report helpers (macOS)
# ============================================
print_recent_crash_report() {
    local START_TS="${1:-0}"
    local REPORT_DIR="$HOME/Library/Logs/DiagnosticReports"

    if [ ! -d "$REPORT_DIR" ]; then
        return 0
    fi

    local newest_file=""
    local newest_mtime=0

    # Look for .ips (new) and .crash (old) reports.
    local f
    for f in "$REPORT_DIR"/RillMacOSTest-*.ips "$REPORT_DIR"/RillMacOSTest_*.crash; do
        [ -f "$f" ] || continue
        local mtime
        mtime="$(stat -f "%m" "$f" 2>/dev/null || echo 0)"
        if [ "$mtime" -ge "$START_TS" ] && [ "$mtime" -ge "$newest_mtime" ]; then
            newest_mtime="$mtime"
            newest_file="$f"
        fi
    done

    if [ -n "$newest_file" ]; then
        log_warn "Found crash report: $newest_file"
    fi
}

# ============================================
# Engine selection
# ============================================
# Format: <main>-<sandbox>
#   main:    jsc | hermes (host app's JS engine)
#   sandbox: jsc | quickjs | hermes (guest sandbox engine)
#
# Valid combinations:
#   jsc-jsc       : JSC main + JSC sandbox
#   jsc-quickjs   : JSC main + QuickJS sandbox
#   hermes-jsc    : Hermes main + JSC sandbox
#   hermes-quickjs: Hermes main + QuickJS sandbox
#   hermes-hermes : Hermes main + Hermes sandbox

normalize_main_engine() {
    case "${1:-jsc}" in
        jsc) echo "jsc" ;;
        hermes) echo "hermes" ;;
        *)
            log_error "Invalid main engine: $1 (expected: jsc|hermes)"
            exit 1
            ;;
    esac
}

normalize_sandbox_engine() {
    case "${1:-jsc}" in
        jsc|quickjs|hermes) echo "$1" ;;
        *)
            log_error "Invalid sandbox engine: $1 (expected: jsc|quickjs|hermes)"
            exit 1
            ;;
    esac
}

# Check if combination requires Hermes (USE_HERMES=1)
requires_hermes() {
    local MAIN="$1"
    local SANDBOX="$2"
    # Hermes main or Hermes sandbox both need hermes-engine
    [ "$MAIN" = "hermes" ] || [ "$SANDBOX" = "hermes" ]
}

# Parse a test spec like "jsc-jsc" or "hermes-quickjs"
parse_test_spec() {
    local spec="$1"
    local main=""
    local sandbox=""

    case "$spec" in
        jsc-jsc)
            main="jsc"; sandbox="jsc" ;;
        jsc-quickjs)
            main="jsc"; sandbox="quickjs" ;;
        hermes-jsc)
            main="hermes"; sandbox="jsc" ;;
        hermes-quickjs)
            main="hermes"; sandbox="quickjs" ;;
        hermes-hermes)
            main="hermes"; sandbox="hermes" ;;
        *)
            log_error "Invalid test spec: $spec"
            log_error "Valid specs: jsc-jsc, jsc-quickjs, hermes-jsc, hermes-quickjs, hermes-hermes"
            exit 1
            ;;
    esac

    echo "$main $sandbox"
}

# ============================================
# Metro / bundled JS mode
# ============================================
use_metro() {
    # Default: use pre-bundled JS (faster, no Watchman issues, CI-friendly).
    # Set RILL_E2E_USE_METRO=1 to use Metro dev server instead.
    [ "${RILL_E2E_USE_METRO:-0}" = "1" ]
}

bundle_cache_dir() {
    echo "$SCRIPT_DIR/.bundle-cache"
}

bundle_cache_bundle_path() {
    echo "$(bundle_cache_dir)/main.jsbundle"
}

bundle_cache_assets_dir() {
    echo "$(bundle_cache_dir)/assets"
}

ensure_bundle_cache() {
    local CACHE_DIR
    CACHE_DIR="$(bundle_cache_dir)"

    local BUNDLE_PATH
    BUNDLE_PATH="$(bundle_cache_bundle_path)"

    local ASSETS_DIR
    ASSETS_DIR="$(bundle_cache_assets_dir)"

    if [ -f "$BUNDLE_PATH" ] && [ "${RILL_E2E_BUNDLE_RESET:-0}" != "1" ]; then
        return 0
    fi

    rm -rf "$CACHE_DIR"
    mkdir -p "$ASSETS_DIR"

    log_info "Bundling JS (no Metro)..."
    npx react-native bundle \
        --platform macos \
        --dev false \
        --entry-file index.js \
        --bundle-output "$BUNDLE_PATH" \
        --assets-dest "$ASSETS_DIR"
}

install_bundled_js_into_app() {
    local APP_PATH="macos/build/Build/Products/Debug/RillMacOSTest.app"
    local RES_DIR="$APP_PATH/Contents/Resources"

    if [ ! -d "$APP_PATH" ]; then
        log_error "App not found at $APP_PATH (build first)"
        exit 1
    fi

    ensure_bundle_cache

    mkdir -p "$RES_DIR"
    cp "$(bundle_cache_bundle_path)" "$RES_DIR/main.jsbundle"

    local ASSETS_DIR
    ASSETS_DIR="$(bundle_cache_assets_dir)"
    if [ -d "$ASSETS_DIR" ]; then
        if command -v rsync >/dev/null 2>&1; then
            rsync -a "$ASSETS_DIR/" "$RES_DIR/"
        else
            cp -R "$ASSETS_DIR/"* "$RES_DIR/" 2>/dev/null || true
        fi
    fi
}

pod_cmd() {
    # Some environments inject proxy env vars pointing to localhost ports.
    # Do NOT strip proxies by default (many devs use a local proxy for GitHub).
    #
    # - Keep proxies (default): pod install uses your proxy config as-is.
    # - Strip localhost proxies (opt-in): set RILL_E2E_STRIP_LOCAL_PROXY=1
    # - Force keep (even if strip is enabled): set RILL_E2E_KEEP_PROXY=1
    if [ "${RILL_E2E_KEEP_PROXY:-0}" = "1" ] || [ "${RILL_E2E_STRIP_LOCAL_PROXY:-0}" != "1" ]; then
        pod "$@"
        return $?
    fi

    local should_strip_proxy=0
    local p
    for p in "${http_proxy:-}" "${https_proxy:-}" "${all_proxy:-}" "${HTTP_PROXY:-}" "${HTTPS_PROXY:-}" "${ALL_PROXY:-}"; do
        if [[ "$p" == *"127.0.0.1"* ]] || [[ "$p" == *"localhost"* ]]; then
            should_strip_proxy=1
            break
        fi
    done

    if [ "$should_strip_proxy" = "1" ]; then
        env -u http_proxy -u https_proxy -u all_proxy -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY pod "$@"
    else
        pod "$@"
    fi
}

pods_engine_marker_path() {
    echo "Pods/.rill_sandbox_engine"
}

pods_integration_ok() {
    # CocoaPods should create these when integration is complete.
    # If missing, we likely have a partial/aborted install.
    if [ ! -f "Pods/Manifest.lock" ]; then
        return 1
    fi

    local xcconfig_dir="Pods/Target Support Files/Pods-RillMacOSTest-macOS"
    if [ ! -f "$xcconfig_dir/Pods-RillMacOSTest-macOS.debug.xcconfig" ]; then
        return 1
    fi
    return 0
}

pods_install() {
    # Args: main sandbox
    # main:    jsc | hermes (host app's JS engine)
    # sandbox: jsc | quickjs | hermes (guest sandbox engine)
    local MAIN="${1:-jsc}"
    local SANDBOX="${2:-jsc}"

    # Determine USE_HERMES based on main and sandbox engines
    # USE_HERMES=1 when main=hermes OR sandbox=hermes (both need hermes-engine)
    local USE_HERMES_ENV=""
    if requires_hermes "$MAIN" "$SANDBOX"; then
        USE_HERMES_ENV="1"
    fi

    # Determine RILL_SANDBOX_ENGINE for podspec
    # Only set for non-default engines (default is JSC)
    local POD_ENGINE_ENV=""
    if [ "$SANDBOX" != "jsc" ]; then
        POD_ENGINE_ENV="$SANDBOX"
    fi

    log_info "Config: main=$MAIN sandbox=$SANDBOX USE_HERMES=${USE_HERMES_ENV:-0}"

    local MARKER
    MARKER="$(pods_engine_marker_path)"

    # Marker format: "main:sandbox:use_hermes"
    local CURRENT_MARKER="${MAIN}:${SANDBOX}:${USE_HERMES_ENV}"

    local PREV_MARKER=""
    if [ -f "$MARKER" ]; then
        PREV_MARKER="$(cat "$MARKER" 2>/dev/null || true)"
    fi

    # If config changed, prepare for reinstall
    if [ "$PREV_MARKER" != "$CURRENT_MARKER" ]; then
        log_info "Switching config: ${PREV_MARKER:-<none>} -> ${CURRENT_MARKER}"
        # Only remove RillSandboxNative related files, keep other pods
        rm -rf Pods/RillSandboxNative
        rm -rf Pods/Local\ Podspecs/RillSandboxNative.podspec.json
        # Clean build artifacts for RillSandboxNative to force recompile
        rm -rf build/Build/Intermediates.noindex/Pods.build/Debug/RillSandboxNative.build 2>/dev/null || true
        rm -rf build/Build/Products/Debug/RillSandboxNative 2>/dev/null || true
        rm -f build/Build/Products/Debug/libRillSandboxNative.a 2>/dev/null || true
        # Force pod install to re-integrate
        touch Podfile
    fi

    local NEED_INSTALL=0
    if [ ! -d "Pods" ]; then
        NEED_INSTALL=1
    elif [ "Podfile" -nt "Podfile.lock" ]; then
        NEED_INSTALL=1
    elif ! pods_integration_ok; then
        log_warn "Pods exist but integration looks incomplete; reinstalling..."
        rm -rf Pods
        NEED_INSTALL=1
    fi

    if [ "$NEED_INSTALL" = "1" ]; then
        log_info "Installing CocoaPods... (main=$MAIN sandbox=$SANDBOX)"
        # Prefer the official CocoaPods CDN over regional mirrors that can be flaky.
        export COCOAPODS_CDN_URL="${COCOAPODS_CDN_URL:-https://cdn.cocoapods.org/}"
        export COCOAPODS_DISABLE_STATS="${COCOAPODS_DISABLE_STATS:-1}"
        # Enable parallel downloads (CocoaPods 1.11+)
        export COCOAPODS_PARALLEL_DOWNLOADS="${COCOAPODS_PARALLEL_DOWNLOADS:-10}"

        # Export environment for pod install (pod_cmd is a shell function, can't use env)
        if [ -n "$POD_ENGINE_ENV" ]; then
            export RILL_SANDBOX_ENGINE="$POD_ENGINE_ENV"
        else
            unset RILL_SANDBOX_ENGINE 2>/dev/null || true
        fi
        if [ -n "$USE_HERMES_ENV" ]; then
            export USE_HERMES="$USE_HERMES_ENV"
        else
            unset USE_HERMES 2>/dev/null || true
        fi

        if ! pod_cmd install --no-repo-update; then
            log_warn "pod install --no-repo-update failed, retrying with repo update..."
            pod_cmd install
        fi
        # Save current config to marker
        echo "$CURRENT_MARKER" > "$MARKER" 2>/dev/null || true
    else
        log_info "Pods are up to date"
    fi
}

# ============================================
# Check Dependencies
# ============================================
check_deps() {
    log_step "Checking dependencies..."

    local missing=0

    # Node.js
    if ! command -v node &> /dev/null; then
        log_error "Node.js not found"
        missing=1
    else
        log_success "Node.js: $(node --version)"
    fi

    # npm
    if ! command -v npm &> /dev/null; then
        log_error "npm not found"
        missing=1
    else
        log_success "npm: $(npm --version)"
    fi

    # Xcode
    if ! command -v xcodebuild &> /dev/null; then
        log_error "Xcode not found. Install with: xcode-select --install"
        missing=1
    else
        log_success "Xcode: $(xcodebuild -version | head -1)"
    fi

    # CocoaPods
    if ! command -v pod &> /dev/null; then
        log_warn "CocoaPods not found. Installing..."
        sudo gem install cocoapods
    else
        log_success "CocoaPods: $(pod --version)"
    fi

    if [ $missing -eq 1 ]; then
        log_error "Missing dependencies. Please install and retry."
        exit 1
    fi
}

# ============================================
# Install npm dependencies
# ============================================
install_npm() {
    log_step "Installing npm dependencies..."

    if [ ! -d "node_modules" ]; then
        npm install --legacy-peer-deps
    else
        log_info "node_modules exists, running npm install to ensure up-to-date..."
        npm install --legacy-peer-deps
    fi

    log_success "npm dependencies installed"
}

# ============================================
# Initialize macOS project (first time only)
# ============================================
init_macos_project() {
    # Args: main sandbox
    local MAIN="${1:-jsc}"
    local SANDBOX="${2:-jsc}"

    log_step "Initializing macOS native project..."

    # Check if we need to generate the native project
    if [ ! -f "macos/RillMacOSTest.xcworkspace/contents.xcworkspacedata" ]; then
        log_info "Generating macOS native project with react-native-macos-init..."

        # Use npx to run react-native-macos-init
        npx react-native-macos-init --version 0.79.1 --overwrite || {
            log_warn "react-native-macos-init failed, trying alternative method..."
            # Manual init if automated fails
            cd macos
            if [ ! -f "Podfile.lock" ]; then
                pod install
            fi
            cd ..
        }
    else
        log_info "macOS project already exists"
    fi

    # Ensure pods are installed
    cd macos
    pods_install "$MAIN" "$SANDBOX"
    cd ..

    log_success "macOS project initialized"
}

# ============================================
# Build the app
# ============================================
build_app() {
    log_step "Building macOS app..."

    cd macos

    # Detect CPU cores for parallel build
    local JOBS
    JOBS="$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"

    # Only show errors/warnings, hide compile progress
    # Optimizations:
    #   -jobs N           : parallel compilation using N cores
    #   ONLY_ACTIVE_ARCH  : only build arm64 (skip x86_64 on Apple Silicon)
    xcodebuild \
        -workspace RillMacOSTest.xcworkspace \
        -scheme RillMacOSTest-macOS \
        -configuration Debug \
        -derivedDataPath build \
        -jobs "$JOBS" \
        ONLY_ACTIVE_ARCH=YES \
        build \
        2>&1 | tee /tmp/rill-build.log | grep -E "^(Build |error:|warning:)" || true

    # Check build result
    if grep -q "BUILD SUCCEEDED" /tmp/rill-build.log; then
        log_success "Build completed"
    else
        log_error "Build failed. Check /tmp/rill-build.log for details"
        tail -50 /tmp/rill-build.log
        exit 1
    fi

    cd ..
}

# ============================================
# Start Metro bundler in background
# ============================================
start_metro() {
    log_step "Starting Metro bundler..."

    # Kill any existing Metro
    pkill -f "metro" 2>/dev/null || true

    # Some environments disallow binding to 0.0.0.0 (EPERM). Force localhost by default.
    local METRO_HOST="${RILL_E2E_METRO_HOST:-127.0.0.1}"
    local METRO_PORT="${RILL_E2E_METRO_PORT:-8081}"

    # Start Metro in background (silent)
    npm start -- --reset-cache --host "$METRO_HOST" --port "$METRO_PORT" > /tmp/rill-metro.log 2>&1 &
    METRO_PID=$!

    # Wait for Metro to be ready
    log_info "Waiting for Metro to start..."
    local WAIT=0
    while [ $WAIT -lt 30 ]; do
        if curl -s "http://${METRO_HOST}:${METRO_PORT}/status" >/dev/null 2>&1; then
            break
        fi
        if ! kill -0 $METRO_PID 2>/dev/null; then
            log_error "Metro failed to start. Check /tmp/rill-metro.log"
            tail -20 /tmp/rill-metro.log
            exit 1
        fi
        sleep 1
        ((WAIT++))
    done

    if [ $WAIT -ge 30 ]; then
        log_error "Metro timed out. Check /tmp/rill-metro.log"
        exit 1
    fi

    log_success "Metro started (PID: $METRO_PID)"
    echo $METRO_PID > /tmp/rill-metro.pid
}

# ============================================
# Run the app and capture test output
# ============================================
run_tests() {
    # Args: main sandbox
    local MAIN="${1:-jsc}"
    local SANDBOX="${2:-jsc}"

    log_step "Running tests (main=$MAIN sandbox=$SANDBOX)..."

    local APP_PATH="macos/build/Build/Products/Debug/RillMacOSTest.app"

    if [ ! -d "$APP_PATH" ]; then
        log_error "App not found at $APP_PATH"
        exit 1
    fi

    # Create output file
    local OUTPUT_FILE="/tmp/rill-test-output.log"
    rm -f "$OUTPUT_FILE"

    # Run the app and capture output (silent - only log to file)
    log_info "Launching app..."
    local START_TS
    START_TS="$(date +%s)"

    # Set environment variables for the app
    local APP_ENV=()
    APP_ENV+=("RILL_SANDBOX_TARGET=$SANDBOX")
    if ! use_metro; then
        APP_ENV+=("RILL_E2E_USE_BUNDLED_JS=1")
    fi

    env "${APP_ENV[@]}" "$APP_PATH/Contents/MacOS/RillMacOSTest" > "$OUTPUT_FILE" 2>&1 &
    APP_PID=$!

    # Wait for tests to complete (look for END marker)
    local TIMEOUT=60
    local ELAPSED=0
    while [ $ELAPSED -lt $TIMEOUT ]; do
        if grep -q ">>>RILL_TEST_END<<<" "$OUTPUT_FILE" 2>/dev/null; then
            break
        fi
        # If the app exited/crashed, fail fast (avoid waiting full timeout).
        if ! kill -0 $APP_PID 2>/dev/null; then
            log_error "App exited unexpectedly (PID: $APP_PID)"
            print_recent_crash_report "$START_TS"
            if [ -f "$OUTPUT_FILE" ]; then
                log_info "Last 120 lines of app output:"
                tail -120 "$OUTPUT_FILE" || true
            fi
            return 1
        fi
        sleep 0.5
        ELAPSED=$((ELAPSED + 1))
    done

    # Kill app immediately
    kill $APP_PID 2>/dev/null || true
    wait $APP_PID 2>/dev/null || true

    # Parse results
    if [ $ELAPSED -ge $TIMEOUT ]; then
        log_error "Tests timed out after ${TIMEOUT}s"
        print_recent_crash_report "$START_TS"
        if [ -f "$OUTPUT_FILE" ]; then
            log_info "Last 120 lines of app output:"
            tail -120 "$OUTPUT_FILE" || true
        fi
        return 1
    fi

    # Extract and display summary
    local SUMMARY
    SUMMARY=$(grep "^Summary:" "$OUTPUT_FILE" 2>/dev/null || true)
    if [ -n "$SUMMARY" ]; then
        log_info "$SUMMARY"
    fi

    # Check exit code from output
    if grep -q "EXIT_CODE:0" "$OUTPUT_FILE"; then
        log_success "All tests passed!"
        return 0
    else
        log_error "Some tests failed"
        # Show failed tests
        grep ">>>RILL_TEST_RESULT<<<" "$OUTPUT_FILE" | grep '"status":"failed"' | while read line; do
            echo "  $line"
        done
        return 1
    fi
}

# ============================================
# Cleanup
# ============================================
cleanup() {
    log_step "Cleaning up..."

    # Kill Metro if running
    if [ -f /tmp/rill-metro.pid ]; then
        kill $(cat /tmp/rill-metro.pid) 2>/dev/null || true
        rm -f /tmp/rill-metro.pid
    fi

    pkill -f "metro" 2>/dev/null || true

    log_success "Cleanup complete"
}

# ============================================
# Full test run (single spec)
# ============================================
run_single() {
    # Args: test_spec (e.g., "jsc-jsc", "hermes-quickjs")
    local SPEC="${1:-jsc-jsc}"
    read -r MAIN SANDBOX <<< "$(parse_test_spec "$SPEC")"

    trap cleanup EXIT

    check_deps
    install_npm
    init_macos_project "$MAIN" "$SANDBOX"
    build_app

    if use_metro; then
        start_metro
    else
        install_bundled_js_into_app
    fi

    run_tests "$MAIN" "$SANDBOX"
}

# ============================================
# Run all test combinations
# ============================================
run_all() {
    trap cleanup EXIT

    check_deps
    install_npm

    local FAILED=0

    # Group by main engine to minimize rebuilds
    # JSC main tests first, then Hermes main tests
    local JSC_SPECS=("jsc-jsc" "jsc-quickjs")
    local HERMES_SPECS=("hermes-jsc" "hermes-quickjs" "hermes-hermes")

    local TOTAL=$(( ${#JSC_SPECS[@]} + ${#HERMES_SPECS[@]} ))
    local CURRENT=0

    echo ""
    log_info "Running all ${TOTAL} test combinations"
    log_info "  JSC main: ${JSC_SPECS[*]}"
    log_info "  Hermes main: ${HERMES_SPECS[*]}"
    echo ""

    if ! use_metro; then
        log_info "Using pre-bundled JS (set RILL_E2E_USE_METRO=1 for dev server)"
    fi

    # Track results for summary
    local -a RESULTS_SPEC=()
    local -a RESULTS_STATUS=()

    # Run JSC main tests first, then Hermes main tests
    local PREV_CONFIG=""
    for SPEC in "${JSC_SPECS[@]}" "${HERMES_SPECS[@]}"; do
        ((CURRENT++))
        read -r MAIN SANDBOX <<< "$(parse_test_spec "$SPEC")"
        local CURRENT_CONFIG="${MAIN}:${SANDBOX}"

        log_step "[$CURRENT/$TOTAL] $SPEC (main=$MAIN sandbox=$SANDBOX)"

        # Check if config changed (needs pod install + rebuild)
        if [ "$PREV_CONFIG" != "$CURRENT_CONFIG" ]; then
            if [ "$MAIN" = "hermes" ] && [[ "$PREV_CONFIG" == jsc:* ]]; then
                log_warn "Switching to Hermes main (first-time may download ~1.4GB)"
            fi

            cd macos
            pods_install "$MAIN" "$SANDBOX"
            cd ..

            build_app
        fi

        if ! use_metro; then
            install_bundled_js_into_app
        elif [ "$CURRENT" = "1" ]; then
            start_metro
        fi

        RESULTS_SPEC+=("$SPEC")
        if run_tests "$MAIN" "$SANDBOX"; then
            RESULTS_STATUS+=("passed")
        else
            RESULTS_STATUS+=("failed")
            FAILED=1
        fi

        PREV_CONFIG="$CURRENT_CONFIG"
        echo ""
    done

    # Print summary
    echo ""
    echo "======================================"
    echo "  Test Results Summary"
    echo "======================================"
    echo ""

    local PASSED=0
    local i
    for i in "${!RESULTS_SPEC[@]}"; do
        local spec="${RESULTS_SPEC[$i]}"
        local status="${RESULTS_STATUS[$i]}"
        if [ "$status" = "passed" ]; then
            echo -e "  ${GREEN}✓${NC} $spec"
            ((PASSED++))
        else
            echo -e "  ${RED}✗${NC} $spec"
        fi
    done

    echo ""
    if [ "$FAILED" = "1" ]; then
        log_error "$PASSED/$TOTAL tests passed"
        return 1
    else
        log_success "$PASSED/$TOTAL tests passed"
        return 0
    fi
}

# ============================================
# Clean build
# ============================================
clean() {
    log_step "Cleaning..."
    rm -rf node_modules
    rm -rf macos/Pods
    rm -rf macos/build
    rm -rf macos/Podfile.lock
    rm -f /tmp/rill-metro.pid
    rm -f /tmp/rill-test-output.log
    log_success "Cleaned"
}

# ============================================
# Main
# ============================================
main() {
    echo ""
    echo "======================================"
    echo "  Rill macOS E2E Test Runner"
    echo "======================================"
    echo ""

    case "${1:-help}" in
        deps)
            check_deps
            ;;
        install)
            install_npm
            ;;
        init)
            local SPEC="${2:-jsc-jsc}"
            read -r MAIN SANDBOX <<< "$(parse_test_spec "$SPEC")"
            init_macos_project "$MAIN" "$SANDBOX"
            ;;
        build)
            build_app
            ;;
        metro)
            start_metro
            ;;
        run)
            local SPEC="${2:-jsc-jsc}"
            read -r MAIN SANDBOX <<< "$(parse_test_spec "$SPEC")"
            run_tests "$MAIN" "$SANDBOX"
            ;;
        # Single test specs (main-sandbox)
        jsc-jsc|jsc-quickjs|hermes-jsc|hermes-quickjs|hermes-hermes)
            run_single "$1"
            ;;
        # Run all combinations
        all)
            run_all
            ;;
        clean)
            clean
            ;;
        help|*)
            echo "Usage: $0 <command>"
            echo ""
            echo "Test commands (main-sandbox):"
            echo "  jsc-jsc         - JSC main + JSC sandbox"
            echo "  jsc-quickjs     - JSC main + QuickJS sandbox"
            echo "  hermes-jsc      - Hermes main + JSC sandbox"
            echo "  hermes-quickjs  - Hermes main + QuickJS sandbox"
            echo "  hermes-hermes   - Hermes main + Hermes sandbox"
            echo "  all             - Run all 5 combinations"
            echo ""
            echo "Utility commands:"
            echo "  deps    - Check dependencies"
            echo "  install - Install npm packages"
            echo "  init    - Initialize macOS native project"
            echo "  build   - Build the app"
            echo "  metro   - Start Metro bundler"
            echo "  clean   - Clean all build artifacts"
            echo ""
            echo "Examples:"
            echo "  $0 jsc-jsc        # JSC main + JSC sandbox"
            echo "  $0 hermes-hermes  # Hermes main + Hermes sandbox"
            echo "  $0 all            # Run all 5 combinations"
            echo ""
            echo "Environment variables:"
            echo "  RILL_E2E_USE_METRO=1  - Use Metro dev server instead of pre-bundled JS"
            exit 1
            ;;
    esac
}

main "$@"
