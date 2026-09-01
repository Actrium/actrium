#!/bin/bash
# sandbox-native development script
# Usage: ./dev.sh [command] [engine]
#   Commands: test, lint, fmt, build, clean, all
#   Engine: jsc, quickjs, or omit for both

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$SCRIPT_DIR"  # Native directory containing engine subdirectories

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Quiet mode for summarized output
QUIET_MODE=0

# Run command with optional quiet mode (capture and summarize output)
run_quiet() {
    local name=$1
    shift
    if [ "$QUIET_MODE" -eq 1 ]; then
        local output
        local start_time=$(date +%s)
        if output=$("$@" 2>&1); then
            local end_time=$(date +%s)
            local duration=$((end_time - start_time))
            log_success "$name (${duration}s)"
            return 0
        else
            local end_time=$(date +%s)
            local duration=$((end_time - start_time))
            log_error "$name FAILED (${duration}s)"
            # Show last few lines of output on failure
            echo "$output" | tail -10
            return 1
        fi
    else
        "$@"
    fi
}

# Check if clang-format is available
has_clang_format() {
    command -v clang-format &> /dev/null
}

# Check if clang-tidy is available
has_clang_tidy() {
    command -v clang-tidy &> /dev/null
}

# Build engine
do_build() {
    local engine=$1
    log_info "Building $engine..."
    cd "$ROOT_DIR/$engine"
    make -j$(sysctl -n hw.ncpu 2>/dev/null || echo 4)
    log_success "$engine built successfully"
}

# Run tests
do_test() {
    local engine=$1
    log_info "Testing $engine..."
    cd "$ROOT_DIR/$engine"
    make test
    log_success "$engine tests passed"
}

# Clean build
do_clean() {
    local engine=$1
    log_info "Cleaning $engine..."
    cd "$ROOT_DIR/$engine"
    make clean
    log_success "$engine cleaned"
}

# Format code
do_fmt() {
    local engine=$1
    if ! has_clang_format; then
        log_warn "clang-format not found, skipping format"
        return 0
    fi

    log_info "Formatting $engine..."
    cd "$ROOT_DIR/$engine"

    local files=$(find src test -name '*.cpp' -o -name '*.h' -o -name '*.mm' 2>/dev/null)
    if [ -n "$files" ]; then
        echo "$files" | xargs clang-format -i
        log_success "$engine formatted"
    else
        log_warn "No source files found in $engine"
    fi
}

# Lint code (compile with warnings as errors)
do_lint() {
    local engine=$1
    log_info "Linting $engine (compiling with strict warnings)..."
    cd "$ROOT_DIR/$engine"

    # Clean and rebuild with warnings
    make clean >/dev/null 2>&1
    local output
    output=$(make 2>&1)
    local warnings
    warnings=$(echo "$output" | grep -E "warning:|error:" | head -20)

    if [ -n "$warnings" ]; then
        log_error "$engine has warnings:"
        echo "$warnings"
        return 1  # Return non-zero to fail in quiet mode
    else
        log_success "$engine lint passed (no warnings)"
    fi
}

# Run with AddressSanitizer (memory error detection)
do_asan() {
    local engine=$1
    log_info "Running $engine with AddressSanitizer..."
    cd "$ROOT_DIR/$engine"

    # Clean first
    make clean >/dev/null 2>&1

    # Build with ASan flags via make variables
    local ASAN_FLAGS="-fsanitize=address,undefined -fno-omit-frame-pointer -g -O1"
    local ASAN_LDFLAGS="-fsanitize=address,undefined"

    local build_output
    if [ "$engine" = "quickjs" ]; then
        build_output=$(make EXTRA_CFLAGS="$ASAN_FLAGS" EXTRA_CXXFLAGS="$ASAN_FLAGS" EXTRA_LDFLAGS="$ASAN_LDFLAGS" 2>&1)
    else
        build_output=$(make EXTRA_CXXFLAGS="$ASAN_FLAGS" EXTRA_LDFLAGS="$ASAN_LDFLAGS" 2>&1)
    fi

    # Check for build errors
    if echo "$build_output" | grep -q "error:"; then
        log_error "ASan build failed"
        echo "$build_output" | grep -E "error:" | head -5
        return 1
    fi

    # Check if binary was built
    local test_binary=$(ls ./build/*_test 2>/dev/null | head -1)
    if [ -z "$test_binary" ]; then
        log_error "ASan build failed - no test binary"
        return 1
    fi

    # Run tests with ASan
    # Note: LeakSanitizer not supported on macOS, so detect_leaks=0
    log_info "Running tests with ASan enabled..."
    local test_output
    test_output=$(ASAN_OPTIONS="detect_leaks=0:abort_on_error=1:print_stats=0" "$test_binary" 2>&1)
    local test_exit=$?

    # Check for ASan errors (but ignore "detect_leaks is not supported" message)
    if echo "$test_output" | grep -qE "AddressSanitizer:.*(heap-buffer-overflow|stack-buffer-overflow|use-after-free|double-free)"; then
        log_error "$engine ASan detected memory issues:"
        echo "$test_output" | grep -E "(AddressSanitizer|SUMMARY)" | head -10
        return 1
    fi

    # Check test results
    if echo "$test_output" | grep -q "ALL TESTS PASSED"; then
        log_success "$engine ASan tests passed (no memory errors)"
    elif [ $test_exit -eq 0 ]; then
        log_success "$engine ASan tests completed"
    else
        log_error "$engine ASan tests failed"
        echo "$test_output" | tail -10
        return 1
    fi
}

# Run clang-tidy static analysis
do_tidy() {
    local engine=$1
    if ! has_clang_tidy; then
        log_warn "clang-tidy not found, skipping static analysis"
        log_info "Install with: brew install llvm"
        return 0
    fi

    log_info "Running clang-tidy on $engine..."
    cd "$ROOT_DIR/$engine"

    local files=$(find src -name '*.cpp' -o -name '*.mm' 2>/dev/null)
    if [ -z "$files" ]; then
        log_warn "No source files found in $engine"
        return 0
    fi

    local has_issues=0
    for file in $files; do
        log_info "Analyzing $file..."
        if [ "$engine" = "quickjs" ]; then
            clang-tidy "$file" -- -std=c++17 -I. -I.. -isystem vendor 2>/dev/null || has_issues=1
        else
            clang-tidy "$file" -- -std=c++17 -I. -I.. -x objective-c++ 2>/dev/null || has_issues=1
        fi
    done

    if [ $has_issues -eq 0 ]; then
        log_success "$engine clang-tidy passed"
    else
        log_warn "$engine has clang-tidy warnings"
    fi
}

# Run clang static analyzer
do_analyze() {
    local engine=$1
    log_info "Running Clang Static Analyzer on $engine..."
    cd "$ROOT_DIR/$engine"

    # Clean and analyze
    make clean

    if [ "$engine" = "quickjs" ]; then
        scan-build --use-cc=clang --use-c++=clang++ make 2>&1 | tail -20
    else
        scan-build --use-cc=clang --use-c++=clang++ make 2>&1 | tail -20
    fi

    log_success "$engine static analysis complete"
}

# Run command for specified engine(s)
run_for_engines() {
    local cmd=$1
    local engine=$2

    if [ -z "$engine" ] || [ "$engine" = "all" ]; then
        $cmd "quickjs"
        $cmd "jsc"
    else
        $cmd "$engine"
    fi
}

# Build with CMake (for cross-platform/Android)
do_cmake() {
    local engine=$1
    local build_type=${2:-Release}

    if [ "$engine" != "quickjs" ]; then
        log_warn "CMake build only supported for quickjs"
        return 0
    fi

    log_info "Building $engine with CMake ($build_type)..."
    cd "$ROOT_DIR/$engine"

    local build_dir="cmake-build-$(echo $build_type | tr '[:upper:]' '[:lower:]')"
    mkdir -p "$build_dir"
    cd "$build_dir"

    cmake .. -DCMAKE_BUILD_TYPE="$build_type" \
             -DQUICKJS_SANDBOX_BUILD_TESTS=ON \
             -DQUICKJS_SANDBOX_BUILD_STATIC=ON \
             -DQUICKJS_SANDBOX_BUILD_SHARED=ON

    cmake --build . --parallel

    log_success "$engine CMake build complete ($build_dir)"
}

# Print usage
usage() {
    echo "Usage: $0 <command> [engine]"
    echo ""
    echo "Commands:"
    echo "  build    - Build the project (Makefile)"
    echo "  cmake    - Build with CMake (for Android/cross-platform)"
    echo "  test     - Run tests"
    echo "  lint     - Check for warnings/errors"
    echo "  fmt      - Format source code (requires clang-format)"
    echo "  clean    - Clean build artifacts"
    echo "  asan     - Build and test with AddressSanitizer (memory errors)"
    echo "  tidy     - Run clang-tidy static analysis"
    echo "  analyze  - Run Clang Static Analyzer"
    echo "  all      - Run ALL checks with summary output"
    echo "  security - Run security checks (asan, tidy, analyze)"
    echo ""
    echo "Engine:"
    echo "  jsc     - JavaScriptCore sandbox only"
    echo "  quickjs - QuickJS sandbox only"
    echo "  (omit)  - Both engines"
    echo ""
    echo "Examples:"
    echo "  $0 test          # Test both engines"
    echo "  $0 test quickjs  # Test QuickJS only"
    echo "  $0 cmake quickjs # CMake build for QuickJS"
    echo "  $0 asan quickjs  # Run ASan on QuickJS"
    echo "  $0 security      # Run all security checks"
    echo "  $0 all           # Run everything"
}

# Main
main() {
    local cmd=${1:-help}
    local engine=$2

    case "$cmd" in
        build)
            run_for_engines do_build "$engine"
            ;;
        cmake)
            do_cmake "${engine:-quickjs}"
            ;;
        test)
            run_for_engines do_test "$engine"
            ;;
        lint)
            run_for_engines do_lint "$engine"
            ;;
        fmt)
            run_for_engines do_fmt "$engine"
            ;;
        clean)
            run_for_engines do_clean "$engine"
            ;;
        asan)
            run_for_engines do_asan "$engine"
            ;;
        tidy)
            run_for_engines do_tidy "$engine"
            ;;
        analyze)
            run_for_engines do_analyze "$engine"
            ;;
        security)
            log_info "Running all security checks..."
            run_for_engines do_asan "$engine"
            run_for_engines do_tidy "$engine"
            run_for_engines do_analyze "$engine"
            log_success "All security checks passed!"
            ;;
        all)
            QUIET_MODE=1
            echo ""
            log_info "Running ALL checks..."
            echo "────────────────────────────────────────"
            run_quiet "fmt:quickjs" do_fmt "quickjs"
            run_quiet "fmt:jsc" do_fmt "jsc"
            run_quiet "lint:quickjs" do_lint "quickjs"
            run_quiet "lint:jsc" do_lint "jsc"
            run_quiet "test:quickjs" do_test "quickjs"
            run_quiet "test:jsc" do_test "jsc"
            run_quiet "asan:quickjs" do_asan "quickjs"
            run_quiet "asan:jsc" do_asan "jsc"
            run_quiet "tidy:quickjs" do_tidy "quickjs"
            run_quiet "tidy:jsc" do_tidy "jsc"
            run_quiet "analyze:quickjs" do_analyze "quickjs"
            run_quiet "analyze:jsc" do_analyze "jsc"
            echo "────────────────────────────────────────"
            log_success "ALL checks passed!"
            ;;
        help|--help|-h)
            usage
            ;;
        *)
            log_error "Unknown command: $cmd"
            usage
            exit 1
            ;;
    esac
}

main "$@"
