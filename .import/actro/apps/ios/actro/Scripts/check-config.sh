#!/bin/bash

# ==============================================================================
# TestFlight 配置检查脚本
# ==============================================================================

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}===================================================${NC}"
echo -e "${BLUE}🔍 TestFlight 配置检查${NC}"
echo -e "${BLUE}===================================================${NC}"
echo ""

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

ERRORS=0
WARNINGS=0

check_ok() {
    echo -e "${GREEN}✅ $1${NC}"
}

check_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
    WARNINGS=$((WARNINGS + 1))
}

check_error() {
    echo -e "${RED}❌ $1${NC}"
    ERRORS=$((ERRORS + 1))
}

echo "📋 检查必需工具..."
echo ""

# 检查 Xcode
if command -v xcodebuild &> /dev/null; then
    XCODE_VERSION=$(xcodebuild -version | head -n 1)
    check_ok "Xcode: $XCODE_VERSION"
else
    check_error "Xcode 未安装"
fi

# 检查 xcpretty
if command -v xcpretty &> /dev/null; then
    check_ok "xcpretty 已安装"
else
    check_warning "xcpretty 未安装 (可选，用于美化输出)"
    echo "          安装: gem install xcpretty"
fi

# 检查 CocoaPods
if command -v pod &> /dev/null; then
    POD_VERSION=$(pod --version)
    check_ok "CocoaPods: $POD_VERSION"
else
    check_error "CocoaPods 未安装"
    echo "          安装: sudo gem install cocoapods"
fi

# 检查 bun
if command -v bun &> /dev/null; then
    BUN_VERSION=$(bun --version)
    check_ok "bun: $BUN_VERSION"
else
    check_warning "bun 未安装（脚本会自动安装）"
    echo "          手动安装: curl -fsSL https://bun.com/install | bash"
fi

# 检查 agvtool
if command -v agvtool &> /dev/null; then
    check_ok "agvtool 已安装"
else
    check_error "agvtool 未安装"
fi

echo ""
echo "📁 检查项目文件..."
echo ""

# 检查 Workspace
if [ -f "Actro.xcworkspace/contents.xcworkspacedata" ]; then
    check_ok "Workspace 存在"
else
    check_error "Actro.xcworkspace 不存在"
fi

# 检查 Pods
if [ -d "Pods" ]; then
    check_ok "CocoaPods 依赖已安装"
else
    check_warning "CocoaPods 依赖未安装"
    echo "          运行: pod install"
fi

# 检查 ActroRuntime symlink
if [ -L "ActroRuntime" ] && [ -d "ActroRuntime" ]; then
    check_ok "ActroRuntime symlink 已配置"
elif [ -e "ActroRuntime" ]; then
    check_error "ActroRuntime 存在但不是有效 symlink"
else
    check_warning "ActroRuntime symlink 未创建"
    echo "          运行: ./Scripts/ensure-actro-runtime.sh"
fi

# 检查 React Native 依赖
if [ -d "ActroRuntime/node_modules" ]; then
    check_ok "React Native 依赖已安装"
else
    check_warning "React Native 依赖未安装"
    echo "          运行: cd ActroRuntime && bun install"
fi

# 检查 ExportOptions.plist
if [ -f "Scripts/ExportOptions.plist" ]; then
    check_ok "ExportOptions.plist 存在"
    
    # 检查是否包含 YOUR_TEAM_ID
    if grep -q "YOUR_TEAM_ID" "Scripts/ExportOptions.plist"; then
        check_warning "ExportOptions.plist 包含占位符 YOUR_TEAM_ID"
        echo "          请修改为实际的 Team ID"
    fi
else
    check_warning "ExportOptions.plist 不存在，会在 build/upload 时按环境变量生成"
fi

echo ""
echo "🔐 检查认证配置..."
echo ""

# 检查 API Key 配置
if [ -n "$APP_STORE_CONNECT_API_KEY_PATH" ]; then
    if [ -f "$APP_STORE_CONNECT_API_KEY_PATH" ]; then
        check_ok "API Key 文件存在"
    else
        check_error "API Key 文件路径配置但文件不存在: $APP_STORE_CONNECT_API_KEY_PATH"
    fi
    
    if [ -n "$APP_STORE_CONNECT_API_KEY_ID" ]; then
        check_ok "API Key ID 已配置"
    else
        check_warning "APP_STORE_CONNECT_API_KEY_ID 未设置"
    fi
    
    if [ -n "$APP_STORE_CONNECT_API_ISSUER_ID" ]; then
        check_ok "API Issuer ID 已配置"
    else
        check_warning "APP_STORE_CONNECT_API_ISSUER_ID 未设置"
    fi
elif [ -n "$APPLE_ID" ]; then
    check_ok "Apple ID 已配置: $APPLE_ID"
    check_warning "建议使用 App Store Connect API Key 而不是 Apple ID"
else
    check_warning "未配置上传凭证 (API Key 或 Apple ID)"
    echo "          请参考 Scripts/TESTFLIGHT_README.md 配置"
fi

echo ""
echo "🎯 检查项目配置..."
echo ""

# 获取版本信息
if command -v agvtool &> /dev/null; then
    VERSION=$(agvtool what-marketing-version -terse1 2>/dev/null || echo "N/A")
    BUILD=$(agvtool what-version -terse 2>/dev/null || echo "N/A")
    
    echo "   版本号: $VERSION"
    echo "   Build: $BUILD"
    echo ""
fi

# 检查 Team ID
TEAM_ID=$(grep -A 1 "DEVELOPMENT_TEAM" Actro.xcodeproj/project.pbxproj | grep "=" | head -n 1 | cut -d'"' -f2 || echo "")
if [ -n "$TEAM_ID" ] && [ "$TEAM_ID" != "" ]; then
    check_ok "Team ID: $TEAM_ID"
else
    check_warning "Team ID 未在项目中配置"
fi

# 检查 Bundle ID
BUNDLE_ID=$(grep -A 1 "PRODUCT_BUNDLE_IDENTIFIER" Actro.xcodeproj/project.pbxproj | grep "=" | head -n 1 | cut -d'"' -f2 || echo "")
if [ -n "$BUNDLE_ID" ] && [ "$BUNDLE_ID" != "" ]; then
    check_ok "Bundle ID: $BUNDLE_ID"
else
    check_warning "Bundle ID 未在项目中配置"
fi

echo ""
echo "===================================================="
echo ""

if [ $ERRORS -eq 0 ] && [ $WARNINGS -eq 0 ]; then
    echo -e "${GREEN}🎉 所有检查通过！可以运行上传脚本${NC}"
    echo ""
    echo "运行: ./Scripts/upload-testflight.sh"
elif [ $ERRORS -eq 0 ]; then
    echo -e "${YELLOW}⚠️  有 $WARNINGS 个警告，建议修复后再上传${NC}"
    echo ""
    echo "如需继续: ./Scripts/upload-testflight.sh"
else
    echo -e "${RED}❌ 有 $ERRORS 个错误，请先修复${NC}"
    echo ""
    echo "请参考: Scripts/TESTFLIGHT_README.md"
    exit 1
fi

echo ""
echo "💡 提示："
echo "   - 首次上传建议先运行: ./Scripts/test-build.sh"
echo "   - 查看详细文档: Scripts/TESTFLIGHT_README.md"
echo "   - 环境变量配置: Scripts/.env.example"
echo ""
