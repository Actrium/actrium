#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "Usage: scripts/ci/run-claude-from-template.sh <template-path>" >&2
  exit 1
fi

template_path="$1"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Resolve dependencies relative to this script, independent of the caller's cwd.
node "$script_dir/render-prompt.js" "$template_path" \
  | claude --print --verbose --output-format=stream-json --include-partial-messages \
    --dangerously-skip-permissions 2>&1 \
  | stdbuf -oL -eL node "$script_dir/claude-stream-summary.js"
