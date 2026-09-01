#!/usr/bin/env bash
# Detects the highest-maturity pre-release channel label carried by merged
# PRs whose merge commits are part of a given commit list.
#
# Inputs:
#   $1  path to merged-PR JSON as produced by
#       `gh pr list --state merged --base main --json number,labels,mergeCommit`
#       ("-" reads the JSON from stdin)
#   $2  path to a newline-separated commit list (git rev-list output)
#
# Output: the winning channel name (rc, beta, or alpha) on stdout, or
# nothing when no labeled PR matches. Channel priority is fixed by the
# candidate order below: rc > beta > alpha.
set -euo pipefail

prs_json_file=${1:--}
commits_file=${2:?usage: detect-pre-release-label.sh <prs-json-file|-> <commits-file>}

for candidate in rc beta alpha; do
  found=$(jq -r \
    --arg label "release:${candidate}" \
    --rawfile commits "$commits_file" '
    .[]
    | select(any(.labels[]?; .name == $label))
    | select(.mergeCommit.oid as $oid | ($commits | split("\n") | index($oid)))
    | .number
  ' "$prs_json_file" | head -1)
  if [ -n "$found" ]; then
    printf '%s\n' "$candidate"
    exit 0
  fi
done
