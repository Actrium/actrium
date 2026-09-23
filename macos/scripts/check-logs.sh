#!/usr/bin/env bash
set -euo pipefail

fail=0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ACTRIUM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$ACTRIUM_DIR/.." && pwd)"
RUNTIME_DIR="${ACTRIUM_RUNTIME_PATH:-$REPO_ROOT/packages/actrium}"

echo "[log-check] Checking Swift print(...) usage..."
if rg -n "\bprint\s*\(" "$ACTRIUM_DIR/Actrium" --glob '!**/Pods/**' >/dev/null 2>&1; then
  echo "[log-check] ERROR: Found Swift print(...) usage in Actrium/"
  rg -n "\bprint\s*\(" "$ACTRIUM_DIR/Actrium" --glob '!**/Pods/**' || true
  fail=1
else
  echo "[log-check] OK: no Swift print(...)"
fi

echo "[log-check] Checking Actrium runtime console.* usage (via node script)..."
if [ -f "$RUNTIME_DIR/scripts/check-logs.js" ]; then
  ( cd "$RUNTIME_DIR" && node scripts/check-logs.js ) || fail=1
else
  echo "[log-check] SKIP: $RUNTIME_DIR/scripts/check-logs.js not found"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi

echo "[log-check] All OK"
