#!/usr/bin/env bash
set -euo pipefail
scenario_dir="$(cd "$(dirname "$0")" && pwd)"
for realm_mode in provider external; do
    MODE="${MODE:-unpublished}" REALM_MODE="$realm_mode" bash "$scenario_dir/run.sh" "$@"
done
