#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "======================================"
echo "Start Build and Validate APK"
echo "======================================"

cd "$SCRIPT_DIR"

echo ""
echo "1. Clean up old construction..."
./gradlew clean

echo ""
echo "2. Build Debug APK..."
./gradlew :app:assembleDebug

APK_PATH="app/build/outputs/apk/debug/app-debug.apk"

if [ ! -f "$APK_PATH" ]; then
    echo ""
    echo "❌ Error:APK File does not exist!"
    exit 1
fi

echo ""
echo "✅ APK Construction succeeded!"
echo "APK Path: $APK_PATH"
echo "APK Size: $(du -h $APK_PATH | cut -f1)"

echo ""
echo "3. Inspection APK Medium .so Documentation..."
echo "======================================"

echo ""
echo "All arm64-v8a Structured .so Documents:"
unzip -l "$APK_PATH" | grep "lib/arm64-v8a/.*\.so" | awk '{print $4}'

echo ""
echo "4. Check if the key library exists...."
echo "======================================"

if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/libreactnative.so"; then
    echo "✅ libreactnative.so Existence"
else
    echo "❌ libreactnative.so Cannot initialise Evolution's mail component."
fi

if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/libhermes.so"; then
    echo "✅ libhermes.so Existence"
else
    echo "❌ libhermes.so Cannot initialise Evolution's mail component."
fi

if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/libfbjni.so"; then
    echo "✅ libfbjni.so Existence"
else
    echo "❌ libfbjni.so Cannot initialise Evolution's mail component."
fi

echo ""
echo "5. Verify Independence libreact_featureflagsjni.so..."
if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/libreact_featureflagsjni.so"; then
    echo "⚠️  Warning: Discovery of Independence libreact_featureflagsjni.so"It shouldn't exist.""
else
    echo "✅ Correct:libreact_featureflagsjni.so Merged to libreactnative.so Medium"
fi

echo ""
echo "6. Verify Independence librninstance.so..."
if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/librninstance.so"; then
    echo "⚠️  Warning: Discovery of Independence librninstance.so"It shouldn't exist.""
else
    echo "✅ Correct:librninstance.so Merged to libreactnative.so Medium"
fi

echo ""
echo "======================================"
echo "Validation complete!"
echo "======================================"
echo ""
echo "7. Installed to Device..."
./gradlew :app:installDebug

echo ""
echo "✅ All steps complete! Please apply to the equipment and check if there is any more .so Can't find the error."
