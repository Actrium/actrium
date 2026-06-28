//
//  ActroServiceViewController.swift
//  ActroViewService
//
//  NSServiceViewController subclass that hosts React Native content for ViewBridge.
//  Uses Objective-C runtime to properly inherit from private NSServiceViewController.
//
//  Architecture: 1:1:1 Mapping (Bridgeless Mode)
//  - Each Tab has exactly one ViewBridge connection
//  - Each ViewBridge connection has exactly one ActroServiceViewController instance
//  - Each ActroServiceViewController has exactly one RCTSurfaceHostingProxyRootView
//  - All root views share the same RCTHost (managed by ServiceReactNativeHost singleton)
//

import Cocoa
import ObjectiveC
import React
import React_RCTAppDelegate
import ReactAppDependencyProvider

// MARK: - C Helper for RCTHost (Bridgeless Mode)

/// C function to call RCTHost.callFunctionOnJSModule:method:args:
/// Defined in XPCNativeModules.m - Swift can't directly call 3-argument ObjC methods
@_silgen_name("RCTHostCallFunction")
private func RCTHostCallFunction(_ host: AnyObject?, _ module: NSString, _ method: NSString, _ args: NSArray)

// MARK: - Debug Logging

/// Millisecond timestamp for cross-process log correlation
private func ts() -> String {
    let now = Date()
    let formatter = DateFormatter()
    formatter.dateFormat = "HH:mm:ss.SSS"
    return formatter.string(from: now)
}

/// Write to shared debug log file (for cross-process log correlation)
private func ipcLog(_ message: String) {
    let timestamp = ts()
    let line = "[\(timestamp)] [SVC] \(message)\n"
    NSLog("[\(timestamp)] [SVC] %@", message)
    // Also write to shared file
    let logPath = "/tmp/actro_ipc_debug.log"
    if let handle = FileHandle(forWritingAtPath: logPath) {
        handle.seekToEndOfFile()
        if let data = line.data(using: .utf8) {
            handle.write(data)
        }
        handle.closeFile()
    } else {
        FileManager.default.createFile(atPath: logPath, contents: line.data(using: .utf8))
    }
}

private let debugLogPath: String = {
    let tempDir = NSTemporaryDirectory()
    return (tempDir as NSString).appendingPathComponent("actro-xpc-service.log")
}()

private func debugLog(_ message: String) {
    NSLog("[ActroViewService] %@", message)
    fputs("[\(ts())] [SVC] \(message)\n", stderr)
    fflush(stderr)

    let timestamp = ISO8601DateFormatter().string(from: Date())
    let logMessage = "[\(timestamp)] \(message)\n"
    if let data = logMessage.data(using: .utf8) {
        if FileManager.default.fileExists(atPath: debugLogPath) {
            if let handle = FileHandle(forWritingAtPath: debugLogPath) {
                handle.seekToEndOfFile()
                handle.write(data)
                handle.closeFile()
            }
        } else {
            try? data.write(to: URL(fileURLWithPath: debugLogPath))
        }
    }
}

// MARK: - Resizable Container View

/// Container view for each Tab's root view (RCTSurfaceHostingProxyRootView in bridgeless mode)
/// ViewBridge automatically syncs Host's NSRemoteView frame here
/// Key insight: ViewBridge updates the superview's bounds, so we use Auto Layout
/// to pin this view to the superview and automatically follow size changes.
private class ResizableContainerView: NSView {
    /// 使用 flipped 坐标系以匹配 Host 端 (ContainerView.isFlipped = true)
    /// 这确保子视图从左上角开始布局，而不是左下角
    override var isFlipped: Bool { true }

    private var rnView: NSView?
    var onSizeChanged: ((CGSize) -> Void)?
    private var cumulativeMagnification: CGFloat = 0
    private var superviewFrameObserver: NSObjectProtocol?
    private static let maxJailSize: NSSize = ResizableContainerView.computeMaximumJailSize()
    private var lastJailResizeRequest: NSSize = .zero
    private var isApplyingHostSize: Bool = false

    /// The tabId this container belongs to (for 1:1:1 mapping)
    var tabId: String?

    override var acceptsFirstResponder: Bool { true }

    // MARK: - Prevent NSView tracking loop from swallowing mouse events

    /// Default NSView.mouseDown enters a modal tracking loop that dequeues
    /// mouseUp from the event queue, preventing RCTTouchHandler (gesture
    /// recognizer on RCTRootContentView) from ever receiving mouseUp.
    /// This causes touchEnd to never be dispatched, breaking all onPress handlers.
    ///
    /// Fix: override mouseDown/mouseUp to do nothing. The gesture recognizer
    /// already receives mouse events independently through NSWindow's
    /// sendEvent: → gesture recognizer dispatch (which runs before the view
    /// hierarchy dispatch).
    override func mouseDown(with event: NSEvent) {
        // Do NOT call super — default NSView.mouseDown enters a tracking loop.
    }

    override func mouseUp(with event: NSEvent) {
        // Match mouseDown override — do NOT call super.
    }

    /// Setup view when added to hierarchy
    /// Container 固定在 (0,0)，不计算位置
    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()

        // Clean up old observer
        if let observer = superviewFrameObserver {
            NotificationCenter.default.removeObserver(observer)
            superviewFrameObserver = nil
        }

        guard superview != nil else { return }

        translatesAutoresizingMaskIntoConstraints = true
        autoresizingMask = []

        // 🔑 固定在 (0,0)，让 Host 端处理坐标转换
        super.setFrameOrigin(.zero)

        // 🔍 DEBUG: 使用蓝色背景来区分 Service 端 container
        // 如果看到蓝色 = container 大但 RN 内容小（RN 层问题）
        wantsLayer = true
        layer?.backgroundColor = NSColor.blue.cgColor  // DEBUG: 蓝色背景

        ipcLog("🔧 Container at (0,0), no position calculation")
    }

    override func setFrameSize(_ newSize: NSSize) {
        // Basic sanity: reject zero/negative or absurdly large sizes
        guard newSize.width > 0 && newSize.height > 0 else { return }
        guard newSize.width <= 16384 && newSize.height <= 16384 else { return }

        // Skip if size unchanged
        guard frame.size != newSize else { return }

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        super.setFrameSize(newSize)
        rnView?.frame = bounds
        CATransaction.commit()

        // macOS XPC: directly call layoutSubviews (window may be nil, so layout() won't)
        if let rnView = rnView {
            let sel = NSSelectorFromString("layoutSubviews")
            if rnView.responds(to: sel) {
                rnView.perform(sel)
            }
            for subview in rnView.subviews {
                if subview.responds(to: sel) {
                    subview.perform(sel)
                }
            }
        }
    }

    /// Reference to the ViewController for updating preferredContentSize dynamically
    weak var viewController: AnyObject?

    /// Container 固定在 (0, 0)
    /// 由于 jail.layer.isGeometryFlipped = true，(0,0) 就是视觉左上角
    private func syncOriginWithSuperview(_ superview: NSView) {
        // 不需要调整，isGeometryFlipped 已处理坐标系
    }

    /// Notify ViewBridge of desired content size via preferredContentSize.
    /// 🔑 同时直接设置 jail 的 frame 大小，使其与 Container 保持一致。
    /// 这样 Container 在 (0,0) 就能填满整个 jail，不需要计算位置。
    private func syncJailSize(superview: NSView, desiredSize: NSSize) {
        // Clamp to a safe upper bound
        let clamped = NSSize(
            width: min(desiredSize.width, Self.maxJailSize.width),
            height: min(desiredSize.height, Self.maxJailSize.height)
        )

        // 去重：同一尺寸不重复请求（0.5px 容差）
        guard abs(clamped.width - lastJailResizeRequest.width) > 0.5 ||
              abs(clamped.height - lastJailResizeRequest.height) > 0.5 else {
            return
        }
        lastJailResizeRequest = clamped

        // 更新 preferredContentSize
        if let vc = viewController {
            vc.setValue(NSValue(size: clamped), forKey: "preferredContentSize")
        }

        // 🔑 关键：直接设置 jail 的 frame 大小
        // 这样 jail 和 Container 大小一致，Container 在 (0,0) 就能正确显示
        // 不需要计算位置，避免抖动
        if abs(superview.frame.size.width - clamped.width) > 0.5 ||
           abs(superview.frame.size.height - clamped.height) > 0.5 {
            superview.setFrameSize(clamped)
        }
    }

    private static func computeMaximumJailSize() -> NSSize {
        // 🔑 XPC 服务在沙盒中运行，NSScreen.screens 可能返回受限的尺寸
        // (例如：1728x1117 而不是实际的显示器尺寸)
        // 使用足够大的默认值来支持 4K/5K 显示器和多显示器配置
        // 8192x8192 足够覆盖大多数使用场景，同时不会导致内存问题
        return NSSize(width: 8192, height: 8192)
    }

    /// Called when Host sends a valid size (via containerSizeChanged or tabConfig)
    /// This marks the container as ready to propagate sizes to children
    func applyHostSize(_ newSize: NSSize) {

        CATransaction.begin()
        CATransaction.setDisableActions(true)

        if let superview {
            isApplyingHostSize = true
            syncJailSize(superview: superview, desiredSize: newSize)
            isApplyingHostSize = false
        }

        // 设置 Container 尺寸（位置固定在 (0,0)）
        super.setFrameSize(newSize)

        // 🔑 "Frame Jitter" - 先设置不同尺寸再设置正确尺寸
        // 这可以强制 RN 视图重新布局，打破可能的缓存
        if let rnView = rnView {
            let jitterSize = CGSize(width: newSize.width - 1, height: newSize.height - 1)
            rnView.frame = NSRect(origin: .zero, size: jitterSize)
            rnView.frame = bounds
        }

        CATransaction.commit()

        // 🔑 macOS XPC: In XPC service, views have no NSWindow, so layout() skips layoutSubviews
        // (RCTUIKit.m: layout() { if (self.window != nil) { [self layoutSubviews]; } })
        // We must directly call layoutSubviews to trigger the Yoga recalculation chain:
        //   RCTSurfaceHostingProxyRootView.layoutSubviews → surface size update
        //   RCTRootContentView.layoutSubviews → updateAvailableSize → Yoga recalculation
        if let rnView = rnView {
            let sel = NSSelectorFromString("layoutSubviews")
            if rnView.responds(to: sel) {
                rnView.perform(sel)
            }
            for subview in rnView.subviews {
                subview.frame = rnView.bounds
                if subview.responds(to: sel) {
                    subview.perform(sel)
                }
            }
        }

        // 详细日志：检查整个视图层级的尺寸/位置
        let jailSize = superview?.bounds.size ?? .zero
        let jailFrame = superview?.frame ?? .zero
        let rnFrame = rnView?.frame ?? .zero
        let rnBounds = rnView?.bounds ?? .zero
        let containerFrame = self.frame
        let containerBounds = self.bounds

        // 检查 RN 子视图
        var rnSubviewInfo = ""
        if let rnView = rnView {
            for (i, subview) in rnView.subviews.enumerated() {
                rnSubviewInfo += " sub[\(i)]=\(Int(subview.frame.origin.x)),\(Int(subview.frame.origin.y)),\(Int(subview.frame.width))x\(Int(subview.frame.height))"
            }
        }

        ipcLog("📐 applyHostSize: container=(\(Int(containerFrame.origin.x)),\(Int(containerFrame.origin.y)),\(Int(containerFrame.width))x\(Int(containerFrame.height))) bounds=(\(Int(containerBounds.origin.x)),\(Int(containerBounds.origin.y)),\(Int(containerBounds.width))x\(Int(containerBounds.height))) jail=(\(Int(jailFrame.origin.x)),\(Int(jailFrame.origin.y)),\(Int(jailFrame.width))x\(Int(jailFrame.height))) rnView=(\(Int(rnFrame.origin.x)),\(Int(rnFrame.origin.y)),\(Int(rnFrame.width))x\(Int(rnFrame.height)))\(rnSubviewInfo)")

        // 🔍 深度检查 RN view 层级（检查 RCTSurfaceView 等内部视图）
        if let rnView = rnView {
            var depth2Info = ""
            for subview in rnView.subviews {
                for (j, sub2) in subview.subviews.enumerated() {
                    depth2Info += " d2[\(j)]=\(type(of: sub2)):\(Int(sub2.frame.width))x\(Int(sub2.frame.height))"
                }
            }
            if !depth2Info.isEmpty {
                ipcLog("📐 applyHostSize depth2:\(depth2Info)")
            }
        }

        // 🔍 延迟检查：查看是否有回调修改了尺寸
        let expectedSize = newSize
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            let actualContainerFrame = self.frame
            let actualRnFrame = self.rnView?.frame ?? .zero
            let actualJailFrame = self.superview?.frame ?? .zero
            if abs(actualContainerFrame.width - expectedSize.width) > 1 ||
               abs(actualContainerFrame.height - expectedSize.height) > 1 ||
               abs(actualRnFrame.width - expectedSize.width) > 1 ||
               abs(actualRnFrame.height - expectedSize.height) > 1 {
                ipcLog("⚠️ POST-applyHostSize: SIZE CHANGED! expected=\(Int(expectedSize.width))x\(Int(expectedSize.height)) actual.container=\(Int(actualContainerFrame.width))x\(Int(actualContainerFrame.height)) actual.rnView=\(Int(actualRnFrame.width))x\(Int(actualRnFrame.height)) actual.jail=\(Int(actualJailFrame.width))x\(Int(actualJailFrame.height))")
            }
        }

        if newSize.width > 0 && newSize.height > 0 {
            onSizeChanged?(newSize)
        }
    }

    func setRNView(_ view: NSView) {
        self.rnView = view
    }

    override func magnify(with event: NSEvent) {
        switch event.phase {
        case .began:
            cumulativeMagnification = 0
        case .changed:
            cumulativeMagnification += event.magnification
            if cumulativeMagnification < -0.3 {
                debugLog("🤏 Pinch threshold reached → posting pinchToOverview")
                cumulativeMagnification = 0
                DistributedNotificationCenter.default().postNotificationName(
                    NSNotification.Name("com.actro.pinchToOverview"),
                    object: nil,
                    userInfo: ["tabId": tabId ?? ""],
                    deliverImmediately: true
                )
            }
        case .ended, .cancelled:
            cumulativeMagnification = 0
        default:
            break
        }
    }

    deinit {
        if let observer = superviewFrameObserver {
            NotificationCenter.default.removeObserver(observer)
        }
    }
}

// MARK: - React Native Factory Delegate

private class ServiceReactNativeFactoryDelegate: RCTDefaultReactNativeFactoryDelegate {
    override init() {
        super.init()
        self.dependencyProvider = RCTAppDependencyProvider()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Bridgeless + TurboModule

    override func bridgelessEnabled() -> Bool { return true }
    override func turboModuleEnabled() -> Bool { return true }

    /// Lazily resolved host app bundle URL
    private lazy var _hostAppBundleURL: URL? = {
        // Bundle.main in XPC service may be the XPC bundle, not the host app bundle.
        if let url = Bundle.main.url(forResource: "main.rctnapp", withExtension: "jsbundle") {
            ipcLog("🔧 bundleURL: resolved via Bundle.main")
            return url
        }
        // Construct host app bundle path from executable path
        let execPath = ProcessInfo.processInfo.arguments[0]
        var dir = (execPath as NSString).deletingLastPathComponent
        dir = (dir as NSString).deletingLastPathComponent
        dir = (dir as NSString).deletingLastPathComponent
        dir = (dir as NSString).deletingLastPathComponent
        dir = (dir as NSString).deletingLastPathComponent
        let path = (dir as NSString).appendingPathComponent("Resources/main.rctnapp.jsbundle")
        if FileManager.default.fileExists(atPath: path) {
            ipcLog("🔧 bundleURL: resolved via host app path: \(path)")
            return URL(fileURLWithPath: path)
        }
        ipcLog("❌ bundleURL: not found! Bundle.main=\(Bundle.main.bundlePath) hostPath=\(path)")
        return nil
    }()

    override func bundleURL() -> URL? {
        let url = _hostAppBundleURL
        ipcLog("📎 bundleURL() called → \(url?.lastPathComponent ?? "nil") [thread=\(Thread.isMainThread ? "main" : "bg")]")
        return url
    }
}

// MARK: - Tab View Controller Entry

/// Represents a single Tab's ViewController and root view
/// Key component for 1:1:1 architecture
private class TabViewControllerEntry {
    /// The tabId from Host (assigned when first config is received)
    /// Initially nil until Host sends tabConfig with the real tabId
    var tabId: String?

    /// Temporary ID for internal tracking before real tabId is known
    let internalId: String

    weak var containerView: ResizableContainerView?
    /// Reference to the ViewController (runtime-created Objective-C class)
    weak var viewController: AnyObject?
    /// Root view — RCTSurfaceHostingProxyRootView (bridgeless mode)
    var rootView: NSView?
    var currentProperties: [String: Any] = [:]
    var containerSize: CGSize = .zero
    var pendingConfig: [String: Any]?

    /// Whether a pending size change was applied from containerSizeChanged notification
    var hasPendingSizeApplied: Bool = false

    /// Whether this entry is waiting for its first tabConfig from Host
    var isPending: Bool { tabId == nil }

    init(internalId: String, containerView: ResizableContainerView, viewController: AnyObject? = nil) {
        self.internalId = internalId
        self.containerView = containerView
        self.viewController = viewController
        containerView.tabId = internalId  // Use internal ID for logging initially
        containerView.viewController = viewController  // Enable dynamic preferredContentSize updates
        debugLog("[pending:\(internalId.prefix(8))] TabViewControllerEntry created (waiting for Host tabId)")
    }

    /// Assign the real tabId from Host
    func assignTabId(_ hostTabId: String) {
        self.tabId = hostTabId
        self.containerView?.tabId = hostTabId
        debugLog("[\(hostTabId.prefix(8))] TabId assigned (was pending:\(internalId.prefix(8)))")
    }

    deinit {
        debugLog("[\(tabId?.prefix(8) ?? internalId.prefix(8))] TabViewControllerEntry destroyed")
    }
}

// MARK: - Service React Native Host (Process Singleton)

/// Manages shared RCTHost and per-Tab root views (Bridgeless Mode)
/// Singleton per XPC service process
private class ServiceReactNativeHost {
    static let shared = ServiceReactNativeHost()

    private var factoryDelegate: ServiceReactNativeFactoryDelegate?
    private var factory: RCTReactNativeFactory?
    private var tabConfigObserver: NSObjectProtocol?
    private var hostCreated = false

    /// Cached RCTHost instance for event emission (bridgeless mode)
    private var _reactHost: AnyObject?
    private var reactHost: AnyObject? {
        if _reactHost == nil, let rootViewFactory = factory?.rootViewFactory {
            _reactHost = (rootViewFactory as NSObject).value(forKey: "reactHost") as AnyObject?
        }
        return _reactHost
    }

    /// Registry of TabViewControllerEntry by tabId (1:1:1 mapping)
    /// Key is the Host's tabId (Tab.id.uuidString)
    private var tabRegistry: [String: TabViewControllerEntry] = [:]

    /// Pending entries waiting for Host to send tabConfig with real tabId
    /// Key is the internal ID generated at loadView time
    private var pendingEntries: [String: TabViewControllerEntry] = [:]

    /// Order of pending entry creation (internalIds) - ensures FIFO assignment
    /// Dictionary doesn't preserve insertion order, so we need this array
    private var pendingEntryOrder: [String] = []

    /// Pending container size changes received before entry was registered
    /// Key is tabId, value is the pending size
    private var pendingSizeChanges: [String: CGSize] = [:]

    private let registryLock = NSLock()

    /// 🔑 跳帧机制：每个 tabId 的最新 size 请求序列号
    /// 当收到新的 size 通知时，只处理最新的，跳过中间的旧通知
    private var latestSizeSeq: [String: Int] = [:]
    /// 全局序列号计数器
    private var sizeSeqCounter: Int = 0

    /// This service's bundle identifier for notification routing
    let serviceBundleId: String

    private init() {
        // In XPC services, Bundle.main returns the host app's bundle, not the XPC service's bundle.
        // We need to get the XPC service's own bundle identifier from its bundle path.
        // The executable path format: .../RCTViewService0.xpc/Contents/MacOS/ActroViewService
        let executablePath = ProcessInfo.processInfo.arguments[0]
        var bundlePath = (executablePath as NSString).deletingLastPathComponent  // Remove executable name
        bundlePath = (bundlePath as NSString).deletingLastPathComponent  // Remove MacOS
        bundlePath = (bundlePath as NSString).deletingLastPathComponent  // Remove Contents -> now at .xpc bundle root
        if let serviceBundle = Bundle(path: bundlePath),
           let serviceBundleIdentifier = serviceBundle.bundleIdentifier {
            serviceBundleId = serviceBundleIdentifier
            debugLog("Initializing ServiceReactNativeHost, bundleId: \(serviceBundleId) (from: \(bundlePath))")
        } else {
            // Fallback - shouldn't happen in normal operation
            serviceBundleId = Bundle.main.bundleIdentifier ?? "unknown"
            debugLog("⚠️ Failed to get XPC service bundle ID, using fallback: \(serviceBundleId)")
        }

        // Create RCTReactNativeFactory FIRST — RN 0.81 feature flags must be
        // overridden before any other RN API (RCTRedBox, Inspector, etc.) accesses them.
        ensureHostCreated()

        #if DEBUG
        RCTRedBoxSetEnabled(false)
        // Disable default packager server access check (we use dynamic port, not 8081)
        // This prevents "Connection refused" errors to localhost:8081/status
        RCTBundleURLProviderAllowPackagerServerAccess(false)
        // Disable Inspector/Debugger connection to prevent localhost:8081 connection attempts
        RCTInspectorDevServerHelper.disableDebugger()
        #endif

        // Setup listeners
        setupTabConfigListener()

        // Start XPC listener for new IPC module
        startXPCListener()
    }

    /// Start the XPC listener for new IPC module
    /// This enables direct XPC communication alongside legacy DistributedNotification
    private func startXPCListener() {
        // Set ourselves as the delegate to receive XPC requests
        let listener = ServiceXPCListener.shared
        listener.requestHandler.delegate = self

        // Start listening (creates anonymous listener and exports endpoint)
        listener.start()

        debugLog("XPC listener started for service: \(serviceBundleId)")
    }

    /// Track whether JS bundle loaded successfully
    var bundleLoaded = false
    var bundleLoadFailed = false

    /// Create RCTHost via RCTReactNativeFactory (bridgeless mode)
    private func ensureHostCreated() {
        guard !hostCreated else {
            ipcLog("🔧 ensureHostCreated: already created, factory=\(factory != nil ? "OK" : "NIL")")
            return
        }
        hostCreated = true

        // Monitor ALL notifications to catch any RCT-related ones
        NotificationCenter.default.addObserver(forName: nil, object: nil, queue: nil) { [weak self] notification in
            let name = notification.name.rawValue
            if name.contains("RCT") || name.contains("React") || name.contains("JavaScript") || name.contains("Host") || name.contains("Bundle") {
                ipcLog("📡 [NOTIF] \(name)")
                if name.contains("DidLoadBundle") {
                    self?.bundleLoaded = true
                }
                if name.contains("FailToLoad") || name.contains("DidFailToLoad") {
                    self?.bundleLoadFailed = true
                    let error = notification.userInfo?["error"] ?? "?"
                    ipcLog("❌ [NOTIF] error: \(error)")
                }
            }
        }

        // Install error handlers BEFORE creating factory
        RCTSetFatalHandler { error in
            let nsError = error as NSError?
            ipcLog("💀 [FATAL] \(nsError?.localizedDescription ?? "(nil)") domain=\(nsError?.domain ?? "?") code=\(nsError?.code ?? 0)")
        }
        RCTAddLogFunction { level, source, fileName, lineNumber, message in
            let levelStr: String
            switch level.rawValue {
            case 0: levelStr = "TRACE"
            case 1: levelStr = "INFO"
            case 2: levelStr = "WARN"
            case 3: levelStr = "ERROR"
            case 4: levelStr = "FATAL"
            default: levelStr = "L\(level.rawValue)"
            }
            ipcLog("📋 [RCT:\(levelStr)] \(message ?? "(nil)") [\(fileName ?? "?"):\(lineNumber)]")
        }

        ipcLog("🔧 ensureHostCreated: Creating RCTHost (bridgeless mode)")
        factoryDelegate = ServiceReactNativeFactoryDelegate()
        factory = RCTReactNativeFactory(delegate: factoryDelegate!)
        ipcLog("🔧 ensureHostCreated: factory created = \(factory != nil)")

        // Log bundle URL
        if let url = factoryDelegate?.bundleURL() {
            ipcLog("🔧 bundleURL = \(url)")
        } else {
            ipcLog("❌ bundleURL is NIL!")
        }
    }

    /// Track which internalIds have already been bound to prevent duplicate bindings
    /// When multiple tabs share same XPC service, they may all try to bind the same connection
    private var boundInternalIds: Set<String> = []

    /// Bind host tabId to a specific pending entry (by internalId) to avoid FIFO 误绑
    func bind(hostTabId: String, toInternalId internalId: String) {
        NSLog("[XPC] 🔗 bind: hostTabId=%@ internalId=%@", String(hostTabId.prefix(8)), String(internalId.prefix(8)))

        var pendingSize: CGSize?

        registryLock.lock()

        // Log current state
        NSLog("[XPC] 🔗 bind state: pendingEntries=%d, tabRegistry=%d, boundInternalIds=%d",
              pendingEntries.count, tabRegistry.count, boundInternalIds.count)

        // Check if this internalId was already bound - prevent duplicate bindings
        // This can happen when multiple tabs share same XPC service and all send bindTab
        if boundInternalIds.contains(internalId) {
            NSLog("[XPC] 🔗 bind: internalId=%@ already bound, ignoring duplicate from hostTabId=%@",
                  String(internalId.prefix(8)), String(hostTabId.prefix(8)))
            registryLock.unlock()
            return
        }

        if let size = pendingSizeChanges.removeValue(forKey: hostTabId) {
            pendingSize = size
        }

        // Prefer pending entry that matches the connection internalId
        if let entry = pendingEntries.removeValue(forKey: internalId) {
            // Also remove from order array
            pendingEntryOrder.removeAll { $0 == internalId }
            NSLog("[XPC] 🔗 bind: found pending entry for internalId=%@, assigning hostTabId=%@",
                  String(internalId.prefix(8)), String(hostTabId.prefix(8)))
            entry.assignTabId(hostTabId)
            tabRegistry[hostTabId] = entry
            boundInternalIds.insert(internalId)  // Mark as bound
            registryLock.unlock()

            if let size = pendingSize {
                entry.containerSize = size
                entry.hasPendingSizeApplied = true
                entry.containerView?.applyHostSize(size)
                updateDimensionsForEntry(entry)
            }
            return
        }

        // Already registered? Update tabId on container for consistency
        if let entry = tabRegistry[hostTabId] {
            NSLog("[XPC] 🔗 bind: hostTabId=%@ already in tabRegistry", String(hostTabId.prefix(8)))
            registryLock.unlock()
            entry.assignTabId(hostTabId)
            if let size = pendingSize {
                entry.containerSize = size
                entry.hasPendingSizeApplied = true
                entry.containerView?.applyHostSize(size)
                updateDimensionsForEntry(entry)
            }
            return
        }

        NSLog("[XPC] ⚠️ bind: no entry found for internalId=%@ or hostTabId=%@",
              String(internalId.prefix(8)), String(hostTabId.prefix(8)))
        registryLock.unlock()
    }

    // MARK: - Tab Registry (1:1:1 Mapping)

    /// Register a new Tab's ViewController (initially as pending)
    /// The entry will be moved to tabRegistry when Host sends tabConfig with real tabId
    func registerPendingEntry(internalId: String, containerView: ResizableContainerView, viewController: AnyObject? = nil) -> TabViewControllerEntry {
        registryLock.lock()
        defer { registryLock.unlock() }

        let entry = TabViewControllerEntry(internalId: internalId, containerView: containerView, viewController: viewController)
        pendingEntries[internalId] = entry
        pendingEntryOrder.append(internalId)  // Track creation order for FIFO
        ipcLog("📝 registerPendingEntry: internalId=\(String(internalId.prefix(8))), pendingEntries=\(pendingEntries.count), pendingOrder=\(pendingEntryOrder.count)")
        return entry
    }

    /// Unregister a Tab when its ViewController is destroyed
    func unregisterEntry(internalId: String) {
        registryLock.lock()
        defer { registryLock.unlock() }

        // Try to find by internalId in pending
        if let entry = pendingEntries.removeValue(forKey: internalId) {
            // Also remove from order array
            pendingEntryOrder.removeAll { $0 == internalId }
            // Also remove from tabRegistry if it was assigned
            if let tabId = entry.tabId {
                tabRegistry.removeValue(forKey: tabId)
            }
            return
        }

        // Try to find in tabRegistry by checking internalId
        for (tabId, entry) in tabRegistry {
            if entry.internalId == internalId {
                tabRegistry.removeValue(forKey: tabId)
                return
            }
        }
    }

    /// Get entry for a specific tabId (Host's Tab.id)
    func getTabEntry(tabId: String) -> TabViewControllerEntry? {
        registryLock.lock()
        defer { registryLock.unlock() }
        return tabRegistry[tabId]
    }

    /// Try to bind a tabId to an existing pending entry (for late-arriving pendingTabId)
    /// Returns true if successfully bound, false if no pending entry available
    func tryBindPendingEntry(tabId: String) -> Bool {
        registryLock.lock()
        let hasPending = !pendingEntries.isEmpty
        registryLock.unlock()

        guard hasPending else { return false }
        return assignPendingEntry(tabId: tabId) != nil
    }

    /// Find the oldest pending entry and assign it the given tabId
    /// Returns the entry if successful, nil if no pending entries
    private func assignPendingEntry(tabId: String) -> TabViewControllerEntry? {
        registryLock.lock()

        let tabIdShort = String(tabId.prefix(8))
        // Log current state for debugging
        ipcLog("🔍 assignPendingEntry: tabId=\(tabIdShort), pendingEntries=\(pendingEntries.count), pendingOrder=\(pendingEntryOrder.count), tabRegistry=\(tabRegistry.count)")

        // If already registered, return existing
        if let existing = tabRegistry[tabId] {
            ipcLog("🔍 assignPendingEntry: already in tabRegistry")
            registryLock.unlock()
            return existing
        }

        // Find the oldest pending entry (FIFO) using order array
        guard let internalId = pendingEntryOrder.first,
              let entry = pendingEntries[internalId] else {
            ipcLog("⚠️ assignPendingEntry: pendingEntries is EMPTY!")
            registryLock.unlock()
            return nil
        }

        // Remove from pending (both dictionary and order array)
        pendingEntryOrder.removeFirst()
        pendingEntries.removeValue(forKey: internalId)

        // Assign real tabId and add to registry
        entry.assignTabId(tabId)
        tabRegistry[tabId] = entry

        // Check for any pending size changes for this tabId
        let pendingSize = pendingSizeChanges.removeValue(forKey: tabId)

        registryLock.unlock()

        // Apply pending size change if any (outside lock)
        if let size = pendingSize {
            entry.containerSize = size
            entry.hasPendingSizeApplied = true
            if let container = entry.containerView {
                container.applyHostSize(size)
            }
        }

        return entry
    }

    // MARK: - Tab Config IPC

    private var containerSizeObserver: NSObjectProtocol?
    private var bindTabObserver: NSObjectProtocol?
    private var uiStateChangedObserver: NSObjectProtocol?
    private var pendingTabIdObserver: NSObjectProtocol?
    private var readyNotificationTimer: Timer?
    private var hasReceivedTabConfig = false

    /// 主进程的 PID，用于进程隔离通知
    /// 从 pendingTabId 通知中获取，确保只处理来自正确主进程的通知
    private(set) var hostProcessId: Int32?

    /// Queue of pending tabIds sent by Host before connection request
    /// Used to associate each loadView call with the correct Host tabId
    private var pendingTabIdQueue: [String] = []
    private let pendingTabIdLock = NSLock()

    /// Queue of tab configs that arrived before their pending entries were created
    /// When loadView creates a pending entry, it dequeues the first config and applies it (FIFO)
    private var pendingTabConfigQueue: [[String: Any]] = []
    private let pendingTabConfigQueueLock = NSLock()

    /// Get the next pending tabId from the queue (FIFO)
    /// Waits briefly for pendingTabId notification to arrive if queue is empty
    /// 尝试从队列中获取 pendingTabId（不再等待）
    /// 新架构下，tabId 主要通过 bootstrap tabConfig 传递，这个方法只是辅助机制
    /// 用于处理 pendingTabId 比 loadView 更早到达的情况
    func dequeueNextPendingTabId() -> String? {
        pendingTabIdLock.lock()
        defer { pendingTabIdLock.unlock() }

        if !pendingTabIdQueue.isEmpty {
            let tabId = pendingTabIdQueue.removeFirst()
            NSLog("[XPC] ✅ dequeueNextPendingTabId: found queued tabId")
            return tabId
        }

        // 不再等待 - tabId 将通过 bootstrap tabConfig 传递
        // 如果队列为空，返回 nil，entry 将保持 pending 状态
        // 后续收到 tabConfig 时会通过 assignPendingEntry 绑定
        return nil
    }

    private func setupTabConfigListener() {
        // Listen for tab config with service-specific notification
        let notificationName = "com.actro.tabConfig.\(serviceBundleId)"
        ipcLog("🎧 Setting up tabConfig listener: \(notificationName)")
        tabConfigObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(notificationName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            ipcLog("📬 Received notification on: \(notificationName)")
            self?.handleTabConfigNotification(notification)
        }

        // Listen for container size changes from Host (service-specific notification)
        let containerSizeName = "com.actro.containerSizeChanged.\(serviceBundleId)"
        containerSizeObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(containerSizeName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleContainerSizeChanged(notification)
        }

        // Listen for explicit tabId binding from Host (connection-aware)
        let bindName = "com.actro.bindTab.\(serviceBundleId)"
        bindTabObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(bindName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let info = notification.userInfo,
                  let tabId = info["tabId"] as? String,
                  let internalId = info["internalId"] as? String else { return }
            self?.bind(hostTabId: tabId, toInternalId: internalId)
        }

        // Listen for pending tabId from Host (sent before connection request)
        // 同时提取 hostProcessId 用于进程隔离
        let pendingTabIdName = "com.actro.pendingTabId.\(serviceBundleId)"
        pendingTabIdObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(pendingTabIdName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self = self,
                  let userInfo = notification.userInfo,
                  let tabId = userInfo["tabId"] as? String else { return }

            // 提取 hostProcessId（第一次收到时设置，后续忽略不同 PID 的通知）
            if let pid = userInfo["hostProcessId"] as? Int32 {
                if self.hostProcessId == nil {
                    self.hostProcessId = pid
                    NSLog("[XPC] 📌 hostProcessId set to %d", pid)
                    // Set hostProcessId on XPC listener for state reporting
                    ServiceXPCListener.shared.setHostProcessId(pid)
                    // 收到 hostProcessId 后，重新设置带 PID 的通知监听器
                    self.setupProcessIsolatedListeners()
                } else if self.hostProcessId != pid {
                    // 忽略来自其他主进程的通知
                    return
                }
            }

            // First try to bind to an existing pending entry (handles late arrival)
            // If loadView already executed but didn't get tabId, this binds it now
            if self.tryBindPendingEntry(tabId: tabId) {
                NSLog("[XPC] ✅ pendingTabId %@ bound to existing pending entry", String(tabId.prefix(8)))
                return
            }

            // No pending entry yet - add to queue for upcoming loadView to consume
            self.pendingTabIdLock.lock()
            self.pendingTabIdQueue.append(tabId)
            self.pendingTabIdLock.unlock()
            NSLog("[XPC] 📥 pendingTabId %@ queued for future loadView", String(tabId.prefix(8)))
        }

        // Notify Host that this service is ready (with retry until tab config received)
        postXPCServiceReady()
        startReadyNotificationRetry()
    }

    /// 设置带进程隔离的通知监听器（在收到 hostProcessId 后调用）
    private func setupProcessIsolatedListeners() {
        guard let pid = hostProcessId else { return }

        // 移除旧的不带 PID 的监听器
        if let observer = tabConfigObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
        }
        if let observer = containerSizeObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
        }
        if let observer = bindTabObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
        }
        if let observer = uiStateChangedObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
        }

        // 使用带 PID 的通知名称重新设置监听器
        let tabConfigName = "com.actro.tabConfig.\(pid).\(serviceBundleId)"
        tabConfigObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(tabConfigName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleTabConfigNotification(notification)
        }

        let containerSizeName = "com.actro.containerSizeChanged.\(pid).\(serviceBundleId)"
        ipcLog("🎧 Setting up containerSizeChanged listener: \(containerSizeName)")
        containerSizeObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(containerSizeName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleContainerSizeChanged(notification)
        }

        let bindTabName = "com.actro.bindTab.\(pid).\(serviceBundleId)"
        bindTabObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(bindTabName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let info = notification.userInfo,
                  let tabId = info["tabId"] as? String,
                  let internalId = info["internalId"] as? String else { return }
            self?.bind(hostTabId: tabId, toInternalId: internalId)
        }

        // Listen for UI state changes (e.g. left panel visibility toggle)
        let uiStateName = "com.actro.uiStateChanged.\(pid).\(serviceBundleId)"
        uiStateChangedObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(uiStateName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleUIStateChanged(notification)
        }

        NSLog("[XPC] 📡 Process-isolated listeners setup for pid=%d", pid)

        // 🔑 通知 Host 监听器已设置好，请求重新发送 containerSizeChanged
        // 这解决了竞态条件：Host 可能在监听器设置好之前就发送了 containerSizeChanged
        let listenerReadyName = "com.actro.listenerReady.\(pid).\(serviceBundleId)"
        ipcLog("📤 Sending listenerReady notification: \(listenerReadyName)")
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(listenerReadyName),
            object: nil,
            userInfo: ["serviceId": serviceBundleId],
            deliverImmediately: true
        )
    }

    private func handleContainerSizeChanged(_ notification: Notification) {
        // Parse JSON string format (for cross-process compatibility)
        guard let userInfo = notification.userInfo,
              let jsonString = userInfo["sizeInfo"] as? String,
              let jsonData = jsonString.data(using: .utf8),
              let sizeInfo = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any],
              let tabId = sizeInfo["tabId"] as? String,
              let width = sizeInfo["width"] as? Double,
              let height = sizeInfo["height"] as? Double else {
            ipcLog("⚠️ handleContainerSizeChanged: failed to parse notification")
            return
        }

        // Extract requestId for log correlation
        let requestId = sizeInfo["requestId"] as? String ?? "?"
        // Extract isOverviewMode for compact view in system apps
        let isOverviewMode = sizeInfo["isOverviewMode"] as? Bool ?? false

        // 🔑 使用 floor() 取整，与 Host 端保持一致
        let newSize = CGSize(width: floor(width), height: floor(height))

        // 🔑 跳帧机制：记录最新的 size 请求序列号
        // 这样即使多个通知快速到达，我们也只处理最新的
        sizeSeqCounter += 1
        let mySeq = sizeSeqCounter
        latestSizeSeq[tabId] = mySeq

        ipcLog("📥 containerSizeChanged: seq=\(mySeq) req=\(requestId) tab=\(tabId.prefix(8)) size=\(Int(newSize.width))x\(Int(newSize.height))")

        // 🔑 延迟一个 run loop 周期，让后续通知有机会到达
        // 这样如果有更新的通知，当前这个就会被跳过
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }

            // 🔑 检查是否仍然是最新的请求
            // 如果有更新的请求到达，跳过当前处理（跳帧）
            if let latestSeq = self.latestSizeSeq[tabId], latestSeq != mySeq {
                ipcLog("⏭️ SKIP FRAME: seq=\(mySeq) (latest=\(latestSeq)) tab=\(tabId.prefix(8)) size=\(Int(newSize.width))x\(Int(newSize.height))")
                return
            }

            // 去重：相同尺寸且 isOverviewMode 未变化时不重复处理
            if let entry = self.getTabEntry(tabId: tabId) {
                let currentOverviewMode = entry.currentProperties["isOverviewMode"] as? Bool ?? false
                let sizeUnchanged = entry.containerSize == newSize
                let overviewUnchanged = currentOverviewMode == isOverviewMode

                if sizeUnchanged && overviewUnchanged {
                    ipcLog("📐 SKIP (same): seq=\(mySeq) tab=\(tabId.prefix(8)) size=\(Int(newSize.width))x\(Int(newSize.height))")
                    return
                }
            }

            ipcLog("📐 APPLY: seq=\(mySeq) req=\(requestId) tab=\(tabId.prefix(8)) size=\(Int(newSize.width))x\(Int(newSize.height))")
            self.applySizeUpdate(tabId: tabId, size: newSize, requestId: requestId, isOverviewMode: isOverviewMode)
        }
    }

    /// Handle UI state changes from Host (e.g. left panel visibility toggle)
    private func handleUIStateChanged(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let jsonString = userInfo["payload"] as? String,
              let jsonData = jsonString.data(using: .utf8),
              let payload = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any],
              let tabId = payload["tabId"] as? String else {
            ipcLog("⚠️ handleUIStateChanged: failed to parse notification")
            return
        }

        ipcLog("📥 uiStateChanged: tab=\(tabId.prefix(8)) payload=\(payload)")

        guard let entry = getTabEntry(tabId: tabId) else {
            ipcLog("⚠️ handleUIStateChanged: no entry for tab=\(tabId.prefix(8))")
            return
        }

        // Update entry properties with changed values
        var properties = entry.currentProperties
        var changed = false

        if let isLeftPanelVisible = payload["isLeftPanelVisible"] as? Bool {
            let current = properties["isLeftPanelVisible"] as? Bool ?? true
            if current != isLeftPanelVisible {
                properties["isLeftPanelVisible"] = isLeftPanelVisible
                changed = true
                ipcLog("📤 uiStateChanged: tab=\(tabId.prefix(8)) isLeftPanelVisible \(current) → \(isLeftPanelVisible)")
            }
        }

        guard changed else { return }

        entry.currentProperties = properties
        setAppProperties(properties, on: entry.rootView)

        // Emit props update event to RN (more reliable than appProperties)
        emitPropsUpdate(for: entry)
    }

    /// 应用尺寸更新（同步，无延迟）
    private func applySizeUpdate(tabId: String, size: CGSize, requestId: String = "?", isOverviewMode: Bool? = nil) {
        if let entry = getTabEntry(tabId: tabId) {
            // 检查尺寸或 isOverviewMode 是否真的变化了
            let sizeChanged = entry.containerSize != size
            let currentOverviewMode = entry.currentProperties["isOverviewMode"] as? Bool ?? false
            let overviewChanged = isOverviewMode != nil && currentOverviewMode != isOverviewMode

            guard sizeChanged || overviewChanged else { return }

            // 🔑 关键：立即更新 entry.containerSize，防止竞态条件
            // 如果等待 applyHostSize 的回调链来更新，后续快速到达的通知
            // 可能在回调执行前检查状态，导致错误的 "same size" 判断
            entry.containerSize = size

            // Update props
            var properties = entry.currentProperties
            if let isOverview = isOverviewMode {
                properties["isOverviewMode"] = isOverview
            }
            properties["containerWidth"] = size.width
            properties["containerHeight"] = size.height
            entry.currentProperties = properties

            // 🔑 不重建 root view：避免 Focus↔Overview 时丢失 JS 状态（例如 counter 值）
            // 通过 appProperties + DeviceEventEmitter 强制刷新 props，保证 RN 能感知变化。
            if entry.rootView == nil, let container = entry.containerView {
                if let newRootView = ServiceReactNativeHost.shared.createRootView(for: entry, initialProperties: properties) {
                    entry.rootView = newRootView
                    newRootView.frame = CGRect(origin: .zero, size: size)
                    container.addSubview(newRootView)
                    container.setRNView(newRootView)
                }
            }

            setAppProperties(properties, on: entry.rootView)
            emitPropsUpdate(for: entry)
            ipcLog("📱 Updated RN props: tab=\(String(tabId.prefix(8))) isOverviewMode=\(isOverviewMode ?? false) size=\(Int(size.width))x\(Int(size.height))")

            // 先更新 viewController 的 preferredContentSize
            // 这会让 ViewBridge 知道我们需要更大的 jail view
            if let vc = entry.viewController {
                vc.setValue(NSValue(size: size), forKey: "preferredContentSize")
            }

            if let container = entry.containerView {
                // applyHostSize → setFrameSize → onSizeChanged → updateContainerSize
                container.applyHostSize(size)
            }

            // 🔑 关键：更新 RN rootView 的 frame 并强制 Yoga 重新布局
            // applyHostSize 已经设置了 rnView.frame 并直接调用 layoutSubviews，
            // 但作为安全措施，确保 rootView 也被正确更新
            if let rootView = entry.rootView {
                let oldFrame = rootView.frame
                rootView.frame = CGRect(origin: .zero, size: size)
                // 如果 frame 确实变了，直接调用 layoutSubviews（XPC 中 window 为 nil，layout() 不会调用 layoutSubviews）
                if oldFrame.size != size {
                    let sel = NSSelectorFromString("layoutSubviews")
                    if rootView.responds(to: sel) {
                        rootView.perform(sel)
                    }
                    for subview in rootView.subviews {
                        if subview.responds(to: sel) {
                            subview.perform(sel)
                        }
                    }
                }
                ipcLog("📱 Updated RN rootView.frame + layoutSubviews: tab=\(String(tabId.prefix(8))) size=\(Int(size.width))x\(Int(size.height))")
            }

            // 🔑 通知 RN 布局系统尺寸变化（发送 didUpdateDimensions 事件）
            ServiceReactNativeHost.shared.updateDimensionsForEntry(entry)

            // 🔑 通过 DeviceEventEmitter 发送 props 更新事件（appProperties 可能不可靠）
            ServiceReactNativeHost.shared.emitPropsUpdate(for: entry)

            // 通知 Host 尺寸更新完成
            postSizeApplied(tabId: tabId, size: size, requestId: requestId)
        } else {
            // Entry not yet registered - cache for later
            registryLock.lock()
            pendingSizeChanges[tabId] = size
            registryLock.unlock()
        }
    }

    /// 通知 Host 端尺寸更新已完成
    private func postSizeApplied(tabId: String, size: CGSize, requestId: String = "?") {
        let notificationName: String
        if let pid = hostProcessId {
            notificationName = "com.actro.sizeApplied.\(pid).\(serviceBundleId)"
        } else {
            notificationName = "com.actro.sizeApplied.\(serviceBundleId)"
        }
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: [
                "tabId": tabId,
                "width": size.width,
                "height": size.height,
                "requestId": requestId
            ],
            deliverImmediately: true
        )
    }

    private func postXPCServiceReady() {
        // 包含 hostProcessId（如果已设置），用于主进程过滤
        var userInfo: [String: Any] = ["bundleId": serviceBundleId]
        if let pid = hostProcessId {
            userInfo["hostProcessId"] = pid
        }
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.actro.xpcServiceReady"),
            object: nil,
            userInfo: userInfo,
            deliverImmediately: true
        )
    }

    /// Retry sending xpcServiceReady every 500ms until first tab config is received
    /// This handles race condition where Host hasn't set up listener yet
    private func startReadyNotificationRetry() {
        readyNotificationTimer?.invalidate()
        readyNotificationTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] timer in
            guard let self = self else {
                timer.invalidate()
                return
            }
            if self.hasReceivedTabConfig {
                timer.invalidate()
                self.readyNotificationTimer = nil
                NSLog("[XPC] 📤 Stopped xpcServiceReady retry (config received)")
            } else {
                self.postXPCServiceReady()
            }
        }
    }

    private func handleTabConfigNotification(_ notification: Notification) {
        ipcLog("📥 handleTabConfigNotification received")

        guard let userInfo = notification.userInfo else {
            ipcLog("⚠️ handleTabConfigNotification: userInfo is nil")
            return
        }

        guard let configJson = userInfo["config"] as? String else {
            let keysStr = userInfo.keys.map { String(describing: $0) }.joined(separator: ", ")
            ipcLog("⚠️ handleTabConfigNotification: config not found, keys: \(keysStr)")
            return
        }

        guard let configData = configJson.data(using: .utf8),
              let config = try? JSONSerialization.jsonObject(with: configData) as? [String: Any] else {
            ipcLog("⚠️ handleTabConfigNotification: failed to parse JSON")
            return
        }

        guard let tabId = config["tabId"] as? String else {
            ipcLog("⚠️ handleTabConfigNotification: tabId not found")
            return
        }

        // 🔑 从 bootstrap tabConfig 中提取 hostProcessId（替代之前的 pendingTabId 机制）
        // 这样即使 pendingTabId 没有发送，也能从第一个 tabConfig 中获取 hostProcessId
        if let pid = config["hostProcessId"] as? Int32 {
            if self.hostProcessId == nil {
                self.hostProcessId = pid
                ipcLog("📌 hostProcessId set to \(pid) (from tabConfig)")
                // Set hostProcessId on XPC listener for state reporting
                ServiceXPCListener.shared.setHostProcessId(pid)
                // 设置带 PID 的通知监听器
                self.setupProcessIsolatedListeners()
            } else if self.hostProcessId != pid {
                // 忽略来自其他主进程的通知
                ipcLog("⚠️ Ignoring tabConfig from different host: \(pid) (current: \(self.hostProcessId!))")
                return
            }
        }

        let systemAppType = config["systemAppType"] as? String ?? "nil"
        let isBootstrap = config["isBootstrap"] as? Bool ?? false
        let isOverviewMode = config["isOverviewMode"] as? Bool ?? false
        let tabIdShort = String(tabId.prefix(8))
        ipcLog("📥 tabConfig tab=\(tabIdShort) systemAppType=\(systemAppType) isBootstrap=\(isBootstrap) isOverviewMode=\(isOverviewMode)")

        // Stop retry timer once we receive first config
        hasReceivedTabConfig = true

        // Try to find existing entry, or assign a pending entry
        let entry: TabViewControllerEntry?
        if let existing = getTabEntry(tabId: tabId) {
            ipcLog("✅ Found existing entry for tab=\(tabIdShort)")
            entry = existing
        } else {
            // First config for this tabId - assign a pending entry
            entry = assignPendingEntry(tabId: tabId)
            if entry != nil {
                ipcLog("✅ Assigned pending entry for tab=\(tabIdShort)")
            } else {
                ipcLog("⚠️ No pending entry for tab=\(tabIdShort) (pendingEntries empty)")
            }
        }

        if let entry = entry {
            applyConfigToEntry(entry, config: config)
        } else {
            // Cache config for later - loadView will apply it when entry is created
            ipcLog("📦 Queueing config for tab=\(tabIdShort) (will apply when entry created)")
            pendingTabConfigQueueLock.lock()
            pendingTabConfigQueue.append(config)
            pendingTabConfigQueueLock.unlock()
        }
    }

    /// Dequeue the next pending tab config (FIFO)
    /// Called by loadView after creating a pending entry
    private func dequeueNextPendingTabConfig() -> [String: Any]? {
        pendingTabConfigQueueLock.lock()
        defer { pendingTabConfigQueueLock.unlock() }

        if !pendingTabConfigQueue.isEmpty {
            let config = pendingTabConfigQueue.removeFirst()
            if let tabId = config["tabId"] as? String {
                NSLog("[XPC] 📦 Dequeued pending config for tab=%@", String(tabId.prefix(8)))
            }
            return config
        }
        return nil
    }

    /// Try to apply a pending config to a newly created entry
    /// Called by loadView after creating a pending entry
    func tryApplyPendingConfig(for entry: TabViewControllerEntry, internalId: String) {
        guard let cachedConfig = dequeueNextPendingTabConfig(),
              let tabId = cachedConfig["tabId"] as? String else {
            return
        }

        NSLog("[XPC] 📦 Applying cached config for tab=%@ to entry=%@",
              String(tabId.prefix(8)), String(internalId.prefix(8)))

        // Assign tabId to entry and move to tabRegistry
        entry.assignTabId(tabId)
        registryLock.lock()
        pendingEntries.removeValue(forKey: internalId)
        pendingEntryOrder.removeAll { $0 == internalId }  // Also remove from order array
        tabRegistry[tabId] = entry
        registryLock.unlock()

        // Apply the cached config
        applyConfigToEntry(entry, config: cachedConfig)
    }

    private func applyConfigToEntry(_ entry: TabViewControllerEntry, config: [String: Any]) {
        let tabIdShort = String((config["tabId"] as? String ?? entry.internalId).prefix(8))
        ipcLog("🎨 applyConfigToEntry: tab=\(tabIdShort), hasRootView=\(entry.rootView != nil), hasContainerView=\(entry.containerView != nil)")

        // If root view not ready yet, create it now (event-driven)
        guard entry.rootView != nil else {
            createRootViewWithConfig(for: entry, config: config)
            return
        }

        // Use Host's tabId (should be assigned by now)
        let tabId = entry.tabId ?? entry.internalId

        // Build tab properties
        var tabDict: [String: Any] = [
            "id": tabId,
            "title": config["title"] as? String ?? "Untitled",
            "backgroundColor": config["backgroundColor"] as? String ?? "#1f1f1f"
        ]

        // System app type
        if let systemAppType = config["systemAppType"] as? String {
            tabDict["systemAppType"] = systemAppType
        }

        // Scripts
        if let s = config["unifiedScript"] as? String { tabDict["unifiedScript"] = s }
        if let s = config["leftPanelScript"] as? String { tabDict["leftPanelScript"] = s }
        if let s = config["rightPanelScript"] as? String { tabDict["rightPanelScript"] = s }
        if let s = config["guestScript"] as? String { tabDict["guestScript"] = s }
        if let reloadKey = config["reloadKey"] { tabDict["reloadKey"] = reloadKey }

        // System app data
        if let bookmarks = config["bookmarks"] { tabDict["bookmarks"] = bookmarks }
        if let recentHistory = config["recentHistory"] { tabDict["recentHistory"] = recentHistory }
        if let historyGroups = config["historyGroups"] { tabDict["historyGroups"] = historyGroups }
        if let historyItems = config["historyItems"] { tabDict["historyItems"] = historyItems }

        // Update container size
        if let width = config["containerWidth"] as? Double,
           let height = config["containerHeight"] as? Double {
            entry.containerSize = CGSize(width: width, height: height)
        }

        // Build full properties
        let isOverviewMode = config["isOverviewMode"] as? Bool ?? false
        let properties: [String: Any] = [
            "singleTabMode": true,
            "tab": tabDict,
            "currentTabId": tabId,
            "isLeftPanelVisible": config["leftPanelVisible"] as? Bool ?? true,
            "isRightPanelVisible": config["rightPanelVisible"] as? Bool ?? true,
            "containerWidth": entry.containerSize.width,
            "containerHeight": entry.containerSize.height,
            "isOverviewMode": isOverviewMode
        ]

        entry.currentProperties = properties

        if entry.rootView != nil {
            setAppProperties(properties, on: entry.rootView)

            // RN bridge may still be loading, re-apply properties multiple times to ensure render
            for delay in [0.3, 0.8, 1.5] {
                DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self, weak entry] in
                    guard let self = self, let entry = entry else { return }
                    self.setAppProperties(entry.currentProperties, on: entry.rootView)
                }
            }
        }

        // Update RN dimensions
        updateDimensionsForEntry(entry)
    }

    // MARK: - Root View Creation (Bridgeless Mode)

    /// Create a new root view for a specific Tab
    /// Returns RCTSurfaceHostingProxyRootView (bridgeless mode)
    func createRootView(for entry: TabViewControllerEntry, initialProperties: [String: Any]) -> NSView? {
        ensureHostCreated()

        guard let factory = factory else {
            ipcLog("⚠️ createRootView: factory is nil!")
            return nil
        }

        var properties = initialProperties
        properties["containerWidth"] = entry.containerSize.width
        properties["containerHeight"] = entry.containerSize.height
        entry.currentProperties = properties

        ipcLog("📦 createRootView: calling rootViewFactory.view with moduleName=ActrApp")
        let view = factory.rootViewFactory.view(
            withModuleName: "ActrApp",
            initialProperties: properties,
            launchOptions: nil
        )

        ipcLog("📦 createRootView: got \(type(of: view)) frame=\(view.frame)")

        if view.responds(to: NSSelectorFromString("setSizeFlexibility:")) {
            view.setValue(0, forKey: "sizeFlexibility")  // 0 = .none
        }
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor.clear.cgColor
        entry.rootView = view
        return view
    }

    /// Update container size for a specific Tab
    func updateContainerSize(for entry: TabViewControllerEntry, size: CGSize) {
        guard size.width > 0 && size.height > 0 else { return }
        // 防止重复更新相同尺寸
        guard entry.containerSize != size else { return }

        entry.containerSize = size

        var properties = entry.currentProperties
        properties["containerWidth"] = size.width
        properties["containerHeight"] = size.height
        entry.currentProperties = properties

        setAppProperties(properties, on: entry.rootView)
        updateDimensionsForEntry(entry)
    }

    /// Set appProperties on root view (bridgeless mode uses RCTSurfaceHostingProxyRootView)
    private func setAppProperties(_ properties: [String: Any], on view: NSView?) {
        guard let view = view, view.responds(to: NSSelectorFromString("setAppProperties:")) else { return }
        view.setValue(properties, forKey: "appProperties")
    }

    /// Emit DeviceEventEmitter event via RCTHost (bridgeless mode)
    private func emitDeviceEvent(_ name: String, body: Any) {
        guard let host = reactHost else {
            ipcLog("⚠️ emitDeviceEvent: reactHost not available")
            return
        }
        let args = [name, body] as NSArray
        RCTHostCallFunction(host, "RCTDeviceEventEmitter" as NSString, "emit" as NSString, args)
    }

    func updateDimensionsForEntry(_ entry: TabViewControllerEntry) {
        let size = entry.containerSize
        let scale = NSScreen.main?.backingScaleFactor ?? 2.0

        let dims: [String: Any] = [
            "width": size.width,
            "height": size.height,
            "scale": scale,
            "fontScale": 1.0
        ]

        let dimensions: [String: Any] = [
            "window": dims,
            "screen": dims
        ]
        emitDeviceEvent("didUpdateDimensions", body: dimensions)
    }

    /// 通过 DeviceEventEmitter 发送 props 更新事件
    func emitPropsUpdate(for entry: TabViewControllerEntry) {
        let props = entry.currentProperties
        let tabId = props["tab"].flatMap { ($0 as? [String: Any])?["id"] as? String } ?? "unknown"
        let isOverviewMode = props["isOverviewMode"] as? Bool ?? false

        ipcLog("📤 emitPropsUpdate: tab=\(String(tabId.prefix(8))) isOverviewMode=\(isOverviewMode)")
        emitDeviceEvent("onPropsUpdate", body: props)
    }
}

// MARK: - ServiceRequestHandlerDelegate

extension ServiceReactNativeHost: ServiceRequestHandlerDelegate {
    func handleTabConfig(_ config: [String: Any], completion: @escaping (Bool, String?) -> Void) {
        // Forward to existing handleTabConfig logic
        guard let tabId = config["tabId"] as? String else {
            completion(false, "Missing tabId")
            return
        }

        let tabIdShort = String(tabId.prefix(8))
        debugLog("[\(tabIdShort)] handleTabConfig via XPC delegate")

        // Stop retry timer once we receive first config
        hasReceivedTabConfig = true

        // Try to find existing entry, or assign a pending entry
        let entry: TabViewControllerEntry?
        if let existing = getTabEntry(tabId: tabId) {
            debugLog("[XPC] ✅ Found existing entry for tab=\(tabIdShort)")
            entry = existing
        } else {
            // First config for this tabId - assign a pending entry
            entry = assignPendingEntry(tabId: tabId)
            if entry != nil {
                debugLog("[XPC] ✅ Assigned pending entry for tab=\(tabIdShort)")
            } else {
                debugLog("[XPC] ⚠️ No pending entry available for tab=\(tabIdShort)")
            }
        }

        if let entry = entry {
            applyConfigToEntry(entry, config: config)
            completion(true, nil)
        } else {
            completion(false, "No entry for tab")
        }
    }

    func handleContainerSizeChanged(
        tabId: String,
        size: CGSize,
        requestId: Int,
        isOverviewMode: Bool,
        completion: @escaping (Bool, String?) -> Void
    ) {
        let tabIdShort = String(tabId.prefix(8))
        debugLog("[\(tabIdShort)] handleContainerSizeChanged via XPC delegate: \(size.width)x\(size.height)")

        // Use existing size update logic
        applySizeUpdate(tabId: tabId, size: size, requestId: "\(requestId)", isOverviewMode: isOverviewMode)
        completion(true, nil)
    }

    func handleBindTab(hostTabId: String, internalId: String, completion: @escaping (Bool) -> Void) {
        let hostShort = String(hostTabId.prefix(8))
        let internalShort = String(internalId.prefix(8))
        debugLog("[XPC] handleBindTab via delegate: host=\(hostShort) internal=\(internalShort)")

        // Use existing bind logic
        bind(hostTabId: hostTabId, toInternalId: internalId)
        completion(true)
    }
}

// MARK: - Dynamic Class Registration

@_cdecl("registerActroServiceViewController")
public func registerActroServiceViewController() {
    debugLog("Registering ActroServiceViewController class...")

    let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"
    guard dlopen(frameworkPath, RTLD_NOW) != nil else {
        debugLog("Failed to load ViewBridge")
        return
    }

    guard let superclass = NSClassFromString("NSServiceViewController") else {
        debugLog("NSServiceViewController not found")
        return
    }

    if NSClassFromString("ActroServiceViewController") != nil {
        debugLog("Already registered")
        return
    }

    guard let newClass = objc_allocateClassPair(superclass, "ActroServiceViewController", 0) else {
        debugLog("Failed to allocate class pair")
        return
    }

    // Add instance variables to store connection/internalId and host tabId
    class_addIvar(newClass, "_tabId", MemoryLayout<NSString>.size, UInt8(log2(Double(MemoryLayout<NSString>.alignment))), "@")  // internalId
    class_addIvar(newClass, "_hostTabId", MemoryLayout<NSString>.size, UInt8(log2(Double(MemoryLayout<NSString>.alignment))), "@")

    guard let tabIdIvar = class_getInstanceVariable(newClass, "_tabId"),
          let hostTabIdIvar = class_getInstanceVariable(newClass, "_hostTabId") else {
        debugLog("⚠️ Failed to get ivars for ActroServiceViewController")
        return
    }

    let logId: (AnyObject) -> String = { self_ in
        if let host = object_getIvar(self_, hostTabIdIvar) as? NSString {
            return host as String
        }
        if let internalId = object_getIvar(self_, tabIdIvar) as? NSString {
            return internalId as String
        }
        return "?"
    }

    // loadView - creates React Native view for this Tab
    let loadViewBlock: @convention(block) (AnyObject) -> Void = { self_ in
        // Generate internal ID for tracking (Host will send the real tabId later)
        let internalId = UUID().uuidString
        debugLog("[pending:\(internalId.prefix(8))] loadView called")

        // Store internal ID in instance variable (for cleanup on dealloc)
        object_setIvar(self_, tabIdIvar, internalId as NSString)
        object_setIvar(self_, hostTabIdIvar, nil)

        // Initialize shared RCTHost
        let host = ServiceReactNativeHost.shared

        let initialWidth: CGFloat = 1280
        let initialHeight: CGFloat = 720

        let containerView = ResizableContainerView(frame: NSRect(x: 0, y: 0, width: initialWidth, height: initialHeight))
        containerView.wantsLayer = true
        containerView.layer?.backgroundColor = NSColor.clear.cgColor
        containerView.autoresizingMask = []

        // Register as pending entry (will be assigned real tabId when Host sends config)
        // Pass viewController (self_) so we can update preferredContentSize later
        let entry = host.registerPendingEntry(internalId: internalId, containerView: containerView, viewController: self_)
        entry.containerSize = CGSize(width: initialWidth, height: initialHeight)

        // Get the pending tabId from the queue (Host sends this before connection request)
        // This enables 1:1:1 mapping by associating each connection with its Host tabId
        let pendingTabId = host.dequeueNextPendingTabId()
        if let tabId = pendingTabId {
            debugLog("[pending:\(internalId.prefix(8))] Associated with Host tabId: \(tabId.prefix(8))")
        } else {
            debugLog("⚠️ [pending:\(internalId.prefix(8))] No pending tabId in queue")
        }

        // Inform Host about this connection so it can bind tabId -> internalId explicitly
        // Include the tabId so Host can filter notifications for its own tab
        // Include hostProcessId for process isolation
        var userInfo: [String: Any] = [
            "bundleId": host.serviceBundleId,
            "internalId": internalId
        ]
        if let tabId = pendingTabId {
            userInfo["tabId"] = tabId
        }
        if let pid = host.hostProcessId {
            userInfo["hostProcessId"] = pid
        }
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.actro.xpcConnectionCreated"),
            object: nil,
            userInfo: userInfo,
            deliverImmediately: true
        )

        self_.setValue(containerView, forKey: "view")

        // 初始 preferredContentSize 设置为较小值，避免 loadView 时崩溃
        // 当收到 Host 的实际尺寸后，会动态扩展 jail view
        // 注意：太大的初始值（>4096）会导致 ViewBridge layout loop 崩溃
        let initialSize = NSSize(width: 1920, height: 1200)
        if self_.responds(to: NSSelectorFromString("setPreferredContentSize:")) {
            self_.setValue(NSValue(size: initialSize), forKey: "preferredContentSize")
            debugLog("[pending:\(internalId.prefix(8))] Set initial preferredContentSize: \(initialSize)")
        }

        // Setup React Native content for this Tab
        setupReactNativeContent(for: entry)

        // Check for any cached tab config that arrived before this loadView
        // This handles the race condition where bootstrap tabConfig arrives before ViewBridge
        // requests the ViewController
        host.tryApplyPendingConfig(for: entry, internalId: internalId)
    }
    let loadViewImp = imp_implementationWithBlock(loadViewBlock)
    class_addMethod(newClass, #selector(NSViewController.loadView), loadViewImp, "v@:")

    // remoteViewSizeDidChange: - ViewBridge calls this when host resizes
    // DISABLED: 使用 containerSizeChanged notification 作为唯一的尺寸同步路径
    // 避免两条路径互相干扰导致的"拖尾"效果
    let sizeDidChangeBlock: @convention(block) (AnyObject, NSSize) -> Void = { _, _ in
        // 忽略 ViewBridge 的尺寸回调，只使用 containerSizeChanged notification
    }
    let sizeDidChangeImp = imp_implementationWithBlock(sizeDidChangeBlock)
    class_addMethod(newClass, NSSelectorFromString("remoteViewSizeDidChange:"), sizeDidChangeImp, "v@:{CGSize=dd}")

    // allowsImplicitResizeRequests
    let allowsImplicitBlock: @convention(block) (AnyObject) -> Bool = { _ in true }
    class_addMethod(newClass, NSSelectorFromString("allowsImplicitResizeRequests"), imp_implementationWithBlock(allowsImplicitBlock), "B@:")

    // makesExplicitResizeRequests
    let makesExplicitBlock: @convention(block) (AnyObject) -> Bool = { _ in true }
    class_addMethod(newClass, NSSelectorFromString("makesExplicitResizeRequests"), imp_implementationWithBlock(makesExplicitBlock), "B@:")

    // setHostTabId: - Host 显式绑定 tabId 到当前连接，避免 FIFO 错绑
    let setHostTabIdBlock: @convention(block) (AnyObject, NSString) -> Void = { self_, hostTabIdObj in
        let hostTabId = hostTabIdObj as String
        guard !hostTabId.isEmpty else { return }

        object_setIvar(self_, hostTabIdIvar, hostTabIdObj)
        let internalId = (object_getIvar(self_, tabIdIvar) as? NSString) as String? ?? hostTabId
        debugLog("[\(hostTabId.prefix(8))] setHostTabId for connection \(internalId.prefix(8))")
        ServiceReactNativeHost.shared.bind(hostTabId: hostTabId, toInternalId: internalId)
    }
    class_addMethod(newClass, NSSelectorFromString("setHostTabId:"), imp_implementationWithBlock(setHostTabIdBlock), "v@:@")

    // NOTE: We do NOT override setPreferredContentSize:
    // Let the superclass (NSServiceViewController) handle it naturally.
    // This is critical - overriding and ignoring it prevents ViewBridge from
    // knowing our preferred size, causing jail view to stay at 1280x720.

    // remoteViewSize
    let remoteViewSizeBlock: @convention(block) (AnyObject) -> CGSize = { self_ in
        if let view = self_.value(forKey: "view") as? NSView {
            return view.frame.size
        }
        return .zero
    }
    class_addMethod(newClass, NSSelectorFromString("remoteViewSize"), imp_implementationWithBlock(remoteViewSizeBlock), "{CGSize=dd}@:")

    // disconnect - called by Host when tearing down connection
    let disconnectBlock: @convention(block) (AnyObject) -> Void = { self_ in
        if let tabIdObj = object_getIvar(self_, tabIdIvar) as? NSString {
            let internalId = tabIdObj as String
            debugLog("[\(logId(self_).prefix(8))] disconnect called, internal \(internalId.prefix(8))")
            ServiceReactNativeHost.shared.unregisterEntry(internalId: internalId)
        }
    }
    class_addMethod(newClass, NSSelectorFromString("disconnect"), imp_implementationWithBlock(disconnectBlock), "v@:")

    // dealloc - cleanup when ViewController is destroyed
    // Note: Do NOT manually call super dealloc - ARC handles it automatically after this block returns
    let deallocBlock: @convention(block) (AnyObject) -> Void = { self_ in
        if let tabIdObj = object_getIvar(self_, tabIdIvar) as? NSString {
            let internalId = tabIdObj as String
            debugLog("[\(logId(self_).prefix(8))] ViewController dealloc (internal \(internalId.prefix(8)))")
            ServiceReactNativeHost.shared.unregisterEntry(internalId: internalId)
        }
        // ARC will call [super dealloc] automatically
    }
    class_addMethod(newClass, NSSelectorFromString("dealloc"), imp_implementationWithBlock(deallocBlock), "v@:")

    objc_registerClassPair(newClass)
    debugLog("ActroServiceViewController class registered")

    // 🔑 Early initialization of ServiceReactNativeHost
    // This ensures pendingTabId listener is set up BEFORE any ViewController requests
    // Without this, pendingTabId notifications sent by Host would be missed
    _ = ServiceReactNativeHost.shared
    debugLog("ServiceReactNativeHost early init done, pendingTabId listener active")
}

// MARK: - React Native Content Setup

/// Setup container for a Tab entry (root view created when config arrives)
private func setupReactNativeContent(for entry: TabViewControllerEntry) {
    guard entry.containerView != nil else {
        return
    }
    // Root view will be created when tab config is received (event-driven)
}

/// Create root view with the received config (called when tab config arrives)
private func createRootViewWithConfig(for entry: TabViewControllerEntry, config: [String: Any]) {
    let logTabId = config["tabId"] as? String ?? entry.internalId
    let logSystemAppType = config["systemAppType"] as? String ?? "nil"
    ipcLog("🔧 createRootViewWithConfig: tab=\(String(logTabId.prefix(8))) systemAppType=\(logSystemAppType)")

    guard entry.rootView == nil else {
        ipcLog("⚠️ createRootViewWithConfig: rootView already exists for tab=\(String(logTabId.prefix(8)))")
        return
    }

    guard let container = entry.containerView else {
        ipcLog("⚠️ createRootViewWithConfig: containerView is nil for tab=\(String(logTabId.prefix(8)))")
        return
    }

    // Get container size - priority:
    // 1. entry.containerSize if already set by pending size change (most up-to-date)
    // 2. config's containerWidth/Height (from tab config)
    // 3. container.bounds.size (fallback)
    var containerSize: CGSize
    if entry.hasPendingSizeApplied {
        containerSize = entry.containerSize
    } else if let width = config["containerWidth"] as? Double,
              let height = config["containerHeight"] as? Double,
              width > 0 && height > 0 {
        containerSize = CGSize(width: width, height: height)
    } else {
        containerSize = container.bounds.size
    }
    entry.containerSize = containerSize

    // Update viewController's preferredContentSize to match
    if let vc = entry.viewController {
        vc.setValue(NSValue(size: containerSize), forKey: "preferredContentSize")
    }

    // Build initial properties from config
    let systemAppType = config["systemAppType"] as? String
    let tabId = config["tabId"] as? String ?? entry.internalId
    var tabDict: [String: Any] = [
        "id": tabId,
        "title": config["title"] as? String ?? "Tab",
        "backgroundColor": config["backgroundColor"] as? String ?? "#1f1f1f"
    ]
    if let systemAppType = systemAppType {
        tabDict["systemAppType"] = systemAppType
    }
    if let unifiedScript = config["unifiedScript"] as? String {
        tabDict["unifiedScript"] = unifiedScript
    }
    if let reloadKey = config["reloadKey"] { tabDict["reloadKey"] = reloadKey }
    // Copy other tab properties
    if let historyItems = config["historyItems"] { tabDict["historyItems"] = historyItems }
    if let bookmarks = config["bookmarks"] { tabDict["bookmarks"] = bookmarks }
    if let recentHistory = config["recentHistory"] { tabDict["recentHistory"] = recentHistory }

    let initialProperties: [String: Any] = [
        "singleTabMode": true,
        "tab": tabDict,
        "currentTabId": tabId,
        "isLeftPanelVisible": config["leftPanelVisible"] as? Bool ?? true,
        "isRightPanelVisible": config["rightPanelVisible"] as? Bool ?? true,
        "containerWidth": containerSize.width,
        "containerHeight": containerSize.height,
        "isOverviewMode": config["isOverviewMode"] as? Bool ?? false
    ]

    guard let rnView = ServiceReactNativeHost.shared.createRootView(
        for: entry,
        initialProperties: initialProperties
    ) else {
        ipcLog("⚠️ createRootViewWithConfig: createRootView returned nil for tab=\(String(logTabId.prefix(8)))")
        return
    }

    ipcLog("✅ Root view created for tab=\(String(logTabId.prefix(8))) size=\(Int(containerSize.width))x\(Int(containerSize.height))")

    // Set root view frame to fill container
    let targetFrame = CGRect(origin: .zero, size: containerSize)
    rnView.frame = targetFrame
    rnView.autoresizingMask = []
    container.addSubview(rnView)
    container.setRNView(rnView)

    // Apply Host size AFTER rnView is set (applyHostSize updates rnView.frame)
    container.applyHostSize(containerSize)

    // Force layout to ensure RN content is visible immediately
    // 🔑 XPC service has no NSWindow, so layout() won't call layoutSubviews.
    // Directly call layoutSubviews on root view and its subviews.
    let sel = NSSelectorFromString("layoutSubviews")
    if rnView.responds(to: sel) {
        rnView.perform(sel)
    }
    for subview in rnView.subviews {
        if subview.responds(to: sel) {
            subview.perform(sel)
        }
    }

    // Size change callback
    container.onSizeChanged = { [weak entry] newSize in
        guard let entry = entry else { return }
        ServiceReactNativeHost.shared.updateContainerSize(for: entry, size: newSize)
    }

    // Send initial dimensions after host is ready
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak entry] in
        guard let entry = entry else { return }
        ServiceReactNativeHost.shared.updateContainerSize(for: entry, size: entry.containerSize)
    }

    // Diagnostic: log surface state at multiple intervals
    let diagCheck: @convention(block) (Double) -> Void = { delay in
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak entry, weak container] in
            guard let entry = entry, let rv = entry.rootView else { return }
            let logId = String((entry.tabId ?? entry.internalId).prefix(8))
            let host = ServiceReactNativeHost.shared
            var stageStr = "?"
            if rv.responds(to: NSSelectorFromString("surface")) {
                if let surface = rv.value(forKey: "surface") as? NSObject {
                    let stage = surface.value(forKey: "stage") as? Int ?? -1
                    stageStr = "\(stage)"
                }
            }
            ipcLog("🔍 [DIAG:\(logId)] t+\(delay)s: subviews=\(rv.subviews.count) stage=\(stageStr) bundleLoaded=\(host.bundleLoaded) bundleFailed=\(host.bundleLoadFailed)")
        }
    }

    for t in [1.0, 2.0, 3.0, 5.0, 8.0] {
        diagCheck(t)
    }

    // Detailed diagnostic at +3.0s
    DispatchQueue.main.asyncAfter(deadline: .now() + 3.0) { [weak entry, weak container] in
        guard let entry = entry, let container = container, let rv = entry.rootView else { return }
        let logId = String((entry.tabId ?? entry.internalId).prefix(8))
        ipcLog("🔍 [DIAG:\(logId)] rootView type=\(type(of: rv)) frame=\(rv.frame) hidden=\(rv.isHidden) alpha=\(rv.alphaValue)")
        ipcLog("🔍 [DIAG:\(logId)] rootView.subviews=\(rv.subviews.count)")
        for (i, sub) in rv.subviews.enumerated() {
            ipcLog("🔍 [DIAG:\(logId)]   subview[\(i)] type=\(type(of: sub)) frame=\(sub.frame) hidden=\(sub.isHidden)")
            for (j, subsub) in sub.subviews.enumerated() {
                ipcLog("🔍 [DIAG:\(logId)]     subview[\(i)][\(j)] type=\(type(of: subsub)) frame=\(subsub.frame)")
            }
        }
        ipcLog("🔍 [DIAG:\(logId)] container frame=\(container.frame) bounds=\(container.bounds)")
        // Check surface stage via KVC
        if rv.responds(to: NSSelectorFromString("surface")) {
            if let surface = rv.value(forKey: "surface") as? NSObject {
                let stage = surface.value(forKey: "stage") as? Int ?? -1
                ipcLog("🔍 [DIAG:\(logId)] surface stage=\(stage) (7=Preparing, 127=Running)")
            }
        }
    }
}

// MARK: - Module Initialization

@_cdecl("ActroViewServiceModuleInit")
public func moduleInit() {
    registerActroServiceViewController()
}
