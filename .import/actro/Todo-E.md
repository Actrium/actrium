# WP-E: Ad Blocking (P2)

## E1. Content Rule List Engine

**Files:**
- `Canvas/ContentBlocking/ContentBlockerManager.swift` (new)
- `Canvas/ContentBlocking/easylist-lite.json` (bundled)
- `Canvas/MultiProcess/WebViewTabView.swift` — apply rules to WKWebViewConfiguration

**Implementation:**
1. Bundle a lightweight ad-blocking rule list (EasyList-lite, ~2000 rules)
2. Convert to WebKit content blocker JSON format:
   ```json
   [{"trigger":{"url-filter":"..."},"action":{"type":"block"}}]
   ```
3. `ContentBlockerManager.shared`:
   - `loadRules()` → `WKContentRuleListStore.default().compileContentRuleList()`
   - Cache compiled rules
   - `apply(to config: WKWebViewConfiguration)` → `config.userContentController.add(ruleList)`
4. Apply in WebViewHolder init before creating WKWebView

## E2. Settings UI

**Files:**
- `Canvas/Settings/ContentBlockingSettingsView.swift` (new)
- `Canvas/Settings/SettingsView.swift` — add tab

**Implementation:**
1. Master toggle: enable/disable ad blocking
2. Stats: pages blocked count
3. Whitelist: per-domain disable
4. Custom rules text area (advanced)
