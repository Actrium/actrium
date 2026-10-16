#!/bin/bash
# Automated test script for Overview mode fix
# Runs without user interaction

set -e

echo "=== Automated Overview Mode Test ==="
echo ""

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DERIVED_DATA=$(xcodebuild -workspace "$SCRIPT_DIR/Actrium.xcworkspace" -scheme Actrium -showBuildSettings 2>/dev/null | grep -m1 'BUILT_PRODUCTS_DIR' | awk '{print $3}')
APP_PATH="${ACTRIUM_APP_PATH:-${DERIVED_DATA}/Actrium.app}"
LOG_FILE="/tmp/actrium_auto_test_$(date +%Y%m%d_%H%M%S).log"
SCREENSHOT_FOCUS="/tmp/actrium_auto_focus_$(date +%Y%m%d_%H%M%S).png"
SCREENSHOT_OVERVIEW="/tmp/actrium_auto_overview_$(date +%Y%m%d_%H%M%S).png"

# Step 1: Kill existing Actrium processes
echo "[1/6] Stopping existing Actrium processes..."
pkill -f "Actrium.app" 2>/dev/null || true
pkill -f "ActriumViewService" 2>/dev/null || true
sleep 2

# Step 2: Start log capture in background
echo "[2/6] Starting log capture..."
log stream --predicate 'processImagePath CONTAINS "Actrium"' --style compact > "$LOG_FILE" 2>&1 &
LOG_PID=$!
sleep 1

# Step 3: Launch Actrium app
echo "[3/6] Launching Actrium app..."
open "$APP_PATH"
sleep 6

# Step 4: Capture Focus mode screenshot
echo "[4/6] Capturing Focus mode state..."
osascript -e 'tell application "Actrium" to activate'
sleep 1
screencapture -x "$SCREENSHOT_FOCUS"
echo "    Focus screenshot: $SCREENSHOT_FOCUS"

# Step 5: Switch to Overview mode and capture
echo "[5/6] Switching to Overview mode (Cmd+Shift+backslash)..."
osascript -e 'tell application "Actrium" to activate' -e 'delay 0.3' -e 'tell application "System Events" to keystroke "\\" using {command down, shift down}'
sleep 3
screencapture -x "$SCREENSHOT_OVERVIEW"
echo "    Overview screenshot: $SCREENSHOT_OVERVIEW"

# Step 6: Stop log capture and analyze
echo "[6/6] Analyzing logs..."
kill $LOG_PID 2>/dev/null || true
sleep 1

echo ""
echo "=== LOG ANALYSIS ==="
echo ""
echo "--- containerSizeChanged notifications ---"
grep -E "containerSizeChanged|containerSize" "$LOG_FILE" | tail -20 || echo "(none found)"

echo ""
echo "--- isOverviewMode changes ---"
grep -E "isOverviewMode|applyOverviewMode" "$LOG_FILE" | tail -10 || echo "(none found)"

echo ""
echo "--- React Native rendering ---"
grep -E "StartPageApp|SystemAppRouter|COMPACT" "$LOG_FILE" | tail -10 || echo "(none found)"

echo ""
echo "=== TEST COMPLETE ==="
echo "Log file: $LOG_FILE"
echo "Focus screenshot: $SCREENSHOT_FOCUS"
echo "Overview screenshot: $SCREENSHOT_OVERVIEW"
echo ""
echo "View screenshots with: open $SCREENSHOT_OVERVIEW"
