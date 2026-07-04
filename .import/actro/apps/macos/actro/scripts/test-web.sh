#!/bin/bash
# Test Web URL loading in Actro
# Usage: ./scripts/test-web.sh [url]
# Default URL: https://baidu.com

URL="${1:-https://baidu.com}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ACTRO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DERIVED_DATA=$(xcodebuild -workspace "$ACTRO_DIR/Actro.xcworkspace" -scheme Actro -showBuildSettings 2>/dev/null | grep -m1 'BUILT_PRODUCTS_DIR' | awk '{print $3}')
APP_PATH="${ACTRO_APP_PATH:-${DERIVED_DATA}/Actro.app}"

echo "🧪 Testing Actro with URL: $URL"
echo "📍 App path: $APP_PATH"

# Kill existing Actro processes
pkill -f "Actro.app" 2>/dev/null
sleep 1

# Launch directly (not via open) to ensure args are passed
echo "🚀 Launching Actro..."
"$APP_PATH/Contents/MacOS/Actro" --url "$URL" &

echo "✅ Actro launched. Check the app and logs."
echo ""
echo "To view logs in real-time:"
echo "  log stream --predicate 'processImagePath CONTAINS \"Actro\"' --level info"
