# WP-A: Navigation & Progress (P0)

## A1. Stop Loading

**Files:**
- `Canvas/MultiProcess/WebViewTabView.swift` — WebViewHolder add `stopLoading()`
- `Canvas/Views/MainContentView/BrowserViewModel.swift` — add `stopCurrentTab()`
- `Canvas/Views/MainToolbar.swift` — reload button toggles to stop (X) when loading

**Implementation:**
1. WebViewHolder: `func stopLoading() { webView.stopLoading() }`
2. BrowserViewModel: delegate to current holder's stopLoading()
3. Toolbar: when `isLoading`, show X icon (xmark) instead of reload (arrow.clockwise)
4. Callback: ToolbarManager.onReload checks isLoading → stop or reload

## A2. Loading Progress Bar (Safari-style)

**Files:**
- `Canvas/MultiProcess/WebViewTabView.swift` — observe `estimatedProgress` via KVO
- `Canvas/Views/MainContentView/BrowserViewModel.swift` — publish `loadingProgress: Double`
- `Canvas/Views/WorkspaceMainView.swift` — thin progress bar overlay at top of web content

**Implementation:**
1. WebViewHolder: add `progressObservation: NSKeyValueObservation?`
   - Observe `webView.estimatedProgress`
   - Callback: `onProgressChanged: ((Double) -> Void)?`
2. BrowserViewModel: `@Published var loadingProgress: Double = 0`
   - Wire up to current tab's holder progress callback
3. WorkspaceMainView: add thin (2pt) colored bar at the very top of web content area
   - Width = `loadingProgress * containerWidth`
   - Animate with `.animation(.linear)`
   - Hide when progress >= 1.0 (with fade delay)
   - Use accent color (blue like Safari)
