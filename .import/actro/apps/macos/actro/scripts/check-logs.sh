#!/usr/bin/env bash
set -euo pipefail

fail=0

echo "[log-check] Checking Swift print(...) usage..."
if rg -n "\bprint\s*\(" Actro --glob '!**/Pods/**' >/dev/null 2>&1; then
  echo "[log-check] ERROR: Found Swift print(...) usage in Actro/"
  rg -n "\bprint\s*\(" Actro --glob '!**/Pods/**' || true
  fail=1
else
  echo "[log-check] OK: no Swift print(...)"
fi

echo "[log-check] Checking RCTNApps console.* usage (via node script)..."
( cd RCTNApps && node scripts/check-logs.js ) || fail=1

if [ "$fail" -ne 0 ]; then
  exit 1
fi

echo "[log-check] All OK"
