#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "Usage: scripts/ci/run-claude-from-template.sh <template-path>" >&2
  exit 1
fi

template_path="$1"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 基于脚本自身目录定位依赖，避免调用方工作目录变化后找不到配套脚本。
node "$script_dir/render-prompt.js" "$template_path" \
  | claude --print --verbose --output-format=stream-json --include-partial-messages \
    --dangerously-skip-permissions 2>&1 \
  | stdbuf -oL -eL node "$script_dir/claude-stream-summary.js"
