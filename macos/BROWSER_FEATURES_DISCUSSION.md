# Actrium Browser function discussion

## 1. Sidebar Favicon ✅ Achieved

### Achieving the programme
- Web tabs Use Google Favicon Service Load Icon
- Use actor-based `FaviconCache` Performing a thread safe memory cache
- Not Web tabs Show Default doc.text Icon

### Code Position
- `TabOutlineView.swift:makeTabRowView()`
- `FaviconCache` actor

---

## 2. Tab Overview Pageshot ✅ Achieved

### Achieving the programme
- Use `WKWebView.takeSnapshot()` API Capture pageshot
- Screenshot Slows In `WebViewHolder.snapshot` Properties
- Trigger time:
  - From Focus Switch to Overview Time
  - After Page Loading Completed (Delay) 0.5s Waiting for Rendering)
- Overview Show caches in mode, placeholders when no cutshots

### Code Position
- `WebViewTabView.swift:WebViewHolder.takeSnapshot()`
- `WebViewTabView.swift:WebViewMultiProcessTabView`
- `WorkspaceMainView.swift:onChange(of: viewModel.actriumMode)`

---

## 3. Download function ✅ Achieved (Safari Same paragraph)

### Achieving the programme
- Achieved `DownloadManager` Centralize all downloads
- Achieved `DownloadItem` Track progress and status of individual downloads
- `DownloadPopoverView` Safari Down-Stand Panel
- `DownloadToolbarButton` Toolbar button (indicator when active download is available)
- Conditions for testing:
  - `Content-Disposition: attachment` Response Header
  - Link with `download` Properties
  - Unrenderable MIME Type (zip, dmg, etc.)
- Download Save To `~/Downloads`,Auto-processing of a rename file
- Double-click to open file to support the Finder Show

### Code Position
- `Downloads/DownloadManager.swift`
- `Downloads/DownloadPopoverView.swift`
- `WebViewTabView.swift:WebViewHolder`

---

## 4. Search function (Cmd+F) ✅ Achieved

### Achieving the programme
- Use `WKWebView.find(_:configuration:)` API
- Only Focus Under Mode Web Tabs Available
- Shortcut:
  - `Cmd+F`: Open/Close the Search Bar
  - `Escape`: Close the Search Bar
  - `Enter`: Find Next
- Real time search (by entering and searching)
- Previous/Next Navigation Button

### Code Position
- `WorkspaceMainView.swift:FindInPageBar`
- `WorkspaceMainView.swift:performFind()`

---

## 5. Print function ✅ Achieved

### Achieving the programme
- Use `WKWebView.printOperation()` API
- Shortcut Keys: `Cmd+P`
- Native Print Dialogue

### Code Position
- `WebViewTabView.swift:WebViewHolder.print()`
- `ActriumApp.swift:printCurrentPage()`

---

## 6. Scale function ✅ Achieved

### Achieving the programme
- Use `WKWebView.pageZoom` Properties
- Shortcut:
  - `Cmd++`: Zoom in. (Max 300%)
  - `Cmd+-`: Zoom Out (Min 50%)
  - `Cmd+0`: Actual Size (100%)

### Code Position
- `WebViewTabView.swift:WebViewHolder.zoomIn/zoomOut/resetZoom()`
- `ActriumApp.swift:zoomIn/zoomOut/resetZoom()`

---

## 7. Full Screen Mode ✅ Achieved

### Achieving the programme
- Use `NSWindow.toggleFullScreen()`
- Shortcut Keys: `Ctrl+Cmd+F`

### Code Position
- `ActriumApp.swift:toggleFullScreen()`

---

## 8. Browser Functional Integrity Assessment

### Achieved functionality ✅
- Multi-Tapp Management
- Forward!/Backward Navigator
- Address Bar Input
- Bookmarks
- History
- `target="_blank"` New Tab
- Custom User-Agent
- Developer tool (DevTools)
- **Sidebar Favicon(Cache)**
- **Tab Overview Pageshot**
- **Download Manager (%1)Safari Styles)**
- **Search function (Cmd+F)**
- **Print function (Cmd+P)**
- **Scale function (Cmd+/Cmd-/Cmd+0)**
- **Full Screen Mode (Ctrl+Cmd+F)**

### Functions to be achieved
| Functions | Priority | Complexity |
|------|--------|--------|
| Cookie Management | Medium | Medium |
| Password Management | Low | High |
| Extension System | Low | High |
| Read Mode | Low | Medium |

---

## 9. Multi-process model comparison

### Current Actrium Achieved
```
Actrium App (Main Process)
    └── WKWebView
            └── WebContent Process (By WebKit Management)
                    └── Share WKProcessPool
```

- Use `WKProcessPool` Shared Process Pool
- WebKit AutoManaging WebContent Process
- The sandbox is isolated by the system.

### Safari Multi-process architecture
```
Safari (UI Process)
    ├── WebContent Process 1 (Tab 1)
    ├── WebContent Process 2 (Tab 2)
    ├── Networking Process
    └── GPU Process
```

Safari Special features:
- Independent process per tab (site isolated)
- Specialized network processes
- GPU Process Separating
- A more nuanced sandbox.

### Conclusions
Actrium What's used is **WKWebView Standard multi-process model**with Safari ♪ And the bottom of the floor ♪ WebKit) but:
- Safari There's a more stringent site quarantine strategy.
- Safari There's an independent network./GPU Process
- Actrium The process management is entirely by WebKit Frame processing, yes"Standard Safari Same paragraph"The simplified version of the new version
