# Actrium Multi-Procedure Architecture Design Document

## I. OVERVIEW OF THE STRUCTURE

### 1.1 Design objectives

- **Process isolation**: Each Tab Runs in an independent process, and crashes do not affect the main application
- **Performance optimization**: React Native Rendering with Lord UI Parallel, multi-nucleus. CPU
- **Resource monitoring**: Independent process facilitates tracking of memory,CPU Use
- **Security**: Tab Content cannot directly access the main process memory

### 1.2 Technical Selection

| Requirements | Select Type | Rationale |
|------|------|------|
| Inter-process communications | **NSXPCConnection** + **anonymous listener** | Type security, two-way communication,Apple Native |
| Pixels Sharing | **IOSurface** | Zero copies,GPU Friendship. |
| Process Start | **Process** (NSTask) | Simple and reliable. |
| Endpoint Pass | **XPC Service (Broker)** | Sandbox compatibility, type security,launchd Management |

### 1.3 Overall structure (Triangle Broker Mode)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Actrium.app (Main Process)                            │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                        BrowserWindow                             │   │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐            │   │
│  │  │  Tab 1  │  │  Tab 2  │  │  Tab 3  │  │  Tab N  │            │   │
│  │  │IOSurface│  │IOSurface│  │IOSurface│  │IOSurface│            │   │
│  │  │HostView │  │HostView │  │HostView │  │HostView │            │   │
│  │  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘            │   │
│  └───────┼────────────┼────────────┼────────────┼──────────────────┘   │
│          │            │            │            │                       │
│  ┌───────┴────────────┴────────────┴────────────┴──────────────────┐   │
│  │                      TabProcessManager                           │   │
│  │  - Connection Broker, registered as Main App                                   │   │
│  │  - Receive Helper It's... XPC Endpoint (via Broker)                       │   │
│  │  - Straight Company HelperManagement NSXPCConnection Life cycle                      │   │
│  │  - Forward events, handle returns                                             │   │
│  │  - Monitor process status, handle crash recovery                                      │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
          │                                                    │
          │ XPC (Broker)                                       │ XPC (Straight Company)
          ▼                                                    ▼
┌──────────────────────┐                         ┌──────────────────────────┐
│   ActriumBroker       │                         │    ActriumTabRenderer     │
│   (XPC Service)      │                         │    (Helper App)          │
│                      │                         │                          │
│  - launchd Management       │  <── XPC Registration ───       │  NSXPCListener.anonymous()│
│  - Forward Endpoint     │                         │                          │
│  - Sandbox compliance          │                         │  NSApplication.run()     │
│                      │                         │  React Native + IOSurface │
└──────────────────────┘                         └──────────────────────────┘

═══════════════════════════════════════════════════════════════════════════
                              Triangular architecture workflow
═══════════════════════════════════════════════════════════════════════════

Phase 1: Shake hands. (Pass. Broker)
┌─────────┐         ┌─────────────┐         ┌────────────────┐
│  Actrium │◀───────▶│ActriumBroker │◀───────▶│ActriumTabRenderer│
│  (Main) │   XPC   │(XPC Service)│   XPC   │    (Helper)     │
└─────────┘         └─────────────┘         └────────────────┘
    │                     │                        │
    │  registerMainApp()  │                        │
    │────────────────────>│                        │
    │                     │                        │
    │                     │  registerHelper(endpoint)
    │                     │<───────────────────────│
    │                     │                        │
    │  helperDidRegister  │                        │
    │  (endpoint)         │                        │
    │<────────────────────│                        │

Phase 2: Straight Company (Broker Leave.)
┌─────────┐                                  ┌────────────────┐
│  Actrium │◀════════════════════════════════▶│ActriumTabRenderer│
│  (Main) │      Direct XPC Connection (via endpoint) │    (Helper)     │
└─────────┘                                  └────────────────┘
```

---

## II. Launching the process (Triangle Broker Mode)

### 2.1 Time Scheduling

```
Actrium.app              ActriumBroker                ActriumTabRenderer
    │                   (XPC Service)                      │
    │                        │                             │
    │  1. Connect on startup Broker   │                             │
    │     serviceName:       │                             │
    │     "io.actrium.     │                             │
    │      actrium.ActriumBroker"                            │
    │───────────────────────>│                             │
    │                        │                             │
    │  2. registerMainApp()  │                             │
    │───────────────────────>│                             │
    │                        │                             │
    │  3. Process() Start     │                             │
    │     args: [tabId]      │                             │
    │─────────────────────────────────────────────────────>│
    │                        │                             │
    │                        │        4. Initialize NSApplication
    │                        │           setActivationPolicy(.prohibited)
    │                        │                             │
    │                        │        5. Create NSXPCListener.anonymous()
    │                        │           listener.resume()
    │                        │                             │
    │                        │  6. Connection Broker              │
    │                        │<────────────────────────────│
    │                        │                             │
    │                        │  7. registerHelper(tabId, endpoint)
    │                        │<────────────────────────────│
    │                        │                             │
    │  8. helperDidRegister  │                             │
    │     (tabId, endpoint)  │                             │
    │<───────────────────────│                             │
    │                        │                             │
    │  9. reply(success)     │                             │
    │                        │────────────────────────────>│
    │                        │                             │
    │                        │        10. NSApplication.run()
    │                        │            Waiting for direct connection
    │                        │                             │
    │  11. NSXPCConnection(listenerEndpoint:)             │
    │      Settings interfaces                                 │
    │      connection.resume()                            │
    │─────────────────────────────────────────────────────>│
    │                        │                             │
    │                        │        12. shouldAcceptNewConnection
    │                        │            Settings exportedObject
    │                        │            connection.resume()
    │                        │                             │
    │  13. Direct XPC Two-way Communication Create ✓ (Broker No more engagement)        │
    │<════════════════════════════════════════════════════>│
    │                        │                             │
    │  14. renderer.loadContent(config)                   │
    │─────────────────────────────────────────────────────>│
    │                        │                             │
    │                        │        15. Initialize React Native
    │                        │            Create IOSurface
    │                        │                             │
    │  16. reply(success, nil)                            │
    │<─────────────────────────────────────────────────────│
    │                        │                             │
    │  17. renderer.getIOSurfaceID()                      │
    │─────────────────────────────────────────────────────>│
    │                        │                             │
    │  18. reply(surfaceID)                               │
    │<─────────────────────────────────────────────────────│
    │                        │                             │
    │  19. IOSurfaceLookup(surfaceID)                     │
    │      ♪ Tie it to the CALayer Show                             │
    │                        │                             │
```

### 2.2 Endpoint Transfer protocol (Pass. XPC Service)

**Why? XPC Service As Broker: **
- `NSXPCListenerEndpoint` Only through. XPC Connect to pass (not available) NSKeyedArchiver)
- XPC Service Yes. launchd It's managed, legally registered in a sandbox. Mach Services
- After handshake is done Broker The government is not going to be able to do this.Main ↔ Helper Direct company, no middleman.
- Reservations XPC Type security and programming experience

---

## III.XPC Protocol design

### 3.1 Main → Helper (TabRendererProtocol)

```swift
@objc protocol TabRendererProtocol {
    /// Load Tab Contents
    /// - config: Tab Configure (URLScripts, Styles, etc.)
    /// - reply: Success/Failed to callback
    func loadContent(_ config: TabXPCConfig, reply: @escaping (Bool, String?) -> Void)

    /// Update Tab Contents (Heat Load)
    func updateContent(_ config: TabXPCConfig, reply: @escaping (Bool, String?) -> Void)

    /// Forward input event (Mouse, Keyboard, Wheel)
    func handleEvent(_ event: TabXPCEvent)

    /// Update the size of the view mouth (Window resize)
    func updateViewport(width: Double, height: Double)

    /// Access process performance indicators
    func getMetrics(reply: @escaping (TabXPCMetrics?) -> Void)

    /// Access IOSurface ID (For rendering across processes)
    func getIOSurfaceID(reply: @escaping (UInt32) -> Void)

    /// Implementation JavaScript (DevTools Console)
    func evaluateScript(_ script: String, reply: @escaping (String?) -> Void)

    /// Get Component Tree (DevTools Elements)
    func getComponentTree(reply: @escaping (Data?) -> Void)

    /// Highlight Assign Nodes (DevTools Inspect)
    func highlightNode(_ nodeId: String)

    /// Clear Highlight
    func clearHighlight()

    /// Excellency.
    func terminate()
}
```

### 3.2 Helper → Main (TabRendererCallbackProtocol)

```swift
@objc protocol TabRendererCallbackProtocol {
    /// Console Log (Forward to DevTools)
    /// - level: debug/info/warn/error
    /// - message: Log Contents
    /// - source: Source (JS/Native)
    func consoleLog(level: String, message: String, source: String)

    /// Tab Change of title
    func titleChanged(_ newTitle: String)

    /// Navigation Request (For example: Click Link to Open New Tab)
    func navigationRequest(url: String, newTab: Bool)

    /// IOSurface Update (Redrawn)
    func surfaceDidUpdate()

    /// Renderer crashes or fatal errors
    func rendererDidCrash(reason: String)

    /// Host-Guest Message (DevTools Messages Panel)
    func hostGuestMessage(direction: String, event: String, payload: String?, tabId: String?)

    /// Network Request to begin (DevTools Network Panel)
    func networkRequestStart(requestId: String, url: String, method: String, headers: [String: String]?, body: String?)

    /// Network Request completed
    func networkRequestEnd(requestId: String, status: Int, ok: Bool, headers: [String: String]?, body: String?, error: String?)

    /// Engine Monitor Quickshot (DevTools Monitor Panel)
    func engineMonitorSnapshot(_ jsonSnapshot: String)

    /// Selection of Nodes (DevTools Elements Panel Two-way Synchronization)
    func nodeSelected(_ nodeId: String, boxModel: Data?, styles: Data?)
}
```

### 3.3 Data Type

```swift
// Tab Configure
@objc class TabXPCConfig: NSObject, NSSecureCoding {
    let tabId: String
    let title: String
    let url: String?
    let htmlContent: String?
    let unifiedScript: String?
    let backgroundColor: String
    let contractName: String?
    let contractVersion: Int?
    let permissions: [String]?
    // ...
}

// Enter Event
@objc class TabXPCEvent: NSObject, NSSecureCoding {
    enum EventType: Int {
        case mouseDown, mouseUp, mouseMoved, mouseDragged
        case scrollWheel
        case keyDown, keyUp, flagsChanged
    }
    let eventType: Int
    let locationX, locationY: Double
    let viewWidth, viewHeight: Double  // For Coordinate Zooming
    let modifierFlags: UInt
    let keyCode: UInt16
    let characters: String?
    let scrollDeltaX, scrollDeltaY: Double
}

// Performance indicators
@objc class TabXPCMetrics: NSObject, NSSecureCoding {
    let tabId: String
    let pid: Int32
    let memoryBytes: UInt64
    let cpuTimeNanoseconds: UInt64
    let timestamp: Date
}
```

---

## IV.IOSurface Pixels Sharing

### 4.1 The rationale.

```
┌─────────────────────────────────────────────────────────────────┐
│                      Helper Process (Render)                          │
│                                                                 │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │ React Native│ ──> │ NSWindow    │ ──> │ IOSurface   │       │
│  │ View        │     │ (offscreen) │     │ (GPUMemory)    │       │
│  └─────────────┘     └─────────────┘     └──────┬──────┘       │
│                                                  │              │
│                                    IOSurfaceGetID() = 12345    │
└──────────────────────────────────────────────────┼──────────────┘
                                                   │
                                          XPC Pass ID
                                                   │
┌──────────────────────────────────────────────────┼──────────────┐
│                      Main Process (Show)             │              │
│                                                  ▼              │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │ IOSurface   │ <── │ CALayer     │ <── │ SwiftUI     │       │
│  │ Lookup(ID)  │     │ .contents   │     │ View        │       │
│  └─────────────┘     └─────────────┘     └─────────────┘       │
│                                                                 │
│         Zero!GPU Read directly the same memory                            │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 IOSurface Create (Helper End)

```swift
func createIOSurface(width: Int, height: Int) -> IOSurface? {
    let properties: [IOSurfacePropertyKey: Any] = [
        .width: width,
        .height: height,
        .pixelFormat: kCVPixelFormatType_32BGRA,
        .bytesPerElement: 4,
        .bytesPerRow: width * 4,
        .allocSize: width * height * 4
    ]
    return IOSurface(properties: properties as [IOSurfacePropertyKey: Any])
}
```

### 4.3 IOSurface Find (Main End)

```swift
func lookupSurface(id: UInt32) -> IOSurface? {
    IOSurfaceLookup(id)  // kernel automatically creates a cross-process map
}

// ♪ Tie it to the CALayer
layer.contents = surface
```

### 4.4 Synchronization mechanism

```
Render frame sync:
1. Helper: I'm gonna finish a dye.
2. Helper: callbackProxy.surfaceDidUpdate()
3. Main: The following are some of the most recent examples of the events:CALayer.setNeedsDisplay()
4. Main: Next. VSync Time GPU Read IOSurface

Note:IOSurface No built-in locks, application level assurance required:
- Helper Notify when the rendering is complete
- Main Read after receiving notification
- DisplayLink Control the resonance and avoid tearing
```

---

## V.DevTools Integration

### 5.1 Data stream

```
┌─────────────────────────────────────────────────────────────────┐
│                        Helper Process                               │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    React Native                            │  │
│  │                                                            │  │
│  │  console.log() ─────────────────────────────────────────┐  │  │
│  │  fetch() ───────────────────────────────────────────────┤  │  │
│  │  Host-Guest Message ────────────────────────────────────┤  │  │
│  │  Engine Diagnostics ────────────────────────────────────┤  │  │
│  │  Component Tree ────────────────────────────────────────┤  │  │
│  └─────────────────────────────────────────────────────────┼──┘  │
│                                                            │     │
│  ┌─────────────────────────────────────────────────────────┼──┐  │
│  │               TabRendererService                        │  │  │
│  │                                                         ▼  │  │
│  │  callbackProxy.consoleLog(...)          ────────────────┼──┼──>
│  │  callbackProxy.networkRequestStart(...) ────────────────┼──┼──>
│  │  callbackProxy.hostGuestMessage(...)    ────────────────┼──┼──>
│  │  callbackProxy.engineMonitorSnapshot(...)───────────────┼──┼──>
│  │  callbackProxy.nodeSelected(...)        ────────────────┼──┼──>
│  └─────────────────────────────────────────────────────────┘  │  │
└───────────────────────────────────────────────────────────────┘  │
                                                                   │
                              XPC Callback                         │
                                                                   │
┌───────────────────────────────────────────────────────────────┐  │
│                         Main Process                              │  │
│                                                                │  │
│  ┌──────────────────────────────────────────────────────────┐ │  │
│  │              TabRendererCallbackHandler                   │<┘  │
│  │                                                           │    │
│  │  func consoleLog(...) {                                   │    │
│  │      DevToolsManager.shared.logFromRenderer(...)          │    │
│  │  }                                                        │    │
│  │                                                           │    │
│  │  func networkRequestStart(...) {                          │    │
│  │      DevToolsManager.shared.upsertNetworkRequestStart(...) │    │
│  │  }                                                        │    │
│  │  ...                                                      │    │
│  └──────────────────────────────────────────────────────────┘    │
│                               │                                  │
│                               ▼                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │                   DevToolsManager                         │    │
│  │                                                           │    │
│  │  @Published consoleLogs: [DevToolsLogEntry]               │    │
│  │  @Published networkRequests: [DevToolsNetworkRequest]     │    │
│  │  @Published messages: [DevToolsMessage]                   │    │
│  │  @Published engineMonitorSnapshot: ...                    │    │
│  │  @Published componentTree: DevToolsComponentNode?         │    │
│  └──────────────────────────────────────────────────────────┘    │
│                               │                                  │
│                               ▼                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │                    DevTools UI                            │    │
│  │  ┌─────────┬─────────┬─────────┬─────────┬─────────┐     │    │
│  │  │ Console │ Network │ Messages│ Elements│ Monitor │     │    │
│  │  └─────────┴─────────┴─────────┴─────────┴─────────┘     │    │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

### 5.2 DevTools Function Matrix

| Functions | Data sources | XPC Methodology | Orientation |
|------|----------|----------|------|
| **Console** | JS console.log/warn/error | `consoleLog()` | Helper → Main |
| **Network** | fetch() Interception | `networkRequestStart/End()` | Helper → Main |
| **Messages** | Host-Guest Communications | `hostGuestMessage()` | Helper → Main |
| **Elements** | React DevTools Agreements | `getComponentTree()` | Main → Helper |
| **Elements** | Select node sync | `nodeSelected()` | Two-way |
| **Monitor** | Engine Diagnostic data | `engineMonitorSnapshot()` | Helper → Main |
| **Sources** | Source Presentation | (Local File Read) | Main End |
| **Evaluate** | JS Implementation | `evaluateScript()` | Main → Helper |
| **Inspect** | Highlight | `highlightNode()` | Main → Helper |

---

## VI. Life cycle management

### 6.1 Process Status Machine

```
                    ┌─────────┐
                    │  idle   │
                    └────┬────┘
                         │ getRenderer()
                         ▼
                    ┌─────────┐
                    │launching│
                    └────┬────┘
                         │ endpoint Copy that.
                         │ XPC Connection Create
                         ▼
                    ┌─────────┐
        ┌──────────│ running │──────────┐
        │          └────┬────┘          │
        │               │               │
   XPC Disconnect        User Close Tab      Process crashes
        │               │               │
        ▼               ▼               ▼
   ┌─────────┐    ┌─────────┐    ┌─────────┐
   │terminated│   │terminated│   │ crashed │
   └─────────┘    └─────────┘    └────┬────┘
                                      │ Autorecover?
                                      ▼
                                 ┌─────────┐
                                 │launching│
                                 └─────────┘
```

### 6.2 Crash Recovery Strategy

```swift
// TabProcessManager
func handleProcessCrash(tabId: UUID, reason: String) {
    // 1. Announcements UI Show crash page
    NotificationCenter.default.post(
        name: .tabRendererCrashed,
        object: nil,
        userInfo: ["tabId": tabId, "reason": reason]
    )

    // 2. Cleaning of resources
    processes[tabId]?.connection?.invalidate()
    processes[tabId]?.process?.terminate()

    // 3. Optional: Autostart (Take a break.)
    if shouldAutoRestart(tabId: tabId) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
            self.restartTab(tabId: tabId)
        }
    }
}

// Source of crash detection:
// 1. Process.terminationHandler (Process exit)
// 2. XPC connection.invalidationHandler (Connection Disconnected)
// 3. callbackProxy.rendererDidCrash() (Active reporting)
```

### 6.3 Memory Pressure Disposal

```swift
// Pressures in the main process listening system
memoryPressureSource = DispatchSource.makeMemoryPressureSource(
    eventMask: [.warning, .critical],
    queue: .main
)

memoryPressureSource?.setEventHandler { [weak self] in
    if event.contains(.critical) {
        // End Inactive Tab Rendering process
        self?.terminateInactiveTabs()
    } else if event.contains(.warning) {
        // Announcements Helper Release Cache
        self?.notifyMemoryWarning()
    }
}
```

---

## VII. Catalogue structure

```
Actrium/
├── Actrium.app/                              # Main Application Bundle
│   └── Contents/
│       ├── MacOS/Actrium                     # Main Executable
│       ├── Resources/
│       │   └── ActriumTabRenderer.app/       # Helper App (Embedded)
│       │       └── Contents/
│       │           ├── MacOS/ActriumTabRenderer
│       │           └── Info.plist
│       └── Info.plist
│
├── Actrium/                                  # Lord App Source
│   ├── App/
│   │   └── ActriumApp.swift
│   ├── Browser/
│   │   ├── BrowserWindow.swift
│   │   └── BrowserViewModel.swift
│   ├── Tab/
│   │   ├── TabView.swift
│   │   └── TabViewModel.swift
│   ├── MultiProcess/
│   │   ├── TabProcessManager.swift          # Process Manager
│   │   ├── TabProcessInfo.swift             # Process Status
│   │   ├── IOSurfaceHostView.swift          # IOSurface Show
│   │   └── TabRendererCallbackHandler.swift # XPC Recourse processing
│   └── DevTools/
│       ├── DevToolsManager.swift            # DevTools Status
│       ├── DevToolsPanel.swift              # UI
│       └── ...
│
├── ActriumTabRenderer/                       # Helper App Source
│   ├── main.swift                           # Entry point
│   ├── RendererAppDelegate.swift            # App Life cycle + XPC
│   ├── TabRendererService.swift             # XPC Service realization
│   ├── ReactNativeHost.swift                # RN Initialize
│   ├── IOSurfaceRenderer.swift              # Disconnect Rendering
│   └── Info.plist
│
└── Shared/                                  # Shared Code (Framework or a file)
    ├── TabXPCProtocol.swift                 # XPC Agreement definition
    ├── TabXPCTypes.swift                    # Share data type
    └── TabXPCInterface.swift                # Interface Plant
```

---

## VIII.Xcode Configure

### 8.1 Targets

| Target | Type | Product | Annotations |
|--------|------|---------|------|
| **Actrium** | Application | Actrium.app | Main Application |
| **ActriumTabRenderer** | Application | ActriumTabRenderer.app | Helper App |
| **ActriumShared** | Framework (Optional) | ActriumShared.framework | Shared Code |

### 8.2 Build Phases (Actrium Target)

```
1. Dependencies
   └── ActriumTabRenderer

2. Compile Sources
   └── (Lord App Source File)

3. Copy Bundle Resources
   └── (Resource File)

4. Copy Files
   └── Destination: Resources
   └── Subpath: (Empty)
   └── Files: ActriumTabRenderer.app
```

### 8.3 Info.plist (ActriumTabRenderer)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" ...>
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>io.actrium.tabrenderer</string>

    <key>CFBundleName</key>
    <string>ActriumTabRenderer</string>

    <key>CFBundleExecutable</key>
    <string>ActriumTabRenderer</string>

    <key>CFBundlePackageType</key>
    <string>APPL</string>

    <!-- Not shown in Dock -->
    <key>LSBackgroundOnly</key>
    <true/>

    <!-- Do not show menu bar icons -->
    <key>LSUIElement</key>
    <true/>

    <!-- Support Retina -->
    <key>NSHighResolutionCapable</key>
    <true/>

    <!-- App Transport Security (Network if you need it.) -->
    <key>NSAppTransportSecurity</key>
    <dict>
        <key>NSAllowsArbitraryLoads</key>
        <true/>
    </dict>
</dict>
</plist>
```

---

## IX. SECURITY CONSIDERATIONS

### 9.1 Process isolation

- Helper Process Use `LSBackgroundOnly`None UI Interactive capacity
- Every one. Tab Independence process, memory space isolation
- XPC Communication verified with kernel.

### 9.2 Sandbox (Optional)

```xml
<!-- ActriumTabRenderer.entitlements -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" ...>
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    <true/>

    <!-- Network access -->
    <key>com.apple.security.network.client</key>
    <true/>

    <!-- Read-only file access (If local resources need to be loaded) -->
    <key>com.apple.security.files.user-selected.read-only</key>
    <true/>
</dict>
</plist>
```

### 9.3 Code Signature

- Lord App and Helper App Use same Team ID
- Helper App Embedded Master App Bundle Inherit signature trust

---

## X. PERFORMANCE

### 10.1 Start Optimization

- Helper App Preheat: preload one free on application startup Helper
- Endpoint Transfer using binary format, avoid JSON Sequencing expenses

### 10.2 Rendering Optimization

- IOSurface Zero copies,GPU Read directly
- DisplayLink Sync Frame Rate to Avoid Over-rendering
- Inactive Tab Pause Render Cycle

### 10.3 Memory Optimization

- Inactive Tab It's free. IOSurface,Reserve Status
- Auto-termination of background when memory pressure Tab

---

## XI. Debugging support

### 11.1 Helper Process debugging

```bash
# Modalities1: Xcode Attach to Process
Debug > Attach to Process > ActriumTabRenderer

# Modalities2Command line start Helper (♪ Around the Lord ♪ App)
/path/to/ActriumTabRenderer.app/Contents/MacOS/ActriumTabRenderer test-tab-id
```

### 11.2 Log

```swift
// Helper End Use OSLog
import os.log
let logger = Logger(subsystem: "io.actrium.tabrenderer", category: "Render")
logger.info("Frame rendered in \(duration)ms")

// View Log
log stream --predicate 'subsystem == "io.actrium.tabrenderer"'
```

### 11.3 Collapse Report

- Helper The crash will create independence. crash report
- Path:`~/Library/Logs/DiagnosticReports/ActriumTabRenderer-*.ips`

---

## XII. REMOTION PLANS

### Phase 1: Foundation
- [ ] Create ActriumTabRenderer target
- [ ] Achieved NSXPCListener.anonymous() Start
- [ ] Achieved endpoint stdout Transfer
- [ ] Achieved TabProcessManager Connection Management

### Phase 2: Render function
- [ ] Migration TabRendererService Present. Helper
- [ ] Achieved IOSurface Create and share
- [ ] Other Organiser
- [ ] Achieved viewport Adjustment

### Phase 3: DevTools Integration
- [ ] Achieved console Log Forward
- [ ] Achieved network Requesting tracking.
- [ ] Achieved host-guest Message Record
- [ ] Achieved engine monitor Data transfer
- [ ] Achieved elements Component Tree Query

### Phase 4: Stability
- [ ] ..to achieve crash detection and recovery.
- [ ] Achieve memory pressure management
- [ ] Performance testing and optimization
- [ ] Clean up the old ones. XPC Service Code

---

## XIII.FAQ

**Q: Why do you need it? XPC Service As Broker?**
A:
1. `NSXPCListenerEndpoint` Only through. XPC Connect to pass, no use `NSKeyedArchiver` Sequenced
2. XPC Service Yes. launchd It's managed, legally registered in a sandbox. Mach Services
3. After handshake is done Broker The government is not going to be able to do this.Main ↔ Helper Direct company, no middleman.

**Q: Why don't you just use it? XPC Service Rendering?**
A: XPC Service It's... `xpc_main()` ♪ Will take over the main thread cycle of events ♪ `NSApplication.run()` Conflict, leading to failure to create properly AppKit/React Native UI Component. So the rendering must be independent Helper App In progress.

**Q: Helper The crash will affect the Lord. App - Yeah?**
A: No, I won't.Helper It's an independent process. A breakdown will only lead to a response. Tab Show error page, master App Other Tab Not affected.

**Q: IOSurface ID Will it repeat?**
A: IOSurface ID It is the only system in the world, distributed by the kernel and not repeated.

**Q: How to Debug Helper Process?**
A: Xcode > Debug > Attach to ProcessOr in... Scheme Center Configuration Direct Start Helper.

**Q: Is the triangle working?**
A: Almost.Broker Only initial handshake (transmitting) endpoint) , and then Main ↔ Helper It's straight-on. XPC Communications,Broker No more interference.
