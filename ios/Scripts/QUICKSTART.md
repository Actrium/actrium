# 🚀 TestFlight Quick Upload Guide

## One minute to start.

### 1. Check Configuration
```bash
./Scripts/check-config.sh
```

### 2. Configure authentication (select one way)

#### Modalities A: App Store Connect API Key(Recommended)
```bash
# Yes. App Store Connect Create API Key , and download .p8 After file
export APP_STORE_CONNECT_API_KEY_PATH="/path/to/AuthKey_XXXXXXXXXX.p8"
export APP_STORE_CONNECT_API_KEY_ID="XXXXXXXXXX"
export APP_STORE_CONNECT_API_ISSUER_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

#### Modalities B: Apple ID
```bash
# After generating the application of a specific password
xcrun altool --store-password-in-keychain-item "AC_PASSWORD" \
    -u "your-apple-id@example.com" \
    -p "xxxx-xxxx-xxxx-xxxx"

export APPLE_ID="your-apple-id@example.com"
```

### 3. Modify Profile

Edit `Scripts/ExportOptions.plist`, replace:
- `YOUR_TEAM_ID` → Yours. Team ID
- `com.example.actrium` → Yours. Bundle ID

Edit `Scripts/upload-testflight.sh` I'm not sure. 165 Line:
- `DEVELOPMENT_TEAM="YOUR_TEAM_ID"` → Yours. Team ID

### 4. Test Build (optional but recommended)
```bash
./Scripts/test-build.sh
```

### 5. Upload to TestFlight
```bash
./Scripts/upload-testflight.sh
```

## 📁 Annotations

| Documentation | Annotations |
|------|------|
| `upload-testflight.sh` | Main Upload Script |
| `test-build.sh` | Local Test Build (Unallowed) |
| `check-config.sh` | Configure check tool |
| `ExportOptions.plist` | IPA Export Configuration |
| `.env.example` | Environment Variable Template |
| `TESTFLIGHT_README.md` | Detailed documents |

## 🔧 Common problems

### Q: How to get Team ID?
A: Open Xcode → Project Settings → Signing & Capabilities → Team The brackets below ID

### Q: How long does it take to upload?
A: Build normal 5-10 Minutes, upload 2-5 Minutes,App Store Connect Process 5-15 min

### Q: How do you view upload progress?
A: Login [App Store Connect](https://appstoreconnect.apple.com) → Mine. App → TestFlight

### Q: bun What is it?
A: bun It's a fast one. JavaScript Runtime and Package Manager for Management React Native Dependence. Scripts are automatically installed.

### Q: What does it take to upload it for the first time?
A:
1. Yes. App Store Connect Create Application
2. Configure Bundle ID
3. Setup application icons and mandatory information
4. Add Tester

### Q: What if you get a signature error?
A:
1. Open Xcode
2. Preferences → Accounts → Download Manual Profiles
3. Project Settings → Signing & Capabilities Check Configuration

## 📚 Get More Help

- 📖 Detailed documents:`Scripts/TESTFLIGHT_README.md`
- 🔐 API Key Configure:[App Store Connect API](https://developer.apple.com/documentation/appstoreconnectapi)
- 📱 TestFlight Guide:[TestFlight Document](https://developer.apple.com/testflight/)

## ⚡ Shortcut Command

```bash
# Check Configuration
./Scripts/check-config.sh

# Test Build
./Scripts/test-build.sh

# Upload (band) Git label)
GIT_TAG=true ./Scripts/upload-testflight.sh

# View Log
cat build/upload.log

# Clear Build
rm -rf build
```
