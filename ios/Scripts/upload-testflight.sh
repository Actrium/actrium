#!/bin/bash

# ==============================================================================
# TestFlight Autobuild Upload Script
# ==============================================================================
# Function:
# 1. Automatic Increase Build Number
# 2. Build Archive
# 3. Export IPA
# 4. Upload to TestFlight
# 5. Notify build result
# ==============================================================================

set -e  # Exit with error
set -o pipefail  # Exit when Pipe Command Failed

# ==============================================================================
# Configure Area
# ==============================================================================

# Project Configuration
PROJECT_NAME="Actrium"
WORKSPACE_PATH="Actrium.xcworkspace"
SCHEME="Actrium"
CONFIGURATION="Release"

# Path Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_ROOT/build"
ARCHIVE_PATH="$BUILD_DIR/Actrium.xcarchive"
EXPORT_PATH="$BUILD_DIR/Actrium-IPA"
EXPORT_OPTIONS_PATH="$SCRIPT_DIR/ExportOptions.plist"

# Load local configuration
CONFIG_FILE="$SCRIPT_DIR/.config"
if [ -f "$CONFIG_FILE" ]; then
    source "$CONFIG_FILE"
fi

# Colour Output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check for necessary configurations
if [ -z "$DEVELOPMENT_TEAM" ]; then
    echo -e "${RED}❌ Error:DEVELOPMENT_TEAM Not configured${NC}"
    echo ""
    echo "Please create Scripts/.config The document and the following text were added:"
    echo ""
    echo "DEVELOPMENT_TEAM=\"YOUR_TEAM_ID\""
    echo ""
    echo "Clip: From Scripts/.config.example Copy Template"
    exit 1
fi

# ==============================================================================
# Tools Functions
# ==============================================================================

log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
}

log_step() {
    echo ""
    echo -e "${BLUE}===================================================${NC}"
    echo -e "${BLUE}📍 $1${NC}"
    echo -e "${BLUE}===================================================${NC}"
}

check_command() {
    if ! command -v "$1" &> /dev/null; then
        log_error "$1 Not installed, please install first"
        exit 1
    fi
}

# ==============================================================================
# Precheck
# ==============================================================================

log_step "Precheck"

# Checking for necessary orders
check_command xcodebuild
check_command xcrun
check_command agvtool

# Inspection npm or bun Installed
if command -v npm &> /dev/null; then
    log_success "npm Installed: $(npm --version)"
elif command -v bun &> /dev/null; then
    log_success "bun Installed: $(bun --version)"
else
    log_error "npm and bun None installed, please install one of them first"
    exit 1
fi

# Check if in the root directory
if [ ! -f "$PROJECT_ROOT/Actrium.xcworkspace/contents.xcworkspacedata" ]; then
    log_error "Not found Actrium.xcworkspace, please ensure that the project root directory is implemented"
    exit 1
fi

log_success "Precheck complete"
"$SCRIPT_DIR/ensure-actrium-runtime.sh"

# ==============================================================================
# Clear old build
# ==============================================================================

log_step "Clear old build"

# Global Cleanup
log_info "Cleans up old build and dependencies..."
rm -rf "$BUILD_DIR"
rm -rf "$PROJECT_ROOT/ActriumRuntime/node_modules"
rm -rf "$PROJECT_ROOT/Pods"

# Cleaning Watchman and Metro Cache
if command -v watchman &> /dev/null; then
    log_info "Cleaning Watchman Cache..."
    watchman watch-del-all
fi

mkdir -p "$BUILD_DIR"

log_success "Cleanup complete."

# ==============================================================================
# Increase Build Number
# ==============================================================================

log_step "Update Build Number"

cd "$PROJECT_ROOT"

# Get Current Build Number
CURRENT_BUILD=$(agvtool what-version -terse)
log_info "Current Build Number: $CURRENT_BUILD"

# Automatic Increase Build Number
agvtool next-version -all

NEW_BUILD=$(agvtool what-version -terse)
log_success "New Build Number: $NEW_BUILD"

# Access Version
VERSION=$(agvtool what-marketing-version -terse1)
log_info "Current Version: $VERSION"

# ==============================================================================
# Install React Native Dependency (Use) bun)
# ==============================================================================

log_step "Install React Native Dependency"

cd "$PROJECT_ROOT/ActriumRuntime"

if command -v yarn &> /dev/null; then
    log_info "Use yarn Installation Dependence..."
    yarn install
elif command -v bun &> /dev/null; then
    log_info "yarn Not installed, used bun Installation Dependence..."
    bun install
else
    log_error "yarn and bun None installed, please install one of them first"
    exit 1
fi

cd "$PROJECT_ROOT"

log_success "React Native Reliance on installation completed"

# ==============================================================================
# Pack up. React Native Bundle and resources
# ==============================================================================

log_step "Pack up. React Native Bundle"

cd "$PROJECT_ROOT/ActriumRuntime"

# Run Packing Commands
# --bundle-output Output To Actrium Root directory, matching ActriumReactNativeFactoryDelegate.bundleURL Reference Logic
# --assets-dest Output To Actrium Root directory, generate assets/ Folder Level
node node_modules/react-native/scripts/bundle.js \
    --platform ios \
    --dev false \
    --entry-file index.ts \
    --bundle-output "$PROJECT_ROOT/Actrium/runtime.jsbundle" \
    --assets-dest "$PROJECT_ROOT/Actrium"

cd "$PROJECT_ROOT"

log_success "React Native Bundle And the resources are packed."

# ==============================================================================
# Install CocoaPods Dependency
# ==============================================================================

log_step "Install CocoaPods Dependency"

if [ ! -d "$PROJECT_ROOT/Pods" ]; then
    log_info "Pods Directory does not exist, installation is under way..."
    pod install
else
    log_info "Update CocoaPods Dependency..."
    pod install
fi

log_success "CocoaPods Reliance on installation completed"

# ==============================================================================
# Build Archive
# ==============================================================================

log_step "Start Build Archive"

log_info "Configure: $CONFIGURATION"
log_info "Scheme: $SCHEME"

xcodebuild archive \
    -workspace "$WORKSPACE_PATH" \
    -scheme "$SCHEME" \
    -configuration "$CONFIGURATION" \
    -archivePath "$ARCHIVE_PATH" \
    -destination "generic/platform=iOS" \
    -allowProvisioningUpdates \
    CODE_SIGN_STYLE=Automatic \
    DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM"

if [ ! -d "$ARCHIVE_PATH" ]; then
    log_error "Archive Build Failed"
    exit 1
fi

log_success "Archive Build complete: $ARCHIVE_PATH"

# ==============================================================================
# Generate ExportOptions.plist(Dynamic use) DEVELOPMENT_TEAM)
# ==============================================================================

log_step "Generate ExportOptions.plist"

cat > "$EXPORT_OPTIONS_PATH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>method</key>
    <string>app-store-connect</string>
    <key>uploadBitcode</key>
    <false/>
    <key>uploadSymbols</key>
    <true/>
    <key>compileBitcode</key>
    <false/>
    <key>signingStyle</key>
    <string>automatic</string>
    <key>teamID</key>
    <string>$DEVELOPMENT_TEAM</string>
    <key>destination</key>
    <string>export</string>
</dict>
</plist>
EOF

log_success "ExportOptions.plist Generated (Team ID: $DEVELOPMENT_TEAM)"

# ==============================================================================
# Export IPA
# ==============================================================================

log_step "Export IPA"

xcodebuild -exportArchive \
    -archivePath "$ARCHIVE_PATH" \
    -exportPath "$EXPORT_PATH" \
    -exportOptionsPlist "$EXPORT_OPTIONS_PATH" \
    -configuration "$CONFIGURATION" \
    -allowProvisioningUpdates

IPA_PATH="$EXPORT_PATH/$PROJECT_NAME.ipa"

if [ ! -f "$IPA_PATH" ]; then
    # Try to find other named names IPA Documentation
    IPA_PATH=$(find "$EXPORT_PATH" -name "*.ipa" | head -n 1)
    if [ -z "$IPA_PATH" ]; then
        log_error "IPA Export failed"
        exit 1
    fi
fi

IPA_SIZE=$(du -h "$IPA_PATH" | cut -f1)
log_success "IPA Export complete: $IPA_PATH ($IPA_SIZE)"

# ==============================================================================
# Upload to TestFlight
# ==============================================================================

log_step "Upload to TestFlight"

log_info "Start Uploading IPA..."
log_warning "It'll take a few minutes to upload. Please wait...."

# Check if there's any. API Key(Recommended modalities)
if [ -n "$APP_STORE_CONNECT_API_KEY_PATH" ] && [ -f "$APP_STORE_CONNECT_API_KEY_PATH" ] \
   && [ -n "$APP_STORE_CONNECT_API_KEY_ID" ] && [ -n "$APP_STORE_CONNECT_API_ISSUER_ID" ]; then
    log_info "Use App Store Connect API Key Upload"

    xcrun altool --upload-app \
        -f "$IPA_PATH" \
        -t ios \
        --apiKey "$APP_STORE_CONNECT_API_KEY_ID" \
        --apiIssuer "$APP_STORE_CONNECT_API_ISSUER_ID" \
        2>&1 | tee "$BUILD_DIR/upload.log"

    if [ ${PIPESTATUS[0]} -eq 0 ]; then
        log_success "Upload successful!"
    else
        log_error "Upload failed, see log: $BUILD_DIR/upload.log"
        exit 1
    fi
elif [ -n "$APPLE_ID" ]; then
    log_info "Use Apple ID Upload"
    log_warning "Suggested Configuration App Store Connect API Key . To avoid interactive login"

    xcrun altool --upload-app \
        -f "$IPA_PATH" \
        -t ios \
        -u "$APPLE_ID" \
        -p "@keychain:AC_PASSWORD" \
        2>&1 | tee "$BUILD_DIR/upload.log"

    if [ ${PIPESTATUS[0]} -eq 0 ]; then
        log_success "Upload successful!"
    else
        log_error "Upload failed, see log: $BUILD_DIR/upload.log"
        exit 1
    fi
else
    log_warning "Not configured App Store Connect API Key or Apple IDSkip Upload"
    log_info "Please be in. Scripts/.config , and then click the"
    log_info "  APP_STORE_CONNECT_API_KEY_PATH=\"/path/to/AuthKey.p8\""
    log_info "  APP_STORE_CONNECT_API_KEY_ID=\"YOUR_KEY_ID\""
    log_info "  APP_STORE_CONNECT_API_ISSUER_ID=\"YOUR_ISSUER_ID\""
    log_info "IPA Save File In: $IPA_PATH"
fi

# ==============================================================================
# Build Summary
# ==============================================================================

log_step "Build Summary"

echo ""
echo "📦 Apply Name: $PROJECT_NAME"
echo "🔢 Version Number: $VERSION"
echo "🏗️  Build Number: $NEW_BUILD"
echo "📁 Archive: $ARCHIVE_PATH"
echo "📱 IPA: $IPA_PATH"
echo "💾 Size: $IPA_SIZE"
echo ""

log_success "Build and upload complete!"
log_info "Please be in. App Store Connect Center to see build status"
log_info "Usually. 5-15 Minute processing complete."

# ==============================================================================
# Git Label (optional)
# ==============================================================================

if [ "$GIT_TAG" = "true" ]; then
    log_step "Create Git Tab"

    TAG_NAME="v${VERSION}"
    git tag -a "$TAG_NAME" -m "Release $VERSION (Build $NEW_BUILD)"
    git push origin "$TAG_NAME"

    log_success "Git Tab created: $TAG_NAME"
fi

# ==============================================================================
# Completed
# ==============================================================================

echo ""
log_success "All done!"
echo ""
