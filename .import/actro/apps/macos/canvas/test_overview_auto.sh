#!/bin/bash
# Automated test script for Overview mode fix
# Runs without user interaction

set -e

echo "=== Automated Overview Mode Test ==="
echo ""

# Configuration
APP_PATH="/Users/example-dev-2/Library/Developer/Xcode/DerivedData/Canvas-bfrmsauxlmtqwybyzrckiznxvqhp/Build/Products/Debug/Canvas.app"
LOG_FILE="/tmp/canvas_auto_test_$(date +%Y%m%d_%H%M%S).log"
SCREENSHOT_FOCUS="/tmp/canvas_auto_focus_$(date +%Y%m%d_%H%M%S).png"
SCREENSHOT_OVERVIEW="/tmp/canvas_auto_overview_$(date +%Y%m%d_%H%M%S).png"

# Step 1: Kill existing Canvas processes
echo "[1/6] Stopping existing Canvas processes..."
pkill -f "Canvas.app" 2>/dev/null || true
pkill -f "CanvasViewService" 2>/dev/null || true
sleep 2

# Step 2: Start log capture in background
echo "[2/6] Starting log capture..."
log stream --predicate 'processImagePath CONTAINS "Canvas"' --style compact > "$LOG_FILE" 2>&1 &
LOG_PID=$!
sleep 1

# Step 3: Launch Canvas app
echo "[3/6] Launching Canvas app..."
open "$APP_PATH"
sleep 6

# Step 4: Capture Focus mode screenshot
echo "[4/6] Capturing Focus mode state..."
osascript -e 'tell application "Canvas" to activate'
sleep 1
screencapture -x "$SCREENSHOT_FOCUS"
echo "    Focus screenshot: $SCREENSHOT_FOCUS"

# Step 5: Switch to Overview mode and capture
echo "[5/6] Switching to Overview mode (Cmd+Shift+backslash)..."
osascript -e 'tell application "Canvas" to activate' -e 'delay 0.3' -e 'tell application "System Events" to keystroke "\\" using {command down, shift down}'
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
