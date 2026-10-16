# WP-B: Smart Address Bar (P1)

## B1. Search Engine Fallback

**Files:**
- `Actrium/Views/MainContentView/BrowserViewModel.swift` — update `parseURL()`

**Implementation:**
1. In `parseURL()`, when input is not a valid URL (no "." or has spaces):
   - Build search URL: `https://www.google.com/search?q=\(encoded)`
   - Use the configured search engine from UserDefaults
2. Search engine URL templates:
   - Google: `https://www.google.com/search?q=%s`
   - Bing: `https://www.bing.com/search?q=%s`
   - DuckDuckGo: `https://duckduckgo.com/?q=%s`

## B2. Configurable Default Search Engine (Settings)

**Files:**
- `Actrium/Settings/SearchEngineSettings.swift` (new)
- `Actrium/Settings/SettingsView.swift` — add General tab

**Implementation:**
1. Enum `SearchEngine: String, CaseIterable` with name, searchTemplate, icon
2. UserDefaults key: `"browser.defaultSearchEngine"`, default = .google
3. Settings UI: Picker in General/Privacy tab
4. BrowserViewModel reads setting in parseURL()
