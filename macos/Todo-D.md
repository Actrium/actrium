# WP-D: Settings Enhancements (P2)

## D1. Cache/Cookies Clearing

**Files:**
- `Actrium/Settings/PrivacySettingsView.swift` (new)
- `Actrium/Settings/SettingsView.swift` — add Privacy tab

**Implementation:**
1. Use `WKWebsiteDataStore.default().removeData(ofTypes:modifiedSince:)`
2. Data types: cookies, cache, localStorage, indexedDB
3. UI: "Clear Browsing Data" button with checkboxes for each type
4. Optional date range: last hour / last day / all time

## D2. User-Agent Switching

**Files:**
- `Actrium/Settings/PrivacySettingsView.swift`
- `Actrium/MultiProcess/WebViewTabView.swift`

**Implementation:**
1. Presets: Actrium (default), Safari, Chrome, Firefox
2. Store in UserDefaults: `"browser.userAgent"`
3. Apply via `webView.customUserAgent` on creation and setting change
4. Settings UI: Dropdown picker

## D3. General Settings Tab

**Files:**
- `Actrium/Settings/GeneralSettingsView.swift` (new)

**Implementation:**
1. Default search engine picker (from WP-B)
2. Homepage URL field
3. New tab behavior (blank / homepage)
4. Default download location
