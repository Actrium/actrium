#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/actrium/check-large-files.sh [TREEISH|--staged]

Reject tracked blobs larger than 1,000,000 bytes unless they are present in
.github/actrium-large-files-allowlist.txt and remain within their reviewed budget.
EOF
}

case "${1:-HEAD}" in
  -h|--help)
    usage
    exit 0
    ;;
  --staged)
    treeish="$(git write-tree)"
    ;;
  *)
    treeish="${1:-HEAD}"
    ;;
esac

repo_root="$(git rev-parse --show-toplevel)"
policy_file="${repo_root}/.github/actrium-large-files-allowlist.txt"
limit_bytes="${ACTRIUM_LARGE_FILE_LIMIT_BYTES:-1000000}"

cd "${repo_root}"

if [[ ! -f "${policy_file}" ]]; then
  echo "large-file policy is missing: ${policy_file}" >&2
  exit 1
fi

if ! [[ "${limit_bytes}" =~ ^[0-9]+$ ]]; then
  echo "ACTRIUM_LARGE_FILE_LIMIT_BYTES must be a non-negative integer" >&2
  exit 1
fi

git cat-file -e "${treeish}^{tree}"

duplicate_paths="$(
  awk -F '	' '$1 !~ /^#/ && NF >= 2 { print $2 }' "${policy_file}" |
    LC_ALL=C sort |
    uniq -d
)"
if [[ -n "${duplicate_paths}" ]]; then
  echo "duplicate paths in ${policy_file}:" >&2
  echo "${duplicate_paths}" >&2
  exit 1
fi

errors=0
large_count=0
allowed_count=0

while IFS= read -r -d '' entry; do
  metadata="${entry%%$'	'*}"
  path="${entry#*$'	'}"
  read -r _mode object_type _object_id size_bytes <<< "${metadata}"

  if [[ "${object_type}" != "blob" ]]; then
    continue
  fi

  if (( size_bytes <= limit_bytes )); then
    continue
  fi

  large_count=$((large_count + 1))
  max_bytes="$(
    awk -F '	' -v target="${path}" \
      '$1 !~ /^#/ && $2 == target { print $1 }' \
      "${policy_file}"
  )"

  if [[ -z "${max_bytes}" ]]; then
    echo "unreviewed large file: ${path} (${size_bytes} bytes)" >&2
    errors=$((errors + 1))
    continue
  fi

  if ! [[ "${max_bytes}" =~ ^[0-9]+$ ]]; then
    echo "invalid size budget for ${path}: ${max_bytes}" >&2
    errors=$((errors + 1))
    continue
  fi

  if (( size_bytes > max_bytes )); then
    echo "large file exceeds reviewed budget: ${path} (${size_bytes} > ${max_bytes} bytes)" >&2
    errors=$((errors + 1))
    continue
  fi

  allowed_count=$((allowed_count + 1))
done < <(git ls-tree -r -l -z "${treeish}")

while IFS=$'	' read -r max_bytes path; do
  if [[ -z "${max_bytes}" || "${max_bytes}" == \#* ]]; then
    continue
  fi

  if [[ -z "${path:-}" || ! "${max_bytes}" =~ ^[0-9]+$ ]]; then
    echo "invalid allowlist entry: ${max_bytes}${path:+	${path}}" >&2
    errors=$((errors + 1))
    continue
  fi

  if ! size_bytes="$(git cat-file -s "${treeish}:${path}" 2>/dev/null)"; then
    echo "stale allowlist path (not tracked in ${treeish}): ${path}" >&2
    errors=$((errors + 1))
    continue
  fi

  if (( size_bytes <= limit_bytes )); then
    echo "stale allowlist path (no longer over ${limit_bytes} bytes): ${path}" >&2
    errors=$((errors + 1))
  fi
done < "${policy_file}"

if (( errors > 0 )); then
  echo "large-file policy failed with ${errors} error(s)" >&2
  exit 1
fi

echo "large-file policy passed: ${allowed_count}/${large_count} large tracked file(s) reviewed"
