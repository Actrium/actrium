#!/usr/bin/env bash
#
# Build the counterapp Guest, pack and verify the .actr package, then open it
# in Actrium. The host app is reused unless --rebuild-app is passed.
#
# Usage:
#   ./run-macos.sh [--no-open] [--rebuild-app]
#
# Options:
#   --no-open       Build and verify without launching Actrium.
#   --rebuild-app   Rebuild the Actrium host app before opening the package.
#   --help          Show this help.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
APP_PATH="$REPO_ROOT/target/xcode-derived-data/actrium-counter/Build/Products/Debug/Actrium.app"
PACKAGE_PATH="$SCRIPT_DIR/dist/actrium-counter-1.0.0.actr"
REBUILD_APP=0
OPEN_APP=1

usage() {
    sed -n '/^# Usage:/,/^$/p' "$0" \
        | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-open)
            OPEN_APP=0
            shift
            ;;
        --rebuild-app)
            REBUILD_APP=1
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

for command in bun cargo; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "Required command is missing: $command" >&2
        exit 1
    fi
done

cd "$SCRIPT_DIR"
bun run build
bun run build:package
bun run verify:package

if [ "$REBUILD_APP" = "1" ] || [ ! -d "$APP_PATH" ]; then
    for command in pod xcodebuild; do
        if ! command -v "$command" >/dev/null 2>&1; then
            echo "Required command is missing: $command" >&2
            exit 1
        fi
    done

    cd "$REPO_ROOT/macos"
    if [ ! -d Pods ]; then
        pod install
    fi
    ./scripts/bundle-actrium-runtime.sh --target system --dev
    xcodebuild \
        -workspace Actrium.xcworkspace \
        -scheme Actrium \
        -configuration Debug \
        -destination "platform=macOS,arch=$(uname -m)" \
        -derivedDataPath "$REPO_ROOT/target/xcode-derived-data/actrium-counter" \
        CODE_SIGNING_ALLOWED=NO \
        build
fi

if [ ! -d "$APP_PATH" ]; then
    echo "Actrium build did not produce $APP_PATH" >&2
    exit 1
fi

if [ "$OPEN_APP" = "0" ]; then
    echo "Counter Guest verified without launching the UI."
    exit 0
fi

open -n "$APP_PATH" --args --url "actro://$PACKAGE_PATH"
echo "Counter Guest opened in Actrium."
