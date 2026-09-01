#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
RUNTIME_RELATIVE_TARGET="../../../packages/actro-runtime"
RUNTIME_TARGET="$PROJECT_ROOT/$RUNTIME_RELATIVE_TARGET"
RUNTIME_LINK="$PROJECT_ROOT/ActroRuntime"

if [ ! -d "$RUNTIME_TARGET" ]; then
    echo "ERROR: Actro runtime package not found: $RUNTIME_TARGET"
    exit 1
fi

if [ -L "$RUNTIME_LINK" ] || [ ! -e "$RUNTIME_LINK" ]; then
    ln -sfn "$RUNTIME_RELATIVE_TARGET" "$RUNTIME_LINK"
elif [ ! -d "$RUNTIME_LINK" ]; then
    echo "ERROR: ActroRuntime exists but is not a directory or symlink: $RUNTIME_LINK"
    exit 1
fi
