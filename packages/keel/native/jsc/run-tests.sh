#!/bin/bash
# JSC Sandbox Native Tests - One-shot runner
# Builds and runs all native tests, returns non-zero on failure

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

echo "======================================"
echo "  JSC Sandbox Native Tests"
echo "======================================"
echo ""

# Check platform
if [[ "$(uname)" != "Darwin" ]]; then
  echo "[SKIP] JSC tests only run on macOS"
  exit 0
fi

# Clean and build
echo "[1/2] Building..."
make clean > /dev/null 2>&1 || true
make -j$(sysctl -n hw.ncpu)

# Run tests
echo ""
echo "[2/2] Running tests..."
make test

echo ""
echo "[OK] All JSC native tests passed"
