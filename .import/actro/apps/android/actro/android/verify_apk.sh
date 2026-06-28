#!/bin/bash
set -e

echo "======================================"
echo "开始构建和验证 APK"
echo "======================================"

cd /Users/example/Documents/Project/example-app/canvas-kotlin/android

echo ""
echo "1. 清理旧的构建产物..."
./gradlew clean

echo ""
echo "2. 构建 Debug APK..."
./gradlew :app:assembleDebug

APK_PATH="app/build/outputs/apk/debug/app-debug.apk"

if [ ! -f "$APK_PATH" ]; then
    echo ""
    echo "❌ 错误：APK 文件不存在！"
    exit 1
fi

echo ""
echo "✅ APK 构建成功！"
echo "APK 路径: $APK_PATH"
echo "APK 大小: $(du -h $APK_PATH | cut -f1)"

echo ""
echo "3. 检查 APK 中的 .so 文件..."
echo "======================================"

echo ""
echo "所有 arm64-v8a 架构的 .so 文件："
unzip -l "$APK_PATH" | grep "lib/arm64-v8a/.*\.so" | awk '{print $4}'

echo ""
echo "4. 检查关键库是否存在..."
echo "======================================"

if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/libreactnative.so"; then
    echo "✅ libreactnative.so 存在"
else
    echo "❌ libreactnative.so 不存在"
fi

if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/libhermes.so"; then
    echo "✅ libhermes.so 存在"
else
    echo "❌ libhermes.so 不存在"
fi

if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/libfbjni.so"; then
    echo "✅ libfbjni.so 存在"
else
    echo "❌ libfbjni.so 不存在"
fi

echo ""
echo "5. 验证独立的 libreact_featureflagsjni.so..."
if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/libreact_featureflagsjni.so"; then
    echo "⚠️  警告：发现独立的 libreact_featureflagsjni.so（不应该存在）"
else
    echo "✅ 正确：libreact_featureflagsjni.so 已合并到 libreactnative.so 中"
fi

echo ""
echo "6. 验证独立的 librninstance.so..."
if unzip -l "$APK_PATH" | grep -q "lib/arm64-v8a/librninstance.so"; then
    echo "⚠️  警告：发现独立的 librninstance.so（不应该存在）"
else
    echo "✅ 正确：librninstance.so 已合并到 libreactnative.so 中"
fi

echo ""
echo "======================================"
echo "验证完成！"
echo "======================================"
echo ""
echo "7. 安装到设备..."
./gradlew :app:installDebug

echo ""
echo "✅ 所有步骤完成！请在设备上运行应用并检查是否还有 .so 找不到的错误。"
