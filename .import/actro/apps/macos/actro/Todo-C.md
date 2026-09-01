# WP-C: Permission Requests (P1)

## C1. Camera/Microphone Permission

**Files:**
- `Actro/MultiProcess/WebViewTabView.swift` — WebViewHolder (WKUIDelegate)

**Implementation:**
1. Implement `webView(_:requestMediaCapturePermissionFor:initiatedByFrame:type:decisionHandler:)`
2. Show NSAlert with domain name and requested permission type
3. Options: Allow / Deny
4. Call decisionHandler(.grant) or (.deny)

## C2. Geolocation Permission

**Files:**
- `Actro/MultiProcess/WebViewTabView.swift` — WebViewHolder (WKUIDelegate)

**Implementation:**
1. Implement `webView(_:requestDeviceOrientationAndMotionPermissionFor:initiatedByFrame:decisionHandler:)`
2. For geolocation, WKWebView on macOS doesn't have a direct delegate - need JS bridge or CLLocationManager
3. Fallback: grant by default (macOS handles system-level permission)

## C3. Permission Persistence

**Files:**
- `Actro/Settings/PermissionStore.swift` (new)

**Implementation:**
1. Dictionary `[String: [PermissionType: Bool]]` keyed by domain
2. Persist in UserDefaults
3. Check stored decision before showing alert
4. "Remember for this site" checkbox in permission dialog
