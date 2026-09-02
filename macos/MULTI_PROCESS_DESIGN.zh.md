# Actrium 多进程架构设计文档

## 一、架构概述

### 1.1 设计目标

- **进程隔离**：每个 Tab 运行在独立进程，崩溃不影响主应用
- **性能优化**：React Native 渲染与主 UI 并行，利用多核 CPU
- **资源监控**：独立进程便于追踪内存、CPU 使用
- **安全性**：Tab 内容无法直接访问主进程内存

### 1.2 技术选型

| 需求 | 选型 | 理由 |
|------|------|------|
| 进程间通信 | **NSXPCConnection** + **anonymous listener** | 类型安全、双向通信、Apple 原生 |
| 像素共享 | **IOSurface** | 零拷贝、GPU 友好 |
| 进程启动 | **Process** (NSTask) | 简单可靠 |
| Endpoint 传递 | **XPC Service (Broker)** | 沙盒兼容、类型安全、launchd 管理 |

### 1.3 整体架构图 (三角 Broker 模式)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Actrium.app (主进程)                            │
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
│  │  - 连接 Broker，注册为 Main App                                   │   │
│  │  - 接收 Helper 的 XPC Endpoint (via Broker)                       │   │
│  │  - 直连 Helper，管理 NSXPCConnection 生命周期                      │   │
│  │  - 转发事件、处理回调                                             │   │
│  │  - 监控进程状态、处理崩溃恢复                                      │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
          │                                                    │
          │ XPC (Broker)                                       │ XPC (直连)
          ▼                                                    ▼
┌──────────────────────┐                         ┌──────────────────────────┐
│   ActriumBroker       │                         │    ActriumTabRenderer     │
│   (XPC Service)      │                         │    (Helper App)          │
│                      │                         │                          │
│  - launchd 管理       │  <── XPC 注册 ───       │  NSXPCListener.anonymous()│
│  - 转发 Endpoint     │                         │                          │
│  - 沙盒合规          │                         │  NSApplication.run()     │
│                      │                         │  React Native + IOSurface │
└──────────────────────┘                         └──────────────────────────┘

═══════════════════════════════════════════════════════════════════════════
                              三角架构工作流程
═══════════════════════════════════════════════════════════════════════════

Phase 1: 握手 (通过 Broker)
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

Phase 2: 直连 (Broker 退场)
┌─────────┐                                  ┌────────────────┐
│  Actrium │◀════════════════════════════════▶│ActriumTabRenderer│
│  (Main) │      直接 XPC 连接 (via endpoint) │    (Helper)     │
└─────────┘                                  └────────────────┘
```

---

## 二、启动流程 (三角 Broker 模式)

### 2.1 时序图

```
Actrium.app              ActriumBroker                ActriumTabRenderer
    │                   (XPC Service)                      │
    │                        │                             │
    │  1. 启动时连接 Broker   │                             │
    │     serviceName:       │                             │
    │     "io.actrium.     │                             │
    │      actrium.ActriumBroker"                            │
    │───────────────────────>│                             │
    │                        │                             │
    │  2. registerMainApp()  │                             │
    │───────────────────────>│                             │
    │                        │                             │
    │  3. Process() 启动     │                             │
    │     args: [tabId]      │                             │
    │─────────────────────────────────────────────────────>│
    │                        │                             │
    │                        │        4. 初始化 NSApplication
    │                        │           setActivationPolicy(.prohibited)
    │                        │                             │
    │                        │        5. 创建 NSXPCListener.anonymous()
    │                        │           listener.resume()
    │                        │                             │
    │                        │  6. 连接 Broker              │
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
    │                        │            等待直接连接
    │                        │                             │
    │  11. NSXPCConnection(listenerEndpoint:)             │
    │      设置 interfaces                                 │
    │      connection.resume()                            │
    │─────────────────────────────────────────────────────>│
    │                        │                             │
    │                        │        12. shouldAcceptNewConnection
    │                        │            设置 exportedObject
    │                        │            connection.resume()
    │                        │                             │
    │  13. 直接 XPC 双向通信建立 ✓ (Broker 不再参与)        │
    │<════════════════════════════════════════════════════>│
    │                        │                             │
    │  14. renderer.loadContent(config)                   │
    │─────────────────────────────────────────────────────>│
    │                        │                             │
    │                        │        15. 初始化 React Native
    │                        │            创建 IOSurface
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
    │      绑定到 CALayer 显示                             │
    │                        │                             │
```

### 2.2 Endpoint 传输协议 (通过 XPC Service)

**为什么用 XPC Service 作为 Broker：**
- `NSXPCListenerEndpoint` 只能通过 XPC 连接传递（不能用 NSKeyedArchiver）
- XPC Service 是 launchd 管理的，可在沙盒内合法注册 Mach 服务
- 握手完成后 Broker 退场，Main ↔ Helper 直连，无中间人开销
- 保留 XPC 的类型安全和编程体验

---

## 三、XPC 协议设计

### 3.1 Main → Helper (TabRendererProtocol)

```swift
@objc protocol TabRendererProtocol {
    /// 加载 Tab 内容
    /// - config: Tab 配置 (URL、脚本、样式等)
    /// - reply: 成功/失败回调
    func loadContent(_ config: TabXPCConfig, reply: @escaping (Bool, String?) -> Void)

    /// 更新 Tab 内容 (热重载)
    func updateContent(_ config: TabXPCConfig, reply: @escaping (Bool, String?) -> Void)

    /// 转发输入事件 (鼠标、键盘、滚轮)
    func handleEvent(_ event: TabXPCEvent)

    /// 更新视口大小 (窗口 resize)
    func updateViewport(width: Double, height: Double)

    /// 获取进程性能指标
    func getMetrics(reply: @escaping (TabXPCMetrics?) -> Void)

    /// 获取 IOSurface ID (用于跨进程渲染)
    func getIOSurfaceID(reply: @escaping (UInt32) -> Void)

    /// 执行 JavaScript (DevTools Console)
    func evaluateScript(_ script: String, reply: @escaping (String?) -> Void)

    /// 获取组件树 (DevTools Elements)
    func getComponentTree(reply: @escaping (Data?) -> Void)

    /// 高亮指定节点 (DevTools Inspect)
    func highlightNode(_ nodeId: String)

    /// 清除高亮
    func clearHighlight()

    /// 优雅终止
    func terminate()
}
```

### 3.2 Helper → Main (TabRendererCallbackProtocol)

```swift
@objc protocol TabRendererCallbackProtocol {
    /// Console 日志 (转发到 DevTools)
    /// - level: debug/info/warn/error
    /// - message: 日志内容
    /// - source: 来源 (JS/Native)
    func consoleLog(level: String, message: String, source: String)

    /// Tab 标题变更
    func titleChanged(_ newTitle: String)

    /// 导航请求 (如：点击链接打开新 Tab)
    func navigationRequest(url: String, newTab: Bool)

    /// IOSurface 内容已更新 (需要重绘)
    func surfaceDidUpdate()

    /// 渲染器崩溃或致命错误
    func rendererDidCrash(reason: String)

    /// Host-Guest 消息 (DevTools Messages 面板)
    func hostGuestMessage(direction: String, event: String, payload: String?, tabId: String?)

    /// Network 请求开始 (DevTools Network 面板)
    func networkRequestStart(requestId: String, url: String, method: String, headers: [String: String]?, body: String?)

    /// Network 请求完成
    func networkRequestEnd(requestId: String, status: Int, ok: Bool, headers: [String: String]?, body: String?, error: String?)

    /// Engine Monitor 快照 (DevTools Monitor 面板)
    func engineMonitorSnapshot(_ jsonSnapshot: String)

    /// 节点选中 (DevTools Elements 面板双向同步)
    func nodeSelected(_ nodeId: String, boxModel: Data?, styles: Data?)
}
```

### 3.3 数据类型

```swift
// Tab 配置
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

// 输入事件
@objc class TabXPCEvent: NSObject, NSSecureCoding {
    enum EventType: Int {
        case mouseDown, mouseUp, mouseMoved, mouseDragged
        case scrollWheel
        case keyDown, keyUp, flagsChanged
    }
    let eventType: Int
    let locationX, locationY: Double
    let viewWidth, viewHeight: Double  // 用于坐标缩放
    let modifierFlags: UInt
    let keyCode: UInt16
    let characters: String?
    let scrollDeltaX, scrollDeltaY: Double
}

// 性能指标
@objc class TabXPCMetrics: NSObject, NSSecureCoding {
    let tabId: String
    let pid: Int32
    let memoryBytes: UInt64
    let cpuTimeNanoseconds: UInt64
    let timestamp: Date
}
```

---

## 四、IOSurface 像素共享

### 4.1 工作原理

```
┌─────────────────────────────────────────────────────────────────┐
│                      Helper 进程 (渲染)                          │
│                                                                 │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │ React Native│ ──> │ NSWindow    │ ──> │ IOSurface   │       │
│  │ View        │     │ (offscreen) │     │ (GPU内存)    │       │
│  └─────────────┘     └─────────────┘     └──────┬──────┘       │
│                                                  │              │
│                                    IOSurfaceGetID() = 12345    │
└──────────────────────────────────────────────────┼──────────────┘
                                                   │
                                          XPC 传递 ID
                                                   │
┌──────────────────────────────────────────────────┼──────────────┐
│                      Main 进程 (显示)             │              │
│                                                  ▼              │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │ IOSurface   │ <── │ CALayer     │ <── │ SwiftUI     │       │
│  │ Lookup(ID)  │     │ .contents   │     │ View        │       │
│  └─────────────┘     └─────────────┘     └─────────────┘       │
│                                                                 │
│         零拷贝！GPU 直接读取同一块显存                            │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 IOSurface 创建 (Helper 端)

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

### 4.3 IOSurface 查找 (Main 端)

```swift
func lookupSurface(id: UInt32) -> IOSurface? {
    IOSurfaceLookup(id)  // 内核自动建立跨进程映射
}

// 绑定到 CALayer
layer.contents = surface
```

### 4.4 同步机制

```
渲染帧同步：
1. Helper: 完成一帧渲染
2. Helper: callbackProxy.surfaceDidUpdate()
3. Main: 收到通知，CALayer.setNeedsDisplay()
4. Main: 下一个 VSync 时 GPU 读取 IOSurface

注意：IOSurface 没有内置锁，需要应用层保证：
- Helper 渲染完成后再通知
- Main 在收到通知后再读取
- DisplayLink 控制渲染节奏，避免撕裂
```

---

## 五、DevTools 集成

### 5.1 数据流

```
┌─────────────────────────────────────────────────────────────────┐
│                        Helper 进程                               │
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
│                         Main 进程                              │  │
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

### 5.2 DevTools 功能矩阵

| 功能 | 数据来源 | XPC 方法 | 方向 |
|------|----------|----------|------|
| **Console** | JS console.log/warn/error | `consoleLog()` | Helper → Main |
| **Network** | fetch() 拦截 | `networkRequestStart/End()` | Helper → Main |
| **Messages** | Host-Guest 通信 | `hostGuestMessage()` | Helper → Main |
| **Elements** | React DevTools 协议 | `getComponentTree()` | Main → Helper |
| **Elements** | 节点选中同步 | `nodeSelected()` | 双向 |
| **Monitor** | Engine 诊断数据 | `engineMonitorSnapshot()` | Helper → Main |
| **Sources** | 源码展示 | (本地文件读取) | Main 端 |
| **Evaluate** | JS 执行 | `evaluateScript()` | Main → Helper |
| **Inspect** | 节点高亮 | `highlightNode()` | Main → Helper |

---

## 六、生命周期管理

### 6.1 进程状态机

```
                    ┌─────────┐
                    │  idle   │
                    └────┬────┘
                         │ getRenderer()
                         ▼
                    ┌─────────┐
                    │launching│
                    └────┬────┘
                         │ endpoint 收到
                         │ XPC 连接建立
                         ▼
                    ┌─────────┐
        ┌──────────│ running │──────────┐
        │          └────┬────┘          │
        │               │               │
   XPC 断开        用户关闭 Tab      进程崩溃
        │               │               │
        ▼               ▼               ▼
   ┌─────────┐    ┌─────────┐    ┌─────────┐
   │terminated│   │terminated│   │ crashed │
   └─────────┘    └─────────┘    └────┬────┘
                                      │ 自动恢复?
                                      ▼
                                 ┌─────────┐
                                 │launching│
                                 └─────────┘
```

### 6.2 崩溃恢复策略

```swift
// TabProcessManager
func handleProcessCrash(tabId: UUID, reason: String) {
    // 1. 通知 UI 显示崩溃页面
    NotificationCenter.default.post(
        name: .tabRendererCrashed,
        object: nil,
        userInfo: ["tabId": tabId, "reason": reason]
    )

    // 2. 清理资源
    processes[tabId]?.connection?.invalidate()
    processes[tabId]?.process?.terminate()

    // 3. 可选：自动重启 (带退避)
    if shouldAutoRestart(tabId: tabId) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
            self.restartTab(tabId: tabId)
        }
    }
}

// 崩溃检测来源：
// 1. Process.terminationHandler (进程退出)
// 2. XPC connection.invalidationHandler (连接断开)
// 3. callbackProxy.rendererDidCrash() (主动报告)
```

### 6.3 内存压力处理

```swift
// 主进程监听系统内存压力
memoryPressureSource = DispatchSource.makeMemoryPressureSource(
    eventMask: [.warning, .critical],
    queue: .main
)

memoryPressureSource?.setEventHandler { [weak self] in
    if event.contains(.critical) {
        // 终止非活跃 Tab 的渲染进程
        self?.terminateInactiveTabs()
    } else if event.contains(.warning) {
        // 通知 Helper 释放缓存
        self?.notifyMemoryWarning()
    }
}
```

---

## 七、目录结构

```
Actrium/
├── Actrium.app/                              # 主应用 Bundle
│   └── Contents/
│       ├── MacOS/Actrium                     # 主可执行文件
│       ├── Resources/
│       │   └── ActriumTabRenderer.app/       # Helper App (嵌入)
│       │       └── Contents/
│       │           ├── MacOS/ActriumTabRenderer
│       │           └── Info.plist
│       └── Info.plist
│
├── Actrium/                                  # 主 App 源码
│   ├── App/
│   │   └── ActriumApp.swift
│   ├── Browser/
│   │   ├── BrowserWindow.swift
│   │   └── BrowserViewModel.swift
│   ├── Tab/
│   │   ├── TabView.swift
│   │   └── TabViewModel.swift
│   ├── MultiProcess/
│   │   ├── TabProcessManager.swift          # 进程管理器
│   │   ├── TabProcessInfo.swift             # 进程状态
│   │   ├── IOSurfaceHostView.swift          # IOSurface 显示
│   │   └── TabRendererCallbackHandler.swift # XPC 回调处理
│   └── DevTools/
│       ├── DevToolsManager.swift            # DevTools 状态
│       ├── DevToolsPanel.swift              # UI
│       └── ...
│
├── ActriumTabRenderer/                       # Helper App 源码
│   ├── main.swift                           # 入口点
│   ├── RendererAppDelegate.swift            # App 生命周期 + XPC
│   ├── TabRendererService.swift             # XPC 服务实现
│   ├── ReactNativeHost.swift                # RN 初始化
│   ├── IOSurfaceRenderer.swift              # 离屏渲染
│   └── Info.plist
│
└── Shared/                                  # 共享代码 (Framework 或文件)
    ├── TabXPCProtocol.swift                 # XPC 协议定义
    ├── TabXPCTypes.swift                    # 共享数据类型
    └── TabXPCInterface.swift                # Interface 工厂
```

---

## 八、Xcode 配置

### 8.1 Targets

| Target | Type | Product | 说明 |
|--------|------|---------|------|
| **Actrium** | Application | Actrium.app | 主应用 |
| **ActriumTabRenderer** | Application | ActriumTabRenderer.app | Helper App |
| **ActriumShared** | Framework (可选) | ActriumShared.framework | 共享代码 |

### 8.2 Build Phases (Actrium Target)

```
1. Dependencies
   └── ActriumTabRenderer

2. Compile Sources
   └── (主 App 源文件)

3. Copy Bundle Resources
   └── (资源文件)

4. Copy Files
   └── Destination: Resources
   └── Subpath: (空)
   └── Files: ActriumTabRenderer.app
```

### 8.3 Info.plist (ActriumTabRenderer)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" ...>
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>io.actrium.app.tabrenderer</string>

    <key>CFBundleName</key>
    <string>ActriumTabRenderer</string>

    <key>CFBundleExecutable</key>
    <string>ActriumTabRenderer</string>

    <key>CFBundlePackageType</key>
    <string>APPL</string>

    <!-- 不显示在 Dock -->
    <key>LSBackgroundOnly</key>
    <true/>

    <!-- 不显示菜单栏图标 -->
    <key>LSUIElement</key>
    <true/>

    <!-- 支持 Retina -->
    <key>NSHighResolutionCapable</key>
    <true/>

    <!-- App Transport Security (如需网络) -->
    <key>NSAppTransportSecurity</key>
    <dict>
        <key>NSAllowsArbitraryLoads</key>
        <true/>
    </dict>
</dict>
</plist>
```

---

## 九、安全考虑

### 9.1 进程隔离

- Helper 进程使用 `LSBackgroundOnly`，无 UI 交互能力
- 每个 Tab 独立进程，内存空间隔离
- XPC 通信经过内核验证

### 9.2 沙盒 (可选)

```xml
<!-- ActriumTabRenderer.entitlements -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" ...>
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    <true/>

    <!-- 网络访问 -->
    <key>com.apple.security.network.client</key>
    <true/>

    <!-- 只读文件访问 (如需加载本地资源) -->
    <key>com.apple.security.files.user-selected.read-only</key>
    <true/>
</dict>
</plist>
```

### 9.3 代码签名

- 主 App 和 Helper App 使用相同的 Team ID
- Helper App 嵌入主 App Bundle 内，继承签名信任

---

## 十、性能优化

### 10.1 启动优化

- Helper App 预热：在应用启动时预先加载一个空闲 Helper
- Endpoint 传输使用二进制格式，避免 JSON 序列化开销

### 10.2 渲染优化

- IOSurface 零拷贝，GPU 直接读取
- DisplayLink 同步帧率，避免过度渲染
- 非活跃 Tab 暂停渲染循环

### 10.3 内存优化

- 非活跃 Tab 可以释放 IOSurface，保留状态
- 内存压力时自动终止后台 Tab

---

## 十一、调试支持

### 11.1 Helper 进程调试

```bash
# 方式1：Xcode 附加到进程
Debug > Attach to Process > ActriumTabRenderer

# 方式2：命令行启动 Helper (绕过主 App)
/path/to/ActriumTabRenderer.app/Contents/MacOS/ActriumTabRenderer test-tab-id
```

### 11.2 日志

```swift
// Helper 端使用 OSLog
import os.log
let logger = Logger(subsystem: "io.actrium.app.tabrenderer", category: "Render")
logger.info("Frame rendered in \(duration)ms")

// 查看日志
log stream --predicate 'subsystem == "io.actrium.app.tabrenderer"'
```

### 11.3 崩溃报告

- Helper 崩溃会生成独立的 crash report
- 路径：`~/Library/Logs/DiagnosticReports/ActriumTabRenderer-*.ips`

---

## 十二、迁移计划

### Phase 1: 基础架构
- [ ] 创建 ActriumTabRenderer target
- [ ] 实现 NSXPCListener.anonymous() 启动
- [ ] 实现 endpoint stdout 传输
- [ ] 实现 TabProcessManager 连接管理

### Phase 2: 渲染功能
- [ ] 迁移 TabRendererService 到 Helper
- [ ] 实现 IOSurface 创建和共享
- [ ] 实现事件转发
- [ ] 实现 viewport 调整

### Phase 3: DevTools 集成
- [ ] 实现 console 日志转发
- [ ] 实现 network 请求追踪
- [ ] 实现 host-guest 消息记录
- [ ] 实现 engine monitor 数据推送
- [ ] 实现 elements 组件树查询

### Phase 4: 稳定性
- [ ] 实现崩溃检测和恢复
- [ ] 实现内存压力处理
- [ ] 性能测试和优化
- [ ] 清理旧的 XPC Service 代码

---

## 十三、FAQ

**Q: 为什么需要 XPC Service 作为 Broker？**
A:
1. `NSXPCListenerEndpoint` 只能通过 XPC 连接传递，不能用 `NSKeyedArchiver` 序列化
2. XPC Service 是 launchd 管理的，可在沙盒内合法注册 Mach 服务
3. 握手完成后 Broker 退场，Main ↔ Helper 直连，无中间人开销

**Q: 为什么不直接用 XPC Service 做渲染？**
A: XPC Service 的 `xpc_main()` 会接管主线程事件循环，与 `NSApplication.run()` 冲突，导致无法正常创建 AppKit/React Native UI 组件。所以渲染必须在独立的 Helper App 中进行。

**Q: Helper 崩溃会影响主 App 吗？**
A: 不会。Helper 是独立进程，崩溃只会导致对应 Tab 显示错误页面，主 App 和其他 Tab 不受影响。

**Q: IOSurface ID 会重复吗？**
A: IOSurface ID 是系统全局唯一的，由内核分配，不会重复。

**Q: 如何调试 Helper 进程？**
A: Xcode > Debug > Attach to Process，或者在 Scheme 中配置直接启动 Helper。

**Q: 三角架构有性能开销吗？**
A: 几乎没有。Broker 只参与初始握手（传递 endpoint），之后 Main ↔ Helper 是直连的 XPC 通信，Broker 不再介入。
