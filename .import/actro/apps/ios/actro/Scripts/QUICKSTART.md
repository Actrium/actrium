# 🚀 TestFlight 快速上传指南

## 一分钟开始

### 1. 检查配置
```bash
./Scripts/check-config.sh
```

### 2. 配置认证（选择一种方式）

#### 方式 A：App Store Connect API Key（推荐）
```bash
# 在 App Store Connect 创建 API Key 并下载 .p8 文件后
export APP_STORE_CONNECT_API_KEY_PATH="/path/to/AuthKey_XXXXXXXXXX.p8"
export APP_STORE_CONNECT_API_KEY_ID="XXXXXXXXXX"
export APP_STORE_CONNECT_API_ISSUER_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

#### 方式 B：Apple ID
```bash
# 生成应用专用密码后
xcrun altool --store-password-in-keychain-item "AC_PASSWORD" \
    -u "your-apple-id@example.com" \
    -p "xxxx-xxxx-xxxx-xxxx"

export APPLE_ID="your-apple-id@example.com"
```

### 3. 修改配置文件

编辑 `Scripts/ExportOptions.plist`，替换：
- `YOUR_TEAM_ID` → 你的 Team ID
- `com.yourcompany.canvas` → 你的 Bundle ID

编辑 `Scripts/upload-testflight.sh` 第 165 行：
- `DEVELOPMENT_TEAM="YOUR_TEAM_ID"` → 你的 Team ID

### 4. 测试构建（可选但推荐）
```bash
./Scripts/test-build.sh
```

### 5. 上传到 TestFlight
```bash
./Scripts/upload-testflight.sh
```

## 📁 文件说明

| 文件 | 说明 |
|------|------|
| `upload-testflight.sh` | 主上传脚本 |
| `test-build.sh` | 本地测试构建（不上传） |
| `check-config.sh` | 配置检查工具 |
| `ExportOptions.plist` | IPA 导出配置 |
| `.env.example` | 环境变量模板 |
| `TESTFLIGHT_README.md` | 详细文档 |

## 🔧 常见问题

### Q: 如何获取 Team ID？
A: 打开 Xcode → 项目设置 → Signing & Capabilities → Team 下方显示的括号内 ID

### Q: 上传需要多久？
A: 构建通常 5-10 分钟，上传 2-5 分钟，App Store Connect 处理 5-15 分钟

### Q: 如何查看上传进度？
A: 登录 [App Store Connect](https://appstoreconnect.apple.com) → 我的 App → TestFlight

### Q: bun 是什么？
A: bun 是一个快速的 JavaScript 运行时和包管理器，用于管理 React Native 依赖。脚本会自动安装。

### Q: 首次上传需要什么？
A: 
1. 在 App Store Connect 创建应用
2. 配置 Bundle ID
3. 设置应用图标和必填信息
4. 添加测试人员

### Q: 遇到签名错误怎么办？
A: 
1. 打开 Xcode
2. Preferences → Accounts → 下载 Manual Profiles
3. 项目设置 → Signing & Capabilities 检查配置

## 📚 获取更多帮助

- 📖 详细文档：`Scripts/TESTFLIGHT_README.md`
- 🔐 API Key 配置：[App Store Connect API](https://developer.apple.com/documentation/appstoreconnectapi)
- 📱 TestFlight 指南：[TestFlight 文档](https://developer.apple.com/testflight/)

## ⚡ 快捷命令

```bash
# 检查配置
./Scripts/check-config.sh

# 测试构建
./Scripts/test-build.sh

# 上传（带 Git 标签）
GIT_TAG=true ./Scripts/upload-testflight.sh

# 查看日志
cat build/upload.log

# 清理构建
rm -rf build
```
