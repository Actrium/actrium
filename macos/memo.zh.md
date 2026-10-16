# ViewBridge Scale Implementation Memo

## Current Git State
- **Commit**: `585c74ba` - Fix ViewBridge scale to use viewport size with actrium compensation
- **Date**: 2026-01-10

## Problem
Single-layer actrium approach with `scaleEffect` doesn't work for ViewBridge XPC:
- SwiftUI `scaleEffect` does NOT affect NSViewRepresentable content
- CALayer transform on NSRemoteView has issues with bounds/frame mismatch
- Remote view bounds (1280x720) differs from requested containerSize (988x838)

## New Architecture: Dual-Layer ZStack

### Layers
1. **Real Layer**: Fixed at WorkspaceMainViewPort size, directly hosts RemoteView
2. **Scale Layer**: Handles scaling animation, uses screenshots during transitions

### Transition: Focus -> Overview
1. Capture screenshot (S) of current Real layer
2. Place S in Scale layer's Tab Card
3. Set Real layer zIndex below Scale layer
4. Animate Scale layer shrinking to card size
5. Replace screenshot with live scaled RemoteView thumbnail

### Transition: Overview -> Focus
1. Capture screenshot (S) of current Scale layer card
2. Scale layer (with S) starts expanding from card size
3. Animate to full viewport size
4. Set Real layer zIndex above Scale layer
5. Real layer becomes visible with live content

### Benefits
- RemoteView always renders at stable viewport size (no resize during transitions)
- Smooth animations using SwiftUI native scaleEffect on screenshots
- Clean separation of concerns: Real layer = content, Scale layer = animation

---

## Detailed Implementation Design

### Layer Structure
```swift
ZStack {
    // Scale Layer - for Overview mode and transition animations
    ScaleLayer(screenshot: $transitionSnapshot, scale: scaleLayerScale, offset: scaleLayerOffset)
        .zIndex(isFocused ? 0 : 1)

    // Real Layer - for Focus mode, always at viewport size
    RealLayer(tab: tab, viewportSize: viewportSize)
        .zIndex(isFocused ? 1 : 0)
        .opacity(realLayerOpacity)
}
```

### Focus -> Overview Transition
```
t=0ms:   [Trigger] User clicks Overview button
         - Capture Real layer screenshot -> transitionSnapshot
         - Set Scale layer content = screenshot, scale = 1.0, position = center
         - Real layer opacity = 1.0 (still visible behind)

t=0-300ms: [Animation]
         - Scale layer: scale 1.0 -> cardScale, position center -> card slot
         - Real layer: opacity 1.0 -> 0.0

t=300ms: [Complete]
         - Scale layer shows scaled screenshot at card position
         - Real layer hidden but still rendering (for quick Focus return)
         - Optional: periodically update Scale layer screenshot
```

### Overview -> Focus Transition
```
t=0ms:   [Trigger] User clicks Tab Card
         - Scale layer already showing screenshot at card position
         - Real layer hidden (opacity = 0)

t=0-300ms: [Animation]
         - Scale layer: scale cardScale -> 1.0, position card slot -> center
         - Real layer: stays hidden during animation

t=300ms: [Complete]
         - Cross-fade: Scale layer opacity 1.0 -> 0.0, Real layer opacity 0.0 -> 1.0
         - Real layer now visible with live content
```

### Key Components

#### 1. ViewSnapshot Extension
```swift
extension NSView {
    func snapshot() -> NSImage? {
        guard let rep = bitmapImageRepForCachingDisplay(in: bounds) else { return nil }
        cacheDisplay(in: bounds, to: rep)
        let image = NSImage(size: bounds.size)
        image.addRepresentation(rep)
        return image
    }
}
```

#### 2. ViewBridgeHostView - Add snapshot capability
- Expose method to capture current frame
- Called before transition starts

#### 3. TransitionState
```swift
enum TransitionState {
    case idle
    case focusToOverview(progress: CGFloat)
    case overviewToFocus(progress: CGFloat)
}
```

#### 4. ScaleLayerView
- Displays NSImage with scaleEffect
- Positioned using offset from viewport center to card slot

### Animation Curves
- Use `.spring(response: 0.3, dampingFraction: 0.8)` for natural feel
- Or `.easeInOut(duration: 0.25)` for snappy feel

---

## Implementation Status (2026-01-10)

### Completed
1. **ViewBridgeHostView.captureSnapshot()** - Uses NSRemoteView's private snapshot API
2. **ViewBridgeCoordinator.cachedSnapshot** - Stores snapshot for Scale Layer
3. **ViewBridgeCoordinatorManager.shared** - Singleton for global access
4. **FocusedReactNativeOverlay** - Real Layer component (actrium 外全屏)
5. **ViewBridgeThumbnailView** - Scale Layer component (显示截图)
6. **WorkspaceMainView integration** - Real Layer 在 actrium 变换之外
7. **TabPanel integration** - Scale Layer 在 actrium 内部
8. **Snapshot capture timing** - Focus -> Overview 切换时拍摄

### Removed
- displayScale property (no longer needed with dual-layer architecture)
- CALayer transform approach (replaced by screenshot + scaleEffect)
