#!/bin/bash

# ==============================================================================
# 本地测试构建脚本（不上传）
# 用于验证构建配置是否正确
# ==============================================================================

set -e

# 记录开始时间
START_TIME=$(date +%s)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SCHEME="AskAway"

# 颜色输出
BLUE='\033[0;34m'
GREEN='\033[0;32m'
NC='\033[0m'

echo -e "${BLUE}===================================================${NC}"
echo -e "${BLUE}📦 本地测试构建${NC}"
echo -e "${BLUE}===================================================${NC}"
echo ""

cd "$PROJECT_ROOT"

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

# 打包 React Native Bundle 和资源
echo "📦 打包 React Native Bundle..."
cd Subsystem

# 安装依赖
echo "📦 安装 React Native 依赖..."

if command -v npm &> /dev/null; then
    echo "📦 使用 npm 安装依赖 (优先)..."
    npm install
elif command -v bun &> /dev/null; then
    echo "⚠️  npm 未安装，使用 bun..."
    echo "🧹 删除 lock 文件以避免 bun 冲突..."
    bun install
else
    echo "❌ 错误: 未找到 npm 或 bun"
    exit 1
fi

npx react-native bundle \
    --platform ios \
    --dev false \
    --entry-file index.ts \
    --bundle-output "$PROJECT_ROOT/Canvas/actr.jsbundle" \
    --assets-dest "$PROJECT_ROOT/Canvas"

cd "$PROJECT_ROOT"

# 打包示例 App (counterapp.actr)
echo "📦 打包示例 App (counterapp.actr)..."
COUNTER_APP_DIR="$PROJECT_ROOT/Subsystem/actr-app-examples/counterapp"
OUTPUT_ASSET_DIR="$PROJECT_ROOT/Canvas/Resources"
if [ -d "$COUNTER_APP_DIR" ]; then
    mkdir -p "$OUTPUT_ASSET_DIR"
    cd "$COUNTER_APP_DIR"
    zip -j "$OUTPUT_ASSET_DIR/counterapp.actr" manifest.json unified-app.js
    echo "✅ counterapp.actr 已更新至 $OUTPUT_ASSET_DIR"
fi
cd "$PROJECT_ROOT"

echo "📦 安装 CocoaPods 依赖..."
pod install

# 清理
echo "🧹 清理旧构建..."
xcodebuild clean \
    -workspace Canvas.xcworkspace \
    -scheme "$SCHEME"

# 构建 Archive
echo ""
echo "🏗️  开始构建 Archive..."
xcodebuild archive \
    -workspace Canvas.xcworkspace \
    -scheme "$SCHEME" \
    -configuration Release \
    -archivePath build/Canvas.xcarchive \
    -verbose

if [ -d "build/Canvas.xcarchive" ]; then
    echo ""
    echo -e "${GREEN}✅ 构建成功！${NC}"
    echo ""
    echo "📁 Archive 路径: build/Canvas.xcarchive"
    
    # 显示构建信息
    BUILD_VERSION=$(agvtool what-marketing-version -terse1)
    BUILD_NUMBER=$(agvtool what-version -terse)
    
    echo "🔢 版本: $BUILD_VERSION"
    echo "🏗️  Build: $BUILD_NUMBER"
    echo ""

    # 导出 IPA 到桌面
    TIMESTAMP=$(date +%Y%m%d-%H%M%S)
    IPA_NAME="${TIMESTAMP}-${BUILD_VERSION}-${BUILD_NUMBER}.ipa"
    DESKTOP_PATH="$HOME/Desktop"
    
    echo "📦 正在导出 IPA 并重命名..."
    xcodebuild -exportArchive \
        -archivePath build/Canvas.xcarchive \
        -exportPath build/IPA \
        -exportOptionsPlist Scripts/ExportOptions.plist \
        -configuration Release \
        -allowProvisioningUpdates \
        -verbose

    if [ -f "build/IPA/$SCHEME.ipa" ]; then
        mv "build/IPA/$SCHEME.ipa" "$DESKTOP_PATH/$IPA_NAME"
        echo -e "${GREEN}✅ IPA 已成功导出至: $DESKTOP_PATH/$IPA_NAME${NC}"
    else
        # 有时导出的文件名可能不是 Canvas.ipa，尝试查找文件夹下唯一的 ipa
        EXTRACTED_IPA=$(find build/IPA -name "*.ipa" | head -n 1)
        if [ -n "$EXTRACTED_IPA" ]; then
            mv "$EXTRACTED_IPA" "$DESKTOP_PATH/$IPA_NAME"
            echo -e "${GREEN}✅ IPA 已成功导出至: $DESKTOP_PATH/$IPA_NAME${NC}"
        else
            echo -e "${RED}❌ 未能找到生成的 IPA 文件${NC}"
        fi
    fi
else
    echo -e "${RED}❌ 构建失败${NC}"
    exit 1
fi

# 记录结束时间并计算总耗时
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))
MINUTES=$((DURATION / 60))
SECONDS=$((DURATION % 60))

echo -e "${BLUE}===================================================${NC}"
echo -e "${GREEN}🎉 所有任务已完成！${NC}"
echo -e "${BLUE}⏱️  总耗时: ${MINUTES}分${SECONDS}秒${NC}"
echo -e "${BLUE}===================================================${NC}"
