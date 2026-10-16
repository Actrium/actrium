# TestFlight Guide to auto-upload scripts

## 🚀 Quick Start

### 1. Use configuration for first time

#### Method I: Use App Store Connect API Key(Recommended)

1. Yes. [App Store Connect](https://appstoreconnect.apple.com) Generate API Key:
   - Users and Accesses → Key → App Store Connect API
   - Create new key, permissions selection "Developer"
   - Download `.p8` File and Save

2. Sets the environment variable:
```bash
export APP_STORE_CONNECT_API_KEY_PATH="/path/to/AuthKey_XXXXXXXXXX.p8"
export APP_STORE_CONNECT_API_KEY_ID="XXXXXXXXXX"
export APP_STORE_CONNECT_API_ISSUER_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

3. Suggests that environment variables be added to `~/.zshrc` or `~/.bash_profile`:
```bash
echo 'export APP_STORE_CONNECT_API_KEY_PATH="/path/to/AuthKey_XXXXXXXXXX.p8"' >> ~/.zshrc
echo 'export APP_STORE_CONNECT_API_KEY_ID="XXXXXXXXXX"' >> ~/.zshrc
echo 'export APP_STORE_CONNECT_API_ISSUER_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"' >> ~/.zshrc
source ~/.zshrc
```

#### Method II: Use Apple ID(needs to apply a specific password)

1. Generates the application of a specific password:
   - Visits [appleid.apple.com](https://appleid.apple.com)
   - Login → Clear. → Apply a specific password
   - Generate New Password

2. Save To Keychain:
```bash
xcrun altool --store-password-in-keychain-item "AC_PASSWORD" \
    -u "your-apple-id@example.com" \
    -p "xxxx-xxxx-xxxx-xxxx"
```

3. Sets the environment variable:
```bash
export APPLE_ID="your-apple-id@example.com"
```

### 2. Modify Profile

Edit `Scripts/ExportOptions.plist`:
```xml
<key>teamID</key>
<string>YOUR_TEAM_ID</string>  <!-- It's yours. Team ID -->

<key>provisioningProfiles</key>
<dict>
    <key>com.actrium.actro</key>  <!-- It's yours. Bundle ID -->
    <string>YOUR_PROVISIONING_PROFILE_NAME</string>  <!-- It's yours. Provisioning Profile Name -->
</dict>
```

Edit `Scripts/upload-testflight.sh` No. No. 165 Line:
```bash
DEVELOPMENT_TEAM="YOUR_TEAM_ID"  # It's yours. Team ID
```

### 3. Ensure that the project is configured correctly

Inspection `Actrium.xcodeproj/project.pbxproj` , and the version number settings:
- Marketing Version (CFBundleShortVersionString)
- Current Project Version (CFBundleVersion)

### 4. Run Script

```bash
# Grant enforcement authority
chmod +x Scripts/upload-testflight.sh

# Execute Upload
./Scripts/upload-testflight.sh
```

## 📋 Script function

The script will automatically complete the following steps:

1. ✅ PrecheckXcode, tool chains, dependence)
2. 🧹 Clear old build file
3. 🔢 Automatic Increase Build Number
4. 📦 Install React Native Dependency
5. 🏗️  Build Archive
6. 📱 Export IPA
7. ☁️  Upload to TestFlight
8. 📊 Show Build Summary

## 🎯 Advanced Usage

### Create Git Tab

```bash
GIT_TAG=true ./Scripts/upload-testflight.sh
```

### Custom Configuration

Edit configuration area at top of script:

```bash
# Project Configuration
PROJECT_NAME="Actrium"
WORKSPACE_PATH="Actrium.xcworkspace"
SCHEME="Actrium"
CONFIGURATION="Release"
```

### CI/CD Integration

Yes. GitHub Actions / GitLab CI / Jenkins Use of:

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

## 🔧 Disorder clearance.

### Problem: Build failure "No signing certificate"

**Solutions:**
1. Open Xcode
2. Preferences → Accounts → Download Manual Profiles
3. Check in project settings Signing & Capabilities

### Problem: Upload failed "Authentication failed"

**Solutions:**
1. Inspection API Key Is the configuration correct?
2. Confirm. API Key Yes. "Developer" Permissions
3. Inspection Apple ID Is it correct to apply a specific password?

### Question:Build Number No automatic increase

**Solutions:**
1. Ensure `agvtool` Installed
2. Checking for items settings Versioning System Is it "Apple Generic"

### Question:React Native bundle Error

**Solutions:**
```bash
cd ActriumRuntime
rm -rf node_modules
bun install
```

## 📝 Attention

1. **First upload needs App Store Connect Create application**
2. **Ensure Bundle ID, Team ID, Provisioning Profile Match**
3. **Needs after upload App Store Connect of:**
   - Add Tester
   - Fill in compliance information
   - Submitted for review (if required for external testing)
4. **Build processing usually requires 5-15 min**
5. **It's recommended that we test locally before uploading. Archive**

## 🔐 Security recommendations

1. **Don't put API Key File Submitted to Git**
2. **Use `.gitignore` Exclude sensitive documents:**
   ```
   # .gitignore
   Scripts/AuthKey_*.p8
   Scripts/.env
   ```
3. **Yes. CI/CD Use Secret Manage sensitive information**

## 📚 Related Links

- [App Store Connect API](https://developer.apple.com/documentation/appstoreconnectapi)
- [TestFlight Document](https://developer.apple.com/testflight/)
- [Xcode Build Settings](https://developer.apple.com/documentation/xcode/build-settings-reference)
- [altool Document](https://help.apple.com/itc/apploader/)

## 🆘 Get help.

If there is a problem:
1. Inspection `build/upload.log` Log File
2. Run `xcodebuild -showBuildSettings` Check Configuration
3. Visits App Store Connect View Build Status
