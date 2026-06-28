#!/usr/bin/env bash
#
# Bundle RCTNApps for production (without Metro dependency)
#
# Usage:
#   ./scripts/bundle-rctnapp.sh [options]
#
# Options:
#   --target <name>   Bundle target: "system", "actr", or "all" (default: all)
#   --dev             Build development bundle (with source maps, not minified)
#   --help            Show this help message
#
# Bundles:
#   system  -> Actro/Resources/main.rctnapp.jsbundle  (for RCTViewService0)
#   actr    -> Actro/Resources/actr.jsbundle          (for RCTViewService1-N)
#
# Examples:
#   ./scripts/bundle-rctnapp.sh                    # Build both bundles (release)
#   ./scripts/bundle-rctnapp.sh --target system    # Build system apps only
#   ./scripts/bundle-rctnapp.sh --target actr      # Build actr host only
#   ./scripts/bundle-rctnapp.sh --dev              # Build both in dev mode
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RCTNAPP_DIR="$PROJECT_ROOT/RCTNApps"
OUTPUT_DIR="$PROJECT_ROOT/Actro/Resources"

# Default options
TARGET="all"
DEV_MODE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --dev)
            DEV_MODE=true
            shift
            ;;
        --help|-h)
            head -25 "$0" | tail -22
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Validate target
if [[ ! "$TARGET" =~ ^(system|actr|all)$ ]]; then
    echo "❌ Invalid target: $TARGET"
    echo "   Valid targets: system, actr, all"
    exit 1
fi

echo "📦 Bundling RCTNApps..."
echo "   Source: $RCTNAPP_DIR"
echo "   Output: $OUTPUT_DIR"
echo "   Target: $TARGET"
echo "   Dev mode: $DEV_MODE"
echo ""

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

cd "$RCTNAPP_DIR"

# Function to bundle a target
bundle_target() {
    local name="$1"
    local entry_file="$2"
    local output_file="$3"

    echo "🔨 Building $name bundle..."
    echo "   Entry: $entry_file"
    echo "   Output: $output_file"

    if [ "$DEV_MODE" = true ]; then
        npx react-native bundle \
            --platform macos \
            --dev true \
            --entry-file "$entry_file" \
            --bundle-output "$output_file" \
            --assets-dest "$OUTPUT_DIR" \
            --sourcemap-output "$output_file.map"
        echo "   ✅ Dev bundle created (with source map)"
    else
        npx react-native bundle \
            --platform macos \
            --dev false \
            --entry-file "$entry_file" \
            --bundle-output "$output_file" \
            --assets-dest "$OUTPUT_DIR" \
            --minify true
        echo "   ✅ Production bundle created (minified)"
    fi

    # Show bundle size
    if [ -f "$output_file" ]; then
        local size=$(du -h "$output_file" | cut -f1)
        echo "   Size: $size"
    fi
    echo ""
}

# Bundle based on target
if [ "$TARGET" = "system" ] || [ "$TARGET" = "all" ]; then
    bundle_target "System Apps" "index-system.tsx" "$OUTPUT_DIR/main.rctnapp.jsbundle"
fi

if [ "$TARGET" = "actr" ] || [ "$TARGET" = "all" ]; then
    bundle_target "Actr Host" "index-actr.tsx" "$OUTPUT_DIR/actr.jsbundle"
fi

echo "🎉 Done!"
echo ""
echo "Bundle locations:"
if [ "$TARGET" = "system" ] || [ "$TARGET" = "all" ]; then
    echo "   System Apps: $OUTPUT_DIR/main.rctnapp.jsbundle"
fi
if [ "$TARGET" = "actr" ] || [ "$TARGET" = "all" ]; then
    echo "   Actr Host:   $OUTPUT_DIR/actr.jsbundle"
fi
echo ""
echo "Note: Make sure bundle files are added to Xcode project resources."
