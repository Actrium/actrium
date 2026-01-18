#!/bin/bash
# Rill - Master Test Runner
# Runs all tests: unit, native, and E2E
# Returns non-zero if any test suite fails

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m'

FAILED_SUITES=()
SKIPPED_SUITES=()
PASSED_SUITES=()

run_suite() {
  local name="$1"
  local cmd="$2"

  echo ""
  echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "${BLUE}  Running: ${name}${NC}"
  echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo ""

  if eval "$cmd"; then
    PASSED_SUITES+=("$name")
    echo -e "${GREEN}[PASS]${NC} ${name}"
  else
    FAILED_SUITES+=("$name")
    echo -e "${RED}[FAIL]${NC} ${name}"
  fi
}

skip_suite() {
  local name="$1"
  local reason="$2"
  SKIPPED_SUITES+=("$name ($reason)")
  echo -e "${YELLOW}[SKIP]${NC} ${name} - ${reason}"
}

echo "======================================"
echo "  Rill - Master Test Runner"
echo "======================================"
echo ""
echo "Platform: $(uname -s)"
echo "Date: $(date)"

# Parse arguments
RUN_NATIVE=true
RUN_E2E=true
RUN_UNIT=true
RUN_RN=false  # RN macOS E2E is opt-in (requires Xcode build)

while [[ $# -gt 0 ]]; do
  case $1 in
    --skip-native) RUN_NATIVE=false; shift ;;
    --skip-e2e) RUN_E2E=false; shift ;;
    --skip-unit) RUN_UNIT=false; shift ;;
    --with-rn) RUN_RN=true; shift ;;
    --help)
      echo ""
      echo "Usage: $0 [options]"
      echo ""
      echo "Options:"
      echo "  --skip-native    Skip native C++/ObjC tests"
      echo "  --skip-e2e       Skip browser E2E tests"
      echo "  --skip-unit      Skip bun unit tests"
      echo "  --with-rn        Include React Native macOS E2E tests (requires Xcode)"
      echo "  --help           Show this help"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ============================================
# 1. Unit Tests (bun test)
# ============================================
if [[ "$RUN_UNIT" == true ]]; then
  run_suite "Unit Tests (bun test)" "bun test"
fi

# ============================================
# 2. Native Tests (QuickJS + JSC)
# ============================================
if [[ "$RUN_NATIVE" == true ]]; then
  # QuickJS (cross-platform)
  if [[ -f "$ROOT_DIR/native/quickjs/run-tests.sh" ]]; then
    run_suite "Native: QuickJS Sandbox" "bash $ROOT_DIR/native/quickjs/run-tests.sh"
  fi

  # JSC (macOS only)
  if [[ "$(uname)" == "Darwin" ]]; then
    if [[ -f "$ROOT_DIR/native/jsc/run-tests.sh" ]]; then
      run_suite "Native: JSC Sandbox" "bash $ROOT_DIR/native/jsc/run-tests.sh"
    fi
  else
    skip_suite "Native: JSC Sandbox" "macOS only"
  fi
fi

# ============================================
# 3. Browser E2E Tests (Playwright)
# ============================================
if [[ "$RUN_E2E" == true ]]; then
  # WASM E2E (Web)
  if [[ -f "$ROOT_DIR/tests/e2e-wasm-sandbox/run-tests.sh" ]]; then
    run_suite "E2E: Web (WASM Sandbox)" "bash $ROOT_DIR/tests/e2e-wasm-sandbox/run-tests.sh"
  fi
fi

# ============================================
# 4. React Native E2E (opt-in)
# ============================================
if [[ "$RUN_RN" == true ]]; then
  if [[ "$(uname)" == "Darwin" ]]; then
    if [[ -f "$ROOT_DIR/tests/rn-macos-e2e/run-tests.sh" ]]; then
      run_suite "E2E: React Native macOS" "bash $ROOT_DIR/tests/rn-macos-e2e/run-tests.sh all"
    fi
  else
    skip_suite "E2E: React Native macOS" "macOS only"
  fi
fi

# ============================================
# Summary
# ============================================
echo ""
echo "======================================"
echo "  Test Summary"
echo "======================================"
echo ""

if [[ ${#PASSED_SUITES[@]} -gt 0 ]]; then
  echo -e "${GREEN}Passed (${#PASSED_SUITES[@]}):${NC}"
  for suite in "${PASSED_SUITES[@]}"; do
    echo "  - $suite"
  done
fi

if [[ ${#SKIPPED_SUITES[@]} -gt 0 ]]; then
  echo ""
  echo -e "${YELLOW}Skipped (${#SKIPPED_SUITES[@]}):${NC}"
  for suite in "${SKIPPED_SUITES[@]}"; do
    echo "  - $suite"
  done
fi

if [[ ${#FAILED_SUITES[@]} -gt 0 ]]; then
  echo ""
  echo -e "${RED}Failed (${#FAILED_SUITES[@]}):${NC}"
  for suite in "${FAILED_SUITES[@]}"; do
    echo "  - $suite"
  done
  echo ""
  exit 1
fi

echo ""
echo -e "${GREEN}All tests passed!${NC}"
exit 0
