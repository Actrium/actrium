#!/bin/sh
# Extract the pinned agent directly as runner; no later ownership layer is needed.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/versions/agent.env"
. "$script_dir/runner-install-lib.sh"
archive=$(mktemp)
trap 'rm -f "$archive"' EXIT
download_artifact \
    "$(github_url "https://github.com/actions/runner/releases/download/v$RUNNER_VERSION/actions-runner-linux-x64-$RUNNER_VERSION.tar.gz")" \
    "$RUNNER_SHA256" "$archive"
mkdir -p "$HOME/actions-runner"
tar -xzf "$archive" -C "$HOME/actions-runner"
python3 "$script_dir/patch-runner-dll.py"
