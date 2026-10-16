#!/bin/bash
# Test Web URL loading in Actrium
# Usage: ./scripts/test-web.sh [url]
# Default URL: https://baidu.com

URL="${1:-https://baidu.com}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ACTRIUM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DERIVED_DATA=$(xcodebuild -workspace "$ACTRIUM_DIR/Actrium.xcworkspace" -scheme Actrium -showBuildSettings 2>/dev/null | grep -m1 'BUILT_PRODUCTS_DIR' | awk '{print $3}')
APP_PATH="${ACTRIUM_APP_PATH:-${DERIVED_DATA}/Actrium.app}"

echo "🧪 Testing Actrium with URL: $URL"
echo "📍 App path: $APP_PATH"

# Kill existing Actrium processes
pkill -f "Actrium.app" 2>/dev/null
sleep 1

# Launch directly (not via open) to ensure args are passed
echo "🚀 Launching Actrium..."
"$APP_PATH/Contents/MacOS/Actrium" --url "$URL" &

echo "✅ Actrium launched. Check the app and logs."
echo ""
echo "To view logs in real-time:"
echo "  log stream --predicate 'processImagePath CONTAINS \"Actrium\"' --level info"
