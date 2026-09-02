#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

ts_contract="packages/actrium/src/runtime-host/generated/actriumCoreContract.ts"
android_contract="android/android/app/src/main/java/io/actrium/app/generated/ActriumAndroidPersistenceContract.kt"

failures=0

record_failure() {
  failures=$((failures + 1))
}

print_ok() {
  echo "[ok] $1"
}

print_failure() {
  echo
  echo "[fail] $1"
  record_failure
}

for generated_file in "$ts_contract" "$android_contract"; do
  if [[ ! -f "$generated_file" ]]; then
    print_failure "missing generated contract: $generated_file"
  else
    print_ok "generated contract exists: $generated_file"
  fi
done

direct_ts_imports="$(
  rg -n "generated/actriumCoreContract" packages/actrium/src \
    --glob '!**/runtime-host/generated/**' \
    --glob '!**/runtime-host/coreContract.ts' || true
)"

if [[ -n "$direct_ts_imports" ]]; then
  print_failure "runtime source imports the generated TypeScript contract directly"
  printf '%s\n' "$direct_ts_imports"
else
  print_ok "runtime source imports the TypeScript contract through coreContract"
fi

if [[ "$failures" -ne 0 ]]; then
  echo
  echo "generated contract check failed with $failures issue(s)"
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

tmp_ts_contract="$tmp_dir/actriumCoreContract.ts"
tmp_android_contract="$tmp_dir/ActriumAndroidPersistenceContract.kt"

cargo run --quiet -p actrium-core --bin generate-ts-contracts -- "$tmp_ts_contract"
cargo run --quiet -p actrium-core --bin generate-android-contracts -- "$tmp_android_contract"

compare_generated_contract() {
  local expected_file=$1
  local actual_file=$2
  local description=$3

  if cmp -s "$expected_file" "$actual_file"; then
    print_ok "$description is up to date"
    return
  fi

  print_failure "$description is out of date"
  diff -u "$expected_file" "$actual_file" || true
}

compare_generated_contract "$tmp_ts_contract" "$ts_contract" "TypeScript runtime contract"
compare_generated_contract "$tmp_android_contract" "$android_contract" "Android persistence contract"

if [[ "$failures" -ne 0 ]]; then
  echo
  echo "generated contract check failed with $failures issue(s)"
  exit 1
fi

echo
echo "generated contracts are current"
