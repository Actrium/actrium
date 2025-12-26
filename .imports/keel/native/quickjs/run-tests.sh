#!/bin/bash
# QuickJS Sandbox Native Tests - One-shot runner
# Builds and runs all native tests, returns non-zero on failure

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

echo "======================================"
echo "  QuickJS Sandbox Native Tests"
echo "======================================"
echo ""

# Clean and build
echo "[1/2] Building..."
make clean > /dev/null 2>&1 || true
make -j$(sysctl -n hw.ncpu 2>/dev/null || echo 4)

# Run tests
echo ""
echo "[2/2] Running tests..."
make test

echo ""
echo "[OK] All QuickJS native tests passed"
