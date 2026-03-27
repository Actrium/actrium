#!/bin/bash -eu
#
# Clean cache and reinstall the project dependent libraries.

# 全局清理
echo "🧹 全局清理旧构建和依赖..."
rm -rf build
rm -rf Subsystem/node_modules
rm -rf Pods

# 清理 Watchman 和 Metro 缓存
if command -v watchman &> /dev/null; then
    echo "🧹 清理 Watchman 缓存..."
    watchman watch-del-all
fi

# 安装依赖
echo "📦 安装 React Native 依赖..."
cd Subsystem

if command -v yarn &> /dev/null; then
    echo "📦 使用 yarn 安装依赖 (优先)..."
    yarn install
elif command -v bun &> /dev/null; then
    echo "⚠️  yarn 未安装，使用 bun..."
    echo "🧹 删除 lock 文件以避免 bun 冲突..."
    bun install
else
    echo "❌ 错误: 未找到 yarn 或 bun"
    exit 1
fi

echo "📦 安装 CocoaPods 依赖..."
cd ..
pod install

# 清理
echo "🧹 清理旧构建..."
xcodebuild clean \
    -workspace Canvas.xcworkspace \
    -scheme "AskAway"

# 安装 Swift Package Manager 依赖
# xcodebuild -resolvePackageDependencies

printf '\33[1;32m%s\33[0m\n' 'Done!'
exit 0
