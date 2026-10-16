# Multi-Process Rendering Debug Guide

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Main Process (Actrium.app)                                │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. BrowserViewModel receives an .actr package                                │
│    ├─ Verify manifest, signature, and payload digests                        │
│    └─ Create a runtime tab from the declared workload                        │
│                                                                             │
│ 2. Package loading                                                          │
│    ├─ Read the workload entry declared in manifest.toml                      │
│    └─ appendRuntimeTab() → tabs.append(newTab)                               │
│                                                                             │
│ 3. UnifiedActriumView Render                                                    │
│    └─ ActriumView                                                             │
│        └─ ForEach(tabs) → TabCardView                                        │
│            └─ MultiProcessTabView                                            │
│                                                                             │
│ 4. MultiProcessTabView.onAppear                                              │
│    └─ startRenderer() → TabProcessManager.launchProcess()                    │
│                                                                             │
│ 5. Waiting for rendering process to return SURFACE_ID                                                │
│                                                                             │
│ 6. IOSurfaceDisplayView Show IOSurface                                       │
│    └─ CVDisplayLink Call per frame IOSurfaceLookup + Settings layer.contents           │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                         stdin (JSON config)
                                    ↓
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Rendering Process (ActriumTabRenderer.app)                       │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. TabRendererAppDelegate.applicationDidFinishLaunching                      │
│    ├─ Parsing command line parameters (--tab-id, --width, --height, --background)              │
│    └─ From stdin Read JSON Configure                                                 │
│                                                                             │
│ 2. TabRendererService.loadContentDirect()                                    │
│    ├─ Create IOSurface (Global share)                                               │
│    ├─ stdout Output "SURFACE_ID:xxx"                                           │
│    ├─ setupReactNative() → Create RCTRootView                                  │
│    └─ startDisplayLink()                                                     │
│                                                                             │
│ 3. CVDisplayLink Rewind (Every frame)                                                  │
│    └─ renderFrame()                                                          │
│        ├─ Lock IOSurface                                                      │
│        ├─ Create CGContext                                                      │
│        ├─ Fill background colour                                                          │
│        ├─ rootView.layer.render(in: context) ← Render React Native             │
│        └─ Unlock IOSurface                                                      │
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
- Removed `@ObservedObject var viewModel` from `ActriumView` and `TabCardView`
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
- `Actrium/Views/UnifiedActriumView.swift` - Main actrium view
- `Actrium/Views/MainContentView/BrowserViewModel.swift` - Tab management
- `Actrium/MultiProcess/IOSurfaceHostView.swift` - IOSurface display
- `Actrium/MultiProcess/TabProcessManager.swift` - Process management

### Renderer Process
- `ActriumTabRenderer/TabRendererAppDelegate.swift` - App entry point
- `ActriumTabRenderer/TabRendererService.swift` - Rendering logic
- `ActriumTabRenderer/RendererReactNativeHost.swift` - React Native setup
