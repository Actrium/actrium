#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "Usage: scripts/ci/commit-and-push-if-needed.sh <commit-message> <ref>" >&2
  exit 1
fi

commit_message="$1"
target_ref="$2"

git config user.name "Claude Code"
git config user.email "claude@github.com"
git add -A

if git diff --cached --quiet; then
  exit 0
fi

git commit -m "$commit_message"
git push origin "$target_ref"
