#!/bin/bash -eu
#
# Clean cache and reinstall the project dependent libraries.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
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

# Installation Dependence
echo "📦 Install React Native Dependency..."
cd ActroRuntime

if command -v yarn &> /dev/null; then
    echo "📦 Use yarn Installation Dependence (Priority)..."
    yarn install
elif command -v bun &> /dev/null; then
    echo "⚠️  yarn Not installed, used bun..."
    echo "🧹 Delete lock File to Avoid bun Conflict..."
    bun install
else
    echo "❌ Error: Not found yarn or bun"
    exit 1
fi

echo "📦 Install CocoaPods Dependency..."
cd ..
pod install

# Cleaning
echo "🧹 Clear old build..."
xcodebuild clean \
    -workspace Actro.xcworkspace \
    -scheme "Actro"

# Install Swift Package Manager Dependency
# xcodebuild -resolvePackageDependencies

printf '\33[1;32m%s\33[0m\n' 'Done!'
exit 0
