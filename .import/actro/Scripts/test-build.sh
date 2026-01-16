#!/bin/bash

# ==============================================================================
# 本地测试构建脚本（不上传）
# 用于验证构建配置是否正确
# ==============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# 颜色输出
BLUE='\033[0;34m'
GREEN='\033[0;32m'
NC='\033[0m'

echo -e "${BLUE}===================================================${NC}"
echo -e "${BLUE}📦 本地测试构建${NC}"
echo -e "${BLUE}===================================================${NC}"
echo ""

cd "$PROJECT_ROOT"

# 安装依赖
echo "📦 安装 React Native 依赖..."
cd Subsystem
if command -v bun &> /dev/null; then
    bun install
else
    echo "⚠️  bun 未安装，使用 npm..."
    npm install
fi
cd "$PROJECT_ROOT"

echo "📦 安装 CocoaPods 依赖..."
pod install

# 清理
echo "🧹 清理旧构建..."
rm -rf build
xcodebuild clean \
    -workspace Canvas.xcworkspace \
    -scheme Canvas \
    -configuration Release

# 构建 Archive
echo ""
echo "🏗️  开始构建 Archive..."
xcodebuild archive \
    -workspace Canvas.xcworkspace \
    -scheme Canvas \
    -configuration Release \
    -archivePath build/Canvas.xcarchive \
    -destination "generic/platform=iOS" \
    -allowProvisioningUpdates \
    CODE_SIGN_STYLE=Automatic

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
    echo "💡 如需导出 IPA，请运行："
    echo "   xcodebuild -exportArchive -archivePath build/Canvas.xcarchive -exportPath build/IPA -exportOptionsPlist Scripts/ExportOptions.plist"
else
    echo -e "${RED}❌ 构建失败${NC}"
    exit 1
fi
