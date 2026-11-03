#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "${1:-}" = "--watch" ]; then
  cd "$SCRIPT_DIR/.."
  if [ -x /usr/local/bin/rust-target-env ]; then
    eval "$(/usr/local/bin/rust-target-env)"
  fi
  exec cargo watch -w crates -s 'bash crates/sw-host/build.sh'
fi
if [ "$#" -ne 0 ]; then
  echo "Usage: $0 [--watch]" >&2
  exit 2
fi
exec bash "$SCRIPT_DIR/../crates/sw-host/build.sh"
