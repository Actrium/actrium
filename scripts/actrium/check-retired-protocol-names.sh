#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

primary_lower="$(printf '%s%s' 'as' 'kc')"
primary_title="$(printf '%s%s' 'As' 'kc')"
primary_upper="$(printf '%s%s' 'AS' 'KC')"
peer_lower="$(printf '%s%s' 'as' 'ks')"
peer_title="$(printf '%s%s' 'As' 'ks')"
peer_upper="$(printf '%s%s' 'AS' 'KS')"

failures="$(mktemp)"
trap 'rm -f "$failures"' EXIT

while IFS= read -r -d '' path; do
  normalized="/${path,,}/"
  if [[ "$normalized" =~ (^|[^a-z0-9])${primary_lower}([^a-z0-9]|$) ]] ||
     [[ "$normalized" =~ (^|[^a-z0-9])${peer_lower}([^a-z0-9]|$) ]]; then
    printf 'retired identifier in path: %s\n' "$path" >>"$failures"
  fi
done < <(git ls-files -co --exclude-standard -z)

git grep -nI \
  -e "$primary_lower" \
  -e "$primary_title" \
  -e "$primary_upper" \
  -- . >>"$failures" 2>/dev/null || true

git grep -nI -w \
  -e "$peer_lower" \
  -e "$peer_title" \
  -e "$peer_upper" \
  -- . >>"$failures" 2>/dev/null || true

if [[ -s "$failures" ]]; then
  echo "Retired package protocol identifiers are not allowed:" >&2
  cat "$failures" >&2
  exit 1
fi

echo "Retired package protocol identifier check passed."
