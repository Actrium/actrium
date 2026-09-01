#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

primary_lower="$(printf '%s%s' 'as' 'kc')"
peer_lower="$(printf '%s%s' 'as' 'ks')"
token_pattern="(^|[^A-Za-z0-9_])(${primary_lower}|${peer_lower})([^A-Za-z0-9_]|$)"

failures="$(mktemp)"
trap 'rm -f "$failures"' EXIT

while IFS= read -r -d '' path; do
  normalized="$(printf '/%s/' "$path" | LC_ALL=C tr '[:upper:]' '[:lower:]')"
  if [[ "$normalized" =~ (^|[^a-z0-9])${primary_lower}([^a-z0-9]|$) ]] ||
     [[ "$normalized" =~ (^|[^a-z0-9])${peer_lower}([^a-z0-9]|$) ]]; then
    printf 'retired identifier in path: %s\n' "$path" >>"$failures"
  fi
done < <(git ls-files -co --exclude-standard -z)

git grep -niI -w \
  -e "$primary_lower" \
  -e "$peer_lower" \
  -- . >>"$failures" 2>/dev/null || true

if [[ "$(git rev-parse --is-shallow-repository)" == "true" ]]; then
  printf 'full history is required for the retired protocol audit\n' >>"$failures"
else
  if git rev-list --objects --all \
    | cut -d' ' -f2- \
    | LC_ALL=C grep -aEi "$token_pattern" >/dev/null; then
    printf 'retired identifier found in a retained history path\n' >>"$failures"
  fi

  if git log --all --format=%B \
    | LC_ALL=C grep -aEi "$token_pattern" >/dev/null; then
    printf 'retired identifier found in a retained commit message\n' >>"$failures"
  fi

  if git cat-file --batch-all-objects --batch \
    | LC_ALL=C grep -aEi "$token_pattern" >/dev/null; then
    printf 'retired identifier found in the Git object database\n' >>"$failures"
  fi
fi

if [[ -s "$failures" ]]; then
  echo "Retired package protocol identifiers are not allowed:" >&2
  cat "$failures" >&2
  exit 1
fi

echo "Retired package protocol identifier check passed."
