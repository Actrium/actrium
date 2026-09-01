#!/bin/bash

# ==============================================================================
# Local Test Build Script (unremote)
# To verify whether the build configuration is correct
# ==============================================================================

set -e

# Get Build (pkg: debugging, pkg-prd: app-store-connect)
BUILD_TYPE="${1:-pkg-prd}"
if [ "$BUILD_TYPE" = "pkg" ]; then
    EXPORT_METHOD="debugging"
else
    EXPORT_METHOD="app-store-connect"
fi

# Record start time
START_TIME=$(date +%s)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SCHEME="Actro"

# Colour Output
BLUE='\033[0;34m'
GREEN='\033[0;32m'
NC='\033[0m'

echo -e "${BLUE}===================================================${NC}"
echo -e "${BLUE}📦 Local Test Build${NC}"
echo -e "${BLUE}===================================================${NC}"
echo ""

cd "$PROJECT_ROOT"
"$SCRIPT_DIR/ensure-actro-runtime.sh"

# Global Cleanup
echo "🧹 Global cleanup of old builders and dependencies..."
rm -rf build
rm -rf ActroRuntime/node_modules
rm -rf Pods

# Cleaning Watchman and Metro Cache
if command -v watchman &> /dev/null; then
    echo "🧹 Cleaning Watchman Cache..."
    watchman watch-del-all
fi

# Pack up. React Native Bundle and resources
echo "📦 Pack up. React Native Bundle..."
cd ActroRuntime

# Installation Dependence
echo "📦 Install React Native Dependency..."

if command -v npm &> /dev/null; then
    echo "📦 Use npm Installation Dependence (Priority)..."
    npm install
elif command -v bun &> /dev/null; then
    echo "⚠️  npm Not installed, used bun..."
    echo "🧹 Delete lock File to Avoid bun Conflict..."
    bun install
else
    echo "❌ Error: Not found npm or bun"
    exit 1
fi

node node_modules/react-native/scripts/bundle.js \
    --platform ios \
    --dev false \
    --entry-file index.ts \
    --bundle-output "$PROJECT_ROOT/Actro/actr.jsbundle" \
    --assets-dest "$PROJECT_ROOT/Actro"

cd "$PROJECT_ROOT"

echo "📦 Install CocoaPods Dependency..."
pod install

# Generate ExportOptions.plist(from environmental variables) Team ID)
echo "📝 Generate ExportOptions.plist..."
if [ -z "$APPLE_TEAM_ID" ]; then
    echo "❌ Error: No Environment Variable Set APPLE_TEAM_ID"
    echo "Please be in. ~/.zshrc or ~/.bash_profile Medium Settings: export APPLE_TEAM_ID=\"your-team-id\""
    exit 1
fi
TEAM_ID="$APPLE_TEAM_ID"
echo "🔑 Use Team ID: $TEAM_ID"

cat > "$SCRIPT_DIR/ExportOptions.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>method</key>
    <string>$EXPORT_METHOD</string>
    <key>compileBitcode</key>
    <false/>
    <key>uploadBitcode</key>
    <true/>
    <key>thinning</key>
    <string>&lt;none&gt;</string>
    <key>signingStyle</key>
    <string>automatic</string>
    <key>teamID</key>
    <string>$TEAM_ID</string>
    <key>destination</key>
    <string>export</string>
</dict>
</plist>
EOF

# Cleaning
echo "🧹 Clear old build..."
xcodebuild clean \
    -workspace Actro.xcworkspace \
    -scheme "$SCHEME"

# Build Archive
echo ""
echo "🏗️  Start Build Archive..."
xcodebuild archive \
    -workspace Actro.xcworkspace \
    -scheme "$SCHEME" \
    -configuration Release \
    -archivePath build/Actro.xcarchive \
    -verbose

if [ -d "build/Actro.xcarchive" ]; then
    echo ""
    echo -e "${GREEN}✅ Construction succeeded!${NC}"
    echo ""
    echo "📁 Archive Path: build/Actro.xcarchive"
    
    # Show Build Information
    BUILD_VERSION=$(agvtool what-marketing-version -terse1)
    BUILD_NUMBER=$(agvtool what-version -terse)
    
    echo "🔢 Version: $BUILD_VERSION"
    echo "🏗️  Build: $BUILD_NUMBER"
    echo ""

    # Export IPA To Desktop
    TIMESTAMP=$(date +%Y%m%d-%H%M%S)
    IPA_NAME="${SCHEME}-${TIMESTAMP}-${BUILD_VERSION}-${BUILD_NUMBER}.ipa"
    DESKTOP_PATH="$HOME/Desktop"
    
    echo "📦 Exporting IPA and rename..."
    xcodebuild -exportArchive \
        -archivePath build/Actro.xcarchive \
        -exportPath build/IPA \
        -exportOptionsPlist Scripts/ExportOptions.plist \
        -configuration Release \
        -allowProvisioningUpdates \
        -verbose

    if [ -f "build/IPA/$SCHEME.ipa" ]; then
        mv "build/IPA/$SCHEME.ipa" "$DESKTOP_PATH/$IPA_NAME"
        echo -e "${GREEN}✅ IPA Exported successfully to: $DESKTOP_PATH/$IPA_NAME${NC}"
    else
        # Sometimes the exported filename may not be Actro.ipa, try to find the only one in the folder ipa
        EXTRACTED_IPA=$(find build/IPA -name "*.ipa" | head -n 1)
        if [ -n "$EXTRACTED_IPA" ]; then
            mv "$EXTRACTED_IPA" "$DESKTOP_PATH/$IPA_NAME"
            echo -e "${GREEN}✅ IPA Exported successfully to: $DESKTOP_PATH/$IPA_NAME${NC}"
        else
            echo -e "${RED}❌ Could not find generated IPA Documentation${NC}"
        fi
    fi
else
    echo -e "${RED}❌ Build Failed${NC}"
    exit 1
fi

# Record the end time and calculate the total time taken
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))
MINUTES=$((DURATION / 60))
SECONDS=$((DURATION % 60))

echo -e "${BLUE}===================================================${NC}"
echo -e "${GREEN}🎉 All missions completed!${NC}"
echo -e "${BLUE}⏱️  Total time-consuming: ${MINUTES}min${SECONDS}sec${NC}"
echo -e "${BLUE}===================================================${NC}"
