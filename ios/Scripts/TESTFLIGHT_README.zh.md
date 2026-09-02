# TestFlight 自动上传脚本使用指南

## 🚀 快速开始

### 1. 首次使用配置

#### 方法一：使用 App Store Connect API Key（推荐）

1. 在 [App Store Connect](https://appstoreconnect.apple.com) 生成 API Key：
   - 用户和访问 → 密钥 → App Store Connect API
   - 创建新密钥，权限选择 "Developer"
   - 下载 `.p8` 文件并保存

2. 设置环境变量：
```bash
export APP_STORE_CONNECT_API_KEY_PATH="/path/to/AuthKey_XXXXXXXXXX.p8"
export APP_STORE_CONNECT_API_KEY_ID="XXXXXXXXXX"
export APP_STORE_CONNECT_API_ISSUER_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

3. 建议将环境变量添加到 `~/.zshrc` 或 `~/.bash_profile`：
```bash
echo 'export APP_STORE_CONNECT_API_KEY_PATH="/path/to/AuthKey_XXXXXXXXXX.p8"' >> ~/.zshrc
echo 'export APP_STORE_CONNECT_API_KEY_ID="XXXXXXXXXX"' >> ~/.zshrc
echo 'export APP_STORE_CONNECT_API_ISSUER_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"' >> ~/.zshrc
source ~/.zshrc
```

#### 方法二：使用 Apple ID（需要应用专用密码）

1. 生成应用专用密码：
   - 访问 [appleid.apple.com](https://appleid.apple.com)
   - 登录 → 安全 → 应用专用密码
   - 生成新密码

2. 保存到 Keychain：
```bash
xcrun altool --store-password-in-keychain-item "AC_PASSWORD" \
    -u "your-apple-id@example.com" \
    -p "xxxx-xxxx-xxxx-xxxx"
```

3. 设置环境变量：
```bash
export APPLE_ID="your-apple-id@example.com"
```

### 2. 修改配置文件

编辑 `Scripts/ExportOptions.plist`：
```xml
<key>teamID</key>
<string>YOUR_TEAM_ID</string>  <!-- 改为你的 Team ID -->

<key>provisioningProfiles</key>
<dict>
    <key>com.actrium.actrium</key>  <!-- 改为你的 Bundle ID -->
    <string>YOUR_PROVISIONING_PROFILE_NAME</string>  <!-- 改为你的 Provisioning Profile 名称 -->
</dict>
```

编辑 `Scripts/upload-testflight.sh` 的第 165 行：
```bash
DEVELOPMENT_TEAM="YOUR_TEAM_ID"  # 改为你的 Team ID
```

### 3. 确保项目配置正确

检查 `Actrium.xcodeproj/project.pbxproj` 中的版本号设置：
- Marketing Version (CFBundleShortVersionString)
- Current Project Version (CFBundleVersion)

### 4. 运行脚本

```bash
# 赋予执行权限
chmod +x Scripts/upload-testflight.sh

# 执行上传
./Scripts/upload-testflight.sh
```

## 📋 脚本功能

脚本会自动完成以下步骤：

1. ✅ 前置检查（Xcode、工具链、依赖）
2. 🧹 清理旧构建文件
3. 🔢 自动增加 Build Number
4. 📦 安装 React Native 依赖
5. 🏗️  构建 Archive
6. 📱 导出 IPA
7. ☁️  上传到 TestFlight
8. 📊 显示构建摘要

## 🎯 高级用法

### 创建 Git 标签

```bash
GIT_TAG=true ./Scripts/upload-testflight.sh
```

### 自定义配置

编辑脚本顶部的配置区域：

```bash
# 项目配置
PROJECT_NAME="Actrium"
WORKSPACE_PATH="Actrium.xcworkspace"
SCHEME="Actrium"
CONFIGURATION="Release"
```

### CI/CD 集成

在 GitHub Actions / GitLab CI / Jenkins 中使用：

```yaml
# .github/workflows/testflight.yml
name: Upload to TestFlight

on:
  push:
    tags:
      - 'v*'

jobs:
  deploy:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup environment
        run: |
          echo "APP_STORE_CONNECT_API_KEY_PATH=${{ secrets.API_KEY_PATH }}" >> $GITHUB_ENV
          echo "APP_STORE_CONNECT_API_KEY_ID=${{ secrets.API_KEY_ID }}" >> $GITHUB_ENV
          echo "APP_STORE_CONNECT_API_ISSUER_ID=${{ secrets.API_ISSUER_ID }}" >> $GITHUB_ENV
      
      - name: Upload to TestFlight
        run: ./Scripts/upload-testflight.sh
```

## 🔧 故障排除

### 问题：构建失败 "No signing certificate"

**解决方案：**
1. 打开 Xcode
2. Preferences → Accounts → 下载 Manual Profiles
3. 在项目设置中检查 Signing & Capabilities

### 问题：上传失败 "Authentication failed"

**解决方案：**
1. 检查 API Key 配置是否正确
2. 确认 API Key 有 "Developer" 权限
3. 检查 Apple ID 和应用专用密码是否正确

### 问题：Build Number 没有自动增加

**解决方案：**
1. 确保 `agvtool` 已安装
2. 检查项目设置中的 Versioning System 是否为 "Apple Generic"

### 问题：React Native bundle 错误

**解决方案：**
```bash
cd ActriumRuntime
rm -rf node_modules
bun install
```

## 📝 注意事项

1. **首次上传需要在 App Store Connect 中创建应用**
2. **确保 Bundle ID、Team ID、Provisioning Profile 匹配**
3. **上传后需要在 App Store Connect 中：**
   - 添加测试人员
   - 填写合规信息
   - 提交审核（如需外部测试）
4. **构建处理通常需要 5-15 分钟**
5. **建议在上传前先在本地测试 Archive**

## 🔐 安全建议

1. **不要将 API Key 文件提交到 Git**
2. **使用 `.gitignore` 排除敏感文件：**
   ```
   # .gitignore
   Scripts/AuthKey_*.p8
   Scripts/.env
   ```
3. **在 CI/CD 中使用 Secret 管理敏感信息**

## 📚 相关链接

- [App Store Connect API](https://developer.apple.com/documentation/appstoreconnectapi)
- [TestFlight 文档](https://developer.apple.com/testflight/)
- [Xcode Build Settings](https://developer.apple.com/documentation/xcode/build-settings-reference)
- [altool 文档](https://help.apple.com/itc/apploader/)

## 🆘 获取帮助

如果遇到问题：
1. 检查 `build/upload.log` 日志文件
2. 运行 `xcodebuild -showBuildSettings` 检查配置
3. 访问 App Store Connect 查看构建状态
