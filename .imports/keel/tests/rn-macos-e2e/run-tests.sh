#!/bin/bash
# Rill macOS E2E Test Runner
# Full automation: deps -> init -> build -> run -> parse results

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RILL_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$SCRIPT_DIR"

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
    if [ ! -d "Pods" ] || [ "Podfile" -nt "Podfile.lock" ]; then
        log_info "Installing CocoaPods..."
        pod install
    else
        log_info "Pods are up to date"
    fi
    cd ..

    log_success "macOS project initialized"
}

# ============================================
# Build the app
# ============================================
build_app() {
    log_step "Building macOS app..."

    cd macos

    xcodebuild \
        -workspace RillMacOSTest.xcworkspace \
        -scheme RillMacOSTest-macOS \
        -configuration Debug \
        -derivedDataPath build \
        build \
        2>&1 | tee /tmp/rill-build.log | grep -E "^(Build|Compile|Link|error:|warning:)" || true

    # Check build result
    if grep -q "BUILD SUCCEEDED" /tmp/rill-build.log; then
        log_success "Build completed"
    else
        log_error "Build failed. Check /tmp/rill-build.log for details"
        tail -20 /tmp/rill-build.log
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

    # Start Metro in background
    npm start -- --reset-cache &
    METRO_PID=$!

    # Wait for Metro to be ready
    log_info "Waiting for Metro to start..."
    sleep 5

    # Check if Metro is running
    if ! kill -0 $METRO_PID 2>/dev/null; then
        log_error "Metro failed to start"
        exit 1
    fi

    log_success "Metro started (PID: $METRO_PID)"
    echo $METRO_PID > /tmp/rill-metro.pid
}

# ============================================
# Run the app and capture test output
# ============================================
run_tests() {
    log_step "Running tests..."

    local APP_PATH="macos/build/Build/Products/Debug/RillMacOSTest.app"

    if [ ! -d "$APP_PATH" ]; then
        log_error "App not found at $APP_PATH"
        exit 1
    fi

    # Create output file
    local OUTPUT_FILE="/tmp/rill-test-output.log"
    rm -f "$OUTPUT_FILE"

    # Run the app and capture output
    log_info "Launching app..."
    "$APP_PATH/Contents/MacOS/RillMacOSTest" 2>&1 | tee "$OUTPUT_FILE" &
    APP_PID=$!

    # Wait for tests to complete (look for END marker)
    local TIMEOUT=60
    local ELAPSED=0
    while [ $ELAPSED -lt $TIMEOUT ]; do
        if grep -q ">>>RILL_TEST_END<<<" "$OUTPUT_FILE" 2>/dev/null; then
            break
        fi
        sleep 1
        ((ELAPSED++))
    done

    # Kill app
    kill $APP_PID 2>/dev/null || true

    # Parse results
    if [ $ELAPSED -ge $TIMEOUT ]; then
        log_error "Tests timed out after ${TIMEOUT}s"
        return 1
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
# Full test run
# ============================================
run_all() {
    trap cleanup EXIT

    check_deps
    install_npm
    init_macos_project
    build_app
    start_metro
    run_tests
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

    case "${1:-all}" in
        deps)
            check_deps
            ;;
        install)
            install_npm
            ;;
        init)
            init_macos_project
            ;;
        build)
            build_app
            ;;
        metro)
            start_metro
            ;;
        run)
            run_tests
            ;;
        all)
            run_all
            ;;
        clean)
            clean
            ;;
        *)
            echo "Usage: $0 {deps|install|init|build|metro|run|all|clean}"
            echo ""
            echo "  deps    - Check dependencies"
            echo "  install - Install npm packages"
            echo "  init    - Initialize macOS native project"
            echo "  build   - Build the app"
            echo "  metro   - Start Metro bundler"
            echo "  run     - Run tests"
            echo "  all     - Full pipeline (default)"
            echo "  clean   - Clean all build artifacts"
            exit 1
            ;;
    esac
}

main "$@"
