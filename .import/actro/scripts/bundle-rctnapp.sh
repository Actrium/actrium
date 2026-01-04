#!/usr/bin/env bash
#
# Bundle RCTNApps for production (without Metro dependency)
#
# Usage:
#   ./scripts/bundle-rctnapp.sh [--dev]
#
# Output:
#   Canvas/Resources/main.rctnapp.jsbundle
#   Canvas/Resources/assets/ (images, fonts, etc.)
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RCTNAPP_DIR="$PROJECT_ROOT/RCTNApps"
OUTPUT_DIR="$PROJECT_ROOT/Canvas/Resources"

# Parse arguments
DEV_MODE=false
for arg in "$@"; do
    case $arg in
        --dev)
            DEV_MODE=true
            shift
            ;;
    esac
done

echo "📦 Bundling RCTNApps..."
echo "   Source: $RCTNAPP_DIR"
echo "   Output: $OUTPUT_DIR"
echo "   Dev mode: $DEV_MODE"

# Check if RCTNApps directory exists
if [ ! -d "$RCTNAPP_DIR" ]; then
    echo "❌ Error: RCTNApps directory not found at $RCTNAPP_DIR"
    exit 1
fi

# Check if node_modules exists
if [ ! -d "$RCTNAPP_DIR/node_modules" ]; then
    echo "⚠️  node_modules not found, running npm install..."
    (cd "$RCTNAPP_DIR" && npm install)
fi

# Create output directory if needed
mkdir -p "$OUTPUT_DIR"

# Bundle for macOS
echo "🔨 Running react-native bundle..."

BUNDLE_OUTPUT="$OUTPUT_DIR/main.rctnapp.jsbundle"
ASSETS_DEST="$OUTPUT_DIR"

cd "$RCTNAPP_DIR"

if [ "$DEV_MODE" = true ]; then
    npx react-native bundle \
        --platform macos \
        --dev true \
        --entry-file index.ts \
        --bundle-output "$BUNDLE_OUTPUT" \
        --assets-dest "$ASSETS_DEST" \
        --sourcemap-output "$BUNDLE_OUTPUT.map"
    echo "   ✅ Dev bundle created (with source map)"
else
    npx react-native bundle \
        --platform macos \
        --dev false \
        --entry-file index.ts \
        --bundle-output "$BUNDLE_OUTPUT" \
        --assets-dest "$ASSETS_DEST" \
        --minify true
    echo "   ✅ Production bundle created (minified)"
fi

# Show bundle size
if [ -f "$BUNDLE_OUTPUT" ]; then
    BUNDLE_SIZE=$(du -h "$BUNDLE_OUTPUT" | cut -f1)
    echo ""
    echo "📊 Bundle stats:"
    echo "   File: $BUNDLE_OUTPUT"
    echo "   Size: $BUNDLE_SIZE"
fi

echo ""
echo "🎉 Done! Bundle ready at: $BUNDLE_OUTPUT"
echo ""
echo "Note: Make sure main.rctnapp.jsbundle is added to Xcode project resources."
