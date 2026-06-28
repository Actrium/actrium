# Multi-Process Rendering Debug Guide

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           主进程 (Actro.app)                                │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. BrowserViewModel.init()                                                   │
│    ├─ 检测 counterapp 开发目录                                               │
│    └─ Task { loadFromDevDirectory() }                                        │
│                                                                             │
│ 2. loadFromDevDirectory()                                                    │
│    ├─ 读取 unified-app.js                                                    │
│    └─ appendACTRTab() → tabs.append(newTab)                                  │
│                                                                             │
│ 3. UnifiedActroView 渲染                                                    │
│    └─ ActroView                                                             │
│        └─ ForEach(tabs) → TabCardView                                        │
│            └─ MultiProcessTabView                                            │
│                                                                             │
│ 4. MultiProcessTabView.onAppear                                              │
│    └─ startRenderer() → TabProcessManager.launchProcess()                    │
│                                                                             │
│ 5. 等待渲染进程返回 SURFACE_ID                                                │
│                                                                             │
│ 6. IOSurfaceDisplayView 显示 IOSurface                                       │
│    └─ CVDisplayLink 每帧调用 IOSurfaceLookup + 设置 layer.contents           │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                         stdin (JSON config)
                                    ↓
┌─────────────────────────────────────────────────────────────────────────────┐
│                        渲染进程 (ActroTabRenderer.app)                       │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. TabRendererAppDelegate.applicationDidFinishLaunching                      │
│    ├─ 解析命令行参数 (--tab-id, --width, --height, --background)              │
│    └─ 从 stdin 读取 JSON 配置                                                 │
│                                                                             │
│ 2. TabRendererService.loadContentDirect()                                    │
│    ├─ 创建 IOSurface (全局共享)                                               │
│    ├─ stdout 输出 "SURFACE_ID:xxx"                                           │
│    ├─ setupReactNative() → 创建 RCTRootView                                  │
│    └─ startDisplayLink()                                                     │
│                                                                             │
│ 3. CVDisplayLink 回调 (每帧)                                                  │
│    └─ renderFrame()                                                          │
│        ├─ 锁定 IOSurface                                                      │
│        ├─ 创建 CGContext                                                      │
│        ├─ 填充背景色                                                          │
│        ├─ rootView.layer.render(in: context) ← 渲染 React Native             │
│        └─ 解锁 IOSurface                                                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Key Diagnostic Points

### 1. Main Process - View Lifecycle
- `🔵 [DIAG] MultiProcessTabView.onAppear` - View appeared, starting renderer
- `🔴 [DIAG] MultiProcessTabView.onDisappear` - View disappeared (problematic if immediate)

### 2. Main Process - IOSurface Display
- `🖥️ [DIAG] refreshSurfaceContent` - Shows surfaceID, size, and center pixel value
- If centerPixel is just background color, content is not rendering

### 3. Renderer Process - React Native Setup
- `📱 [DIAG] setupReactNative` - Shows config and setup progress
- `📱 [DIAG] RCTContentDidAppearNotification` - React content is ready (important!)
- `📱 [DIAG] RCTJavaScriptDidLoadNotification` - JS bundle loaded
- `📱 [DIAG] RCTJavaScriptDidFailToLoadNotification` - JS loading failed

### 4. Renderer Process - Frame Rendering
- `🎨 [DIAG] renderFrame` - Shows rootView info, subviews, sublayers
- `🎨 [DIAG] Center pixel value` - Shows if anything is rendered

## Known Issues & Attempted Fixes

### Issue 1: View Destroyed Immediately After Appearing
**Symptom**: `onAppear` followed immediately by `onDisappear`
**Cause**: SwiftUI view recreation when `@ObservedObject` properties change
**Attempted Fix**:
- Removed `@ObservedObject var viewModel` from `ActroView` and `TabCardView`
- Added 1 second delay before switching to focus mode

### Issue 2: IOSurface Shows Only Background Color
**Symptom**: Black or single-color display, no React Native content
**Possible Causes**:
1. React Native view not rendering to offscreen window
2. `CALayer.render(in:)` not capturing Metal content
3. React Native content not ready when rendering starts

### Issue 3: Memory Leak (5GB+)
**Symptom**: Renderer process consuming 5GB+ memory, 99% CPU
**Cause**: Creating new `NSBitmapImageRep` every frame
**Fix**: Switched to `CALayer.render(in:)` method

## Debugging Checklist

- [ ] Check if `RCTContentDidAppearNotification` is received
- [ ] Check `rootView.subviews.count` - should be > 0 after content loads
- [ ] Check center pixel value - should not be just background color
- [ ] Verify Metro bundler is running and accessible
- [ ] Check if JS bundle loads successfully

## Files Involved

### Main Process
- `Actro/Views/UnifiedActroView.swift` - Main actro view
- `Actro/Views/MainContentView/BrowserViewModel.swift` - Tab management
- `Actro/MultiProcess/IOSurfaceHostView.swift` - IOSurface display
- `Actro/MultiProcess/TabProcessManager.swift` - Process management

### Renderer Process
- `ActroTabRenderer/TabRendererAppDelegate.swift` - App entry point
- `ActroTabRenderer/TabRendererService.swift` - Rendering logic
- `ActroTabRenderer/RendererReactNativeHost.swift` - React Native setup
