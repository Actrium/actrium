#!/bin/bash

# ==============================================================================
# TestFlight 自动构建上传脚本
# ==============================================================================
# 功能：
# 1. 自动增加 Build Number
# 2. 构建 Archive
# 3. 导出 IPA
# 4. 上传到 TestFlight
# 5. 通知构建结果
# ==============================================================================

set -e  # 遇到错误立即退出
set -o pipefail  # 管道命令失败时退出

# ==============================================================================
# 配置区域
# ==============================================================================

# 项目配置
PROJECT_NAME="Canvas"
WORKSPACE_PATH="Canvas.xcworkspace"
SCHEME="AskAway"
CONFIGURATION="Release"

# 路径配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_ROOT/build"
ARCHIVE_PATH="$BUILD_DIR/Canvas.xcarchive"
EXPORT_PATH="$BUILD_DIR/AskAway-IPA"
EXPORT_OPTIONS_PATH="$SCRIPT_DIR/ExportOptions.plist"

# 加载本地配置
CONFIG_FILE="$SCRIPT_DIR/.config"
if [ -f "$CONFIG_FILE" ]; then
    source "$CONFIG_FILE"
fi

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 检查必需的配置
if [ -z "$DEVELOPMENT_TEAM" ]; then
    echo -e "${RED}❌ 错误：DEVELOPMENT_TEAM 未配置${NC}"
    echo ""
    echo "请创建 Scripts/.config 文件并添加以下内容："
    echo ""
    echo "DEVELOPMENT_TEAM=\"YOUR_TEAM_ID\""
    echo ""
    echo "提示：可以从 Scripts/.config.example 复制模板"
    exit 1
fi

# ==============================================================================
# 工具函数
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
        log_error "$1 未安装，请先安装"
        exit 1
    fi
}

# ==============================================================================
# 前置检查
# ==============================================================================

log_step "前置检查"

# 检查必需的命令
check_command xcodebuild
check_command xcrun
check_command agvtool

# 检查 npm 或 bun 是否已安装
if command -v npm &> /dev/null; then
    log_success "npm 已安装: $(npm --version)"
elif command -v bun &> /dev/null; then
    log_success "bun 已安装: $(bun --version)"
else
    log_error "npm 和 bun 均未安装，请先安装其中之一"
    exit 1
fi

# 检查是否在项目根目录
if [ ! -f "$PROJECT_ROOT/Canvas.xcworkspace/contents.xcworkspacedata" ]; then
    log_error "未找到 Canvas.xcworkspace，请确保在项目根目录执行"
    exit 1
fi

log_success "前置检查完成"

# ==============================================================================
# 清理旧构建
# ==============================================================================

log_step "清理旧构建"

# 全局清理
log_info "清理旧构建和依赖..."
rm -rf "$BUILD_DIR"
rm -rf "$PROJECT_ROOT/Subsystem/node_modules"
rm -rf "$PROJECT_ROOT/Pods"

# 清理 Watchman 和 Metro 缓存
if command -v watchman &> /dev/null; then
    log_info "清理 Watchman 缓存..."
    watchman watch-del-all
fi

mkdir -p "$BUILD_DIR"

log_success "清理完成"

# ==============================================================================
# 增加 Build Number
# ==============================================================================

log_step "更新 Build Number"

cd "$PROJECT_ROOT"

# 获取当前 Build Number
CURRENT_BUILD=$(agvtool what-version -terse)
log_info "当前 Build Number: $CURRENT_BUILD"

# 自动增加 Build Number
agvtool next-version -all

NEW_BUILD=$(agvtool what-version -terse)
log_success "新 Build Number: $NEW_BUILD"

# 获取 Version
VERSION=$(agvtool what-marketing-version -terse1)
log_info "当前 Version: $VERSION"

# ==============================================================================
# 安装 React Native 依赖（使用 bun）
# ==============================================================================

log_step "安装 React Native 依赖"

cd "$PROJECT_ROOT/Subsystem"

if command -v yarn &> /dev/null; then
    log_info "使用 yarn 安装依赖..."
    yarn install
elif command -v bun &> /dev/null; then
    log_info "yarn 未安装，使用 bun 安装依赖..."
    bun install
else
    log_error "yarn 和 bun 均未安装，请先安装其中之一"
    exit 1
fi

cd "$PROJECT_ROOT"

log_success "React Native 依赖安装完成"

# ==============================================================================
# 打包 React Native Bundle 和资源
# ==============================================================================

log_step "打包 React Native Bundle"

cd "$PROJECT_ROOT/Subsystem"

# 运行打包命令
# --bundle-output 输出到 Canvas 根目录，符合 CanvasReactNativeFactoryDelegate.bundleURL 引用逻辑
# --assets-dest 输出到 Canvas 根目录，会生成 assets/ 文件夹层级
npx react-native bundle \
    --platform ios \
    --dev false \
    --entry-file index.ts \
    --bundle-output "$PROJECT_ROOT/Canvas/actr.jsbundle" \
    --assets-dest "$PROJECT_ROOT/Canvas"

cd "$PROJECT_ROOT"

log_success "React Native Bundle 及资源打包完成"

# ==============================================================================
# 打包示例 .actr 应用
# ==============================================================================

log_step "打包示例 App (counterapp.actr)"

COUNTER_APP_DIR="$PROJECT_ROOT/Subsystem/actr-app-examples/counterapp"
OUTPUT_ASSET_DIR="$PROJECT_ROOT/Canvas/Resources"

if [ -d "$COUNTER_APP_DIR" ]; then
    cd "$COUNTER_APP_DIR"
    # 将 manifest.json 和 unified-app.js 压缩，重命名为 .actr 并输出到 Resources
    zip -j "$OUTPUT_ASSET_DIR/counterapp.actr" manifest.json unified-app.js
    log_success "counterapp.actr 已更新至 $OUTPUT_ASSET_DIR"
else
    log_warning "未找到 counterapp 目录，跳过 .actr 打包"
fi

cd "$PROJECT_ROOT"

# ==============================================================================
# 安装 CocoaPods 依赖
# ==============================================================================

log_step "安装 CocoaPods 依赖"

if [ ! -d "$PROJECT_ROOT/Pods" ]; then
    log_info "Pods 目录不存在，正在安装..."
    pod install
else
    log_info "更新 CocoaPods 依赖..."
    pod install
fi

log_success "CocoaPods 依赖安装完成"

# ==============================================================================
# 构建 Archive
# ==============================================================================

log_step "开始构建 Archive"

log_info "配置: $CONFIGURATION"
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
    log_error "Archive 构建失败"
    exit 1
fi

log_success "Archive 构建完成: $ARCHIVE_PATH"

# ==============================================================================
# 生成 ExportOptions.plist（动态使用 DEVELOPMENT_TEAM）
# ==============================================================================

log_step "生成 ExportOptions.plist"

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

log_success "ExportOptions.plist 已生成 (Team ID: $DEVELOPMENT_TEAM)"

# ==============================================================================
# 导出 IPA
# ==============================================================================

log_step "导出 IPA"

xcodebuild -exportArchive \
    -archivePath "$ARCHIVE_PATH" \
    -exportPath "$EXPORT_PATH" \
    -exportOptionsPlist "$EXPORT_OPTIONS_PATH" \
    -configuration "$CONFIGURATION" \
    -allowProvisioningUpdates

IPA_PATH="$EXPORT_PATH/$PROJECT_NAME.ipa"

if [ ! -f "$IPA_PATH" ]; then
    # 尝试查找其他命名的 IPA 文件
    IPA_PATH=$(find "$EXPORT_PATH" -name "*.ipa" | head -n 1)
    if [ -z "$IPA_PATH" ]; then
        log_error "IPA 导出失败"
        exit 1
    fi
fi

IPA_SIZE=$(du -h "$IPA_PATH" | cut -f1)
log_success "IPA 导出完成: $IPA_PATH ($IPA_SIZE)"

# ==============================================================================
# 上传到 TestFlight
# ==============================================================================

log_step "上传到 TestFlight"

log_info "开始上传 IPA..."
log_warning "上传可能需要几分钟，请耐心等待..."

# 检查是否有 API Key（推荐方式）
if [ -n "$APP_STORE_CONNECT_API_KEY_PATH" ] && [ -f "$APP_STORE_CONNECT_API_KEY_PATH" ] \
   && [ -n "$APP_STORE_CONNECT_API_KEY_ID" ] && [ -n "$APP_STORE_CONNECT_API_ISSUER_ID" ]; then
    log_info "使用 App Store Connect API Key 上传"

    xcrun altool --upload-app \
        -f "$IPA_PATH" \
        -t ios \
        --apiKey "$APP_STORE_CONNECT_API_KEY_ID" \
        --apiIssuer "$APP_STORE_CONNECT_API_ISSUER_ID" \
        2>&1 | tee "$BUILD_DIR/upload.log"

    if [ ${PIPESTATUS[0]} -eq 0 ]; then
        log_success "上传成功！"
    else
        log_error "上传失败，请查看日志: $BUILD_DIR/upload.log"
        exit 1
    fi
elif [ -n "$APPLE_ID" ]; then
    log_info "使用 Apple ID 上传"
    log_warning "建议配置 App Store Connect API Key 以避免交互式登录"

    xcrun altool --upload-app \
        -f "$IPA_PATH" \
        -t ios \
        -u "$APPLE_ID" \
        -p "@keychain:AC_PASSWORD" \
        2>&1 | tee "$BUILD_DIR/upload.log"

    if [ ${PIPESTATUS[0]} -eq 0 ]; then
        log_success "上传成功！"
    else
        log_error "上传失败，请查看日志: $BUILD_DIR/upload.log"
        exit 1
    fi
else
    log_warning "未配置 App Store Connect API Key 或 Apple ID，跳过上传"
    log_info "请在 Scripts/.config 中配置："
    log_info "  APP_STORE_CONNECT_API_KEY_PATH=\"/path/to/AuthKey.p8\""
    log_info "  APP_STORE_CONNECT_API_KEY_ID=\"YOUR_KEY_ID\""
    log_info "  APP_STORE_CONNECT_API_ISSUER_ID=\"YOUR_ISSUER_ID\""
    log_info "IPA 文件保留在: $IPA_PATH"
fi

# ==============================================================================
# 构建摘要
# ==============================================================================

log_step "构建摘要"

echo ""
echo "📦 应用名称: $PROJECT_NAME"
echo "🔢 版本号: $VERSION"
echo "🏗️  Build Number: $NEW_BUILD"
echo "📁 Archive: $ARCHIVE_PATH"
echo "📱 IPA: $IPA_PATH"
echo "💾 大小: $IPA_SIZE"
echo ""

log_success "构建并上传完成！"
log_info "请在 App Store Connect 中查看构建状态"
log_info "通常需要 5-15 分钟处理完成"

# ==============================================================================
# Git 标签（可选）
# ==============================================================================

if [ "$GIT_TAG" = "true" ]; then
    log_step "创建 Git 标签"

    TAG_NAME="v${VERSION}"
    git tag -a "$TAG_NAME" -m "Release $VERSION (Build $NEW_BUILD)"
    git push origin "$TAG_NAME"

    log_success "Git 标签已创建: $TAG_NAME"
fi

# ==============================================================================
# 完成
# ==============================================================================

echo ""
log_success "全部完成！"
echo ""
