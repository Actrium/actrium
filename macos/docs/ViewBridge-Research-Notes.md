# ViewBridge.framework Research notes

## 1. Key reference items

### 1.1 avaidyam/RemoteViewHost (⭐⭐⭐⭐⭐ I'll see.)
- **Warehouse**: https://github.com/avaidyam/RemoteViewHost
- **Description**: Use ViewBridge.framework Will GUI Code separated to different services PoC
- **Languages**: Objective-C (93.5%) + Swift (6.5%)
- **Core structure**:
  - `RemoteViewHost/` - Lord Host Apply
  - `RemoteViewService/` - XPC Service Component
  - `ViewBridgePublic/` - Public ViewBridge Header File

### 1.2 avaidyam Blog posts
- **URL**: https://medium.com/@avaidyam/viewbridge-framework-it-works-484de6f31f7d
- **Key information**:
  - Create XPC service + Add a few Info.plist Key + Get header file to work
  - Service View Load Time < 300ms
  - Could define custom NSExtensionSDK To make it..."Expandable"(Similar) Today Widgets)

### 1.3 macOS Headers (Private header file)
- **Warehouse**: https://github.com/w0lfschild/macOS_headers
- **Critical Path**: `macOS/PrivateFrameworks/ViewBridge/383/`
- **Important documents**:
  - `NSRemoteView.h`
  - `NSRemoteViewController.h`
  - `NSRemoteViewDelegate-Protocol.h`
  - `NSRemoteViewMarshal.h`

### 1.4 WebKit (Grand Master's rank.)
- **Warehouse**: https://github.com/WebKit/WebKit
- **Critical Path**: `Source/WebKit/UIProcess/Cocoa/`
- **Relevant documents**: `WKWebView.mm`, `WKWebViewIOS.mm`

---

## 2. NSRemoteView Key interfaces

### 2.1 Category definitions
```objc
@interface NSRemoteView : NSView <
    NSXPCListenerDelegate,
    NSVBXPCConnectionClient,
    HasAuxiliaryConnection,
    AuxiliaryCallsHost,
    _NSAlertSensitiveView
>
```

### 2.2 Resize Relevant methodology
```objc
// Service Request resize(Plussing and completing the callback)
- (void)serviceRequestsResize:(struct CGSize)arg1
    serviceWindowBackgroundColor:(id)arg2
    safeFrame:(struct CGRect)arg3
    animate:(BOOL)arg4
    transaction:(id)arg5
    completion:(CDUnknownBlockType)arg6;

// Simplified version
- (void)_serviceRequestsResize:(struct CGSize)arg1
    animate:(BOOL)arg2
    completion:(CDUnknownBlockType)arg3;

- (void)_serviceRequestsResize:(struct CGSize)arg1
    completion:(CDUnknownBlockType)arg2;
```

### 2.3 Key Properties
- `preferredContentSize` - Expected content dimensions
- `allowsImplicitResizeRequests` - Whether to allow hidden resize Request

---

## 3. WebKit It's... Resize Processing policy (⭐⭐⭐⭐⭐ Core)

### 3.1 No trust automatically Resize
WebKit **Never trust.** ViewBridge Auto Resize.

### 3.2 Official practice
1. **Host End (UIProcess)** Yes. Resize When you're ready to send one IPC Message to WebProcess
2. Message:"I'm going to get bigger. You gotta start over. Layout"
3. **Freeze Current Image (Snapshot)**♪ Until WebProcess Reply"I've got it."
4. Then toggle display

### 3.3 Code evidence
```
// WKWebView.mm Log in
"Stopped holding window resize snapshot for window full screen"
```

### 3.4 iOS Details of realization (WKWebViewIOS.mm)
- Use `RemoteLayerTreeDrawingAreaProxy` Tracking transaction ID
- `drawingArea->setSize()` Set the size of the drawing area
- DynamicView Update Mode:
  - `ResizingWithAnimation`
  - `ResizingWithDocumentHidden`
- **Live resize Snapshot Management**: If the web page is not responding, it will be a good idea.1Force removal of snapshots in seconds

---

## 4. Key issues and solutions

### 4.1 Question:SwiftUI scaleEffect Result Resize Storm
**Reason**: SwiftUI It's... scaleEffect It's going through. CALayer transform Zoom in on the view, but NSRemoteView The content is republished in the independent process. transform Impact.

**Performance**:
- The canvas is covered. scaleEffect Zoom in. (Like 2.5x)
- ViewBridgeHostView It's... frame Zoomed in.
- XPC Service I don't know. transform, resulting in a mismatch of size

### 4.2 Solutions thinking

#### Programme A: Interrupt layout transmission (Recommendations)
Yes. `NSViewRepresentable` Internal cut SwiftUI ♪ The way the world works ♪
```swift
// Don't let SwiftUI Direct management NSRemoteView It's... Frame
// Use pure AppKit It's... NSView Go wrap it up.
```

#### Programme B: Reverse Transform Offset
```swift
// Focus Mode, use 1/currentScale To offset the canvas zooming.
let scaleFactor = isFocused ? (1.0 / currentScale) : (cardWidth / viewportSize.width)
```

#### Programme C: Pure AppKit Achieved (WebKit Practices)
- Not being. scaleEffect Place in the view level of influence ViewBridge
- Will ViewBridge The contents are outside the canvas and directly covered in the eye.

### 4.3 The correct process for the sized sync
1. Host End calculates target size
2. Send IPC Organisation Service End
3. **Freeze Current Image** (Optional: Show snapshots)
4. Wait Service End finish rendering
5. Service Port Response"Completed"
6. Toggle display of new content

---

## 5. Info.plist Configure

### 5.1 XPC Service End
```xml
<key>NSExtension</key>
<dict>
    <key>NSExtensionPointIdentifier</key>
    <string>com.apple.ViewBridge</string>
</dict>

<key>XPCService</key>
<dict>
    <key>RunLoopType</key>
    <string>_NSApplicationMain</string>
</dict>

<key>NSSupportsViewBridge</key>
<true/>
```

---

## 6. TODO

- [ ] Cloning avaidyam/RemoteViewHost Research on practical realization
- [ ] Research WebKit It's... `_pageExtendedBackgroundColorDidChange` and resize snapshot Mechanisms
- [ ] Try Pure AppKit Programme: Not allowed SwiftUI Management NSRemoteView It's... frame
- [ ] Achieved IPC The message mechanism, Jean. Host Proactive Service Change in size
- [ ] Consider achieving Snapshot Mechanisms to avoid resize Flash

---

## 7. Reference Link

- [RemoteViewHost PoC](https://github.com/avaidyam/RemoteViewHost)
- [ViewBridge.framework Wiki](https://github.com/avaidyam/Parrot/wiki/ViewBridge.framework)
- [macOS Private Headers](https://github.com/w0lfschild/macOS_headers)
- [WebKit WKWebView.mm](https://github.com/WebKit/webkit/blob/main/Source/WebKit/UIProcess/API/Cocoa/WKWebView.mm)
- [Medium Articles](https://medium.com/@avaidyam/viewbridge-framework-it-works-484de6f31f7d)
