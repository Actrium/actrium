//
//  CanvasServiceViewController.swift
//  CanvasViewService
//
//  NSServiceViewController subclass that hosts React Native content for ViewBridge.
//  Uses Objective-C runtime to properly inherit from private NSServiceViewController.
//

import Cocoa
import ObjectiveC
import React
import React_RCTAppDelegate
import ReactAppDependencyProvider

// MARK: - Debug Logging

private let debugLogPath: String = {
    let tempDir = NSTemporaryDirectory()
    return (tempDir as NSString).appendingPathComponent("canvas-xpc-service.log")
}()

private func debugLog(_ message: String) {
    // Use NSLog for reliable logging in XPC service
    NSLog("[CanvasViewService] %@", message)

    // Also write to stderr
    fputs("[CanvasViewService] \(message)\n", stderr)
    fflush(stderr)

    // Also append to log file
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

/// Container view - 直接使用 ViewBridge 传来的尺寸（remoteViewSize）
/// ViewBridge 会自动将 Host 端 NSRemoteView 的 frame 同步到这里
private class ResizableContainerView: NSView {
    private var rnView: NSView?
    /// Callback to notify React Native of size changes
    var onSizeChanged: ((CGSize) -> Void)?

    /// 累积的捏合手势值（用于检测 pinch-out）
    private var cumulativeMagnification: CGFloat = 0

    override var acceptsFirstResponder: Bool { true }

    override func setFrameSize(_ newSize: NSSize) {
        // 直接使用 ViewBridge 传来的尺寸，无需任何过滤或锁定
        debugLog("Container setFrameSize: \(newSize)")
        super.setFrameSize(newSize)

        // 更新 RN view 尺寸
        if let rnView = rnView {
            rnView.frame = bounds
            debugLog("Updated RN view frame to: \(bounds)")
        }

        // 通知 React Native 尺寸变化
        if newSize.width > 0 && newSize.height > 0 {
            onSizeChanged?(newSize)
        }
    }

    func setRNView(_ view: NSView) {
        self.rnView = view
    }

    // MARK: - Magnify Gesture Handling (Mode Switching Trigger #4)
    //
    // ┌─────────────────────────────────────────────────────────────────────┐
    // │ 双指捏合手势处理 - Focus → Overview 退出                              │
    // ├─────────────────────────────────────────────────────────────────────┤
    // │ 架构说明:                                                            │
    // │ - NSRemoteView 会将事件直接路由到 Service 进程                         │
    // │ - 无需 Accessibility 权限（区别于 CGEventTap 方案）                    │
    // │ - 通过 DistributedNotificationCenter 通知 Host 端                     │
    // │ - Host 端在 WorkspaceMainView 中监听并调用 exitToOverview()           │
    // └─────────────────────────────────────────────────────────────────────┘

    override func magnify(with event: NSEvent) {
        debugLog("🤏 magnify: phase=\(event.phase.rawValue), delta=\(event.magnification), cumulative=\(cumulativeMagnification)")

        switch event.phase {
        case .began:
            cumulativeMagnification = 0
        case .changed:
            cumulativeMagnification += event.magnification

            // 捏合缩小超过阈值 (-0.3)，通知 Host 端
            if cumulativeMagnification < -0.3 {
                debugLog("🤏 Threshold reached → posting pinchToOverview")
                cumulativeMagnification = 0

                DistributedNotificationCenter.default().postNotificationName(
                    NSNotification.Name("com.canvas.pinchToOverview"),
                    object: nil,
                    userInfo: nil,
                    deliverImmediately: true
                )
            }
        case .ended, .cancelled:
            cumulativeMagnification = 0
        default:
            break
        }
    }
}

// MARK: - React Native Host for XPC Service

private class ServiceReactNativeFactoryDelegate: RCTDefaultReactNativeFactoryDelegate {
    override init() {
        super.init()
        self.dependencyProvider = RCTAppDependencyProvider()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func bundleURL() -> URL? {
        #if DEBUG
        // Connect to Metro bundler in development
        let metroPort = ProcessInfo.processInfo.environment["RCT_METRO_PORT"] ?? "8081"
        let host = "127.0.0.1:\(metroPort)"
        let url = RCTBundleURLProvider.jsBundleURL(
            forBundleRoot: "index",
            packagerHost: host,
            enableDev: true,
            enableMinification: false,
            inlineSourceMap: false
        )
        debugLog("bundleURL() called, returning: \(url?.absoluteString ?? "nil")")
        return url
        #else
        // Use offline bundle in release
        let url = Bundle.main.url(forResource: "actr", withExtension: "jsbundle")
        debugLog("bundleURL() called (release), returning: \(url?.absoluteString ?? "nil")")
        return url
        #endif
    }

    override func sourceURL(for bridge: RCTBridge) -> URL? {
        return bundleURL()
    }
}

// MARK: - Single-Process React Native Host
//
// 简化架构：每个 XPC service 进程只处理一个 tab
// 进程池由 Host app 的 XPCServicePool 管理

private class ServiceReactNativeHost {
    static let shared = ServiceReactNativeHost()

    private let factoryDelegate: ServiceReactNativeFactoryDelegate
    private let factory: RCTReactNativeFactory
    private var rootView: RCTRootView?
    private var currentProperties: [String: Any] = [:]
    private var currentContainerSize: CGSize = .zero
    private var tabConfigObserver: NSObjectProtocol?

    /// 缓存的 tab config，用于在 RN 就绪前收到的配置
    private var pendingTabConfig: [String: Any]?

    private init() {
        debugLog("Initializing React Native host")

        #if DEBUG
        RCTRedBoxSetEnabled(false)
        #endif

        factoryDelegate = ServiceReactNativeFactoryDelegate()
        factory = RCTReactNativeFactory(delegate: factoryDelegate)
        debugLog("React Native factory created")

        // Listen for tab config from host via DistributedNotificationCenter
        setupTabConfigListener()
    }

    // MARK: - Tab Config IPC

    private func setupTabConfigListener() {
        tabConfigObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.tabConfig"),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleTabConfigNotification(notification)
        }
        debugLog("Tab config listener installed")
    }

    private func handleTabConfigNotification(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let configJson = userInfo["config"] as? String,
              let configData = configJson.data(using: .utf8),
              let config = try? JSONSerialization.jsonObject(with: configData) as? [String: Any] else {
            debugLog("⚠️ Invalid tab config notification")
            return
        }

        debugLog("📥 Received tab config: \(config["tabId"] ?? "?")")

        // Update React Native props with new tab config
        updateTabConfig(config)
    }

    func updateTabConfig(_ config: [String: Any]) {
        debugLog("📥 updateTabConfig: \(config["title"] ?? "?")")

        // 如果 RN rootView 还未就绪，缓存配置
        if rootView == nil {
            debugLog("📥 RN not ready, caching tab config for later")
            pendingTabConfig = config
            return
        }

        applyTabConfig(config)
    }

    private func applyTabConfig(_ config: [String: Any]) {
        // Build React Native tabs array from config
        let tabId = config["tabId"] as? String ?? UUID().uuidString
        var tabDict: [String: Any] = [
            "id": tabId,
            "title": config["title"] as? String ?? "Untitled",
            "backgroundColor": config["backgroundColor"] as? String ?? "#1a1a2e"
        ]

        // Copy system app type if present (takes priority over scripts)
        if let systemAppType = config["systemAppType"] as? String {
            tabDict["systemAppType"] = systemAppType
            debugLog("📥 Tab is system app: \(systemAppType)")
        }

        // Copy scripts
        if let unifiedScript = config["unifiedScript"] as? String {
            tabDict["unifiedScript"] = unifiedScript
            debugLog("📥 Tab has unifiedScript: \(unifiedScript.prefix(100))...")
        }
        if let leftPanelScript = config["leftPanelScript"] as? String {
            tabDict["leftPanelScript"] = leftPanelScript
        }
        if let rightPanelScript = config["rightPanelScript"] as? String {
            tabDict["rightPanelScript"] = rightPanelScript
        }
        if let guestScript = config["guestScript"] as? String {
            tabDict["guestScript"] = guestScript
        }

        // Update container size if provided
        if let width = config["containerWidth"] as? Double,
           let height = config["containerHeight"] as? Double {
            currentContainerSize = CGSize(width: width, height: height)
        }

        // Build full properties using single tab mode (matching SingleTabRootView.swift)
        let properties: [String: Any] = [
            "singleTabMode": true,
            "tab": tabDict,
            "currentTabId": tabId,
            "isLeftPanelVisible": true,
            "isRightPanelVisible": true,
            "containerWidth": currentContainerSize.width,
            "containerHeight": currentContainerSize.height
        ]

        currentProperties = properties
        debugLog("📥 Applying tab config to RN: \(config["title"] ?? "?")")

        // Update React Native
        rootView?.appProperties = properties

        // Also update dimensions
        updateDimensionsViaEvent(to: currentContainerSize)
    }

    func createRootView(initialProperties: [String: Any], containerSize: CGSize) -> NSView {
        debugLog("Creating RCTRootView, containerSize: \(containerSize)")

        // Merge container size into initial properties
        var properties = initialProperties
        properties["containerWidth"] = containerSize.width
        properties["containerHeight"] = containerSize.height
        self.currentProperties = properties
        self.currentContainerSize = containerSize

        let view = factory.rootViewFactory.view(
            withModuleName: "ActrApp",
            initialProperties: properties,
            launchOptions: nil
        )

        if let rctRootView = view as? RCTRootView {
            self.rootView = rctRootView
            rctRootView.sizeFlexibility = .none
            rctRootView.backgroundColor = NSColor.clear
            debugLog("RCTRootView created successfully")

            // 应用缓存的 tab config（如果有）
            if let pending = pendingTabConfig {
                debugLog("📥 Applying pending tab config")
                pendingTabConfig = nil
                // 稍微延迟以确保 RN bridge 就绪
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
                    self?.applyTabConfig(pending)
                }
            }
        } else {
            debugLog("WARNING: View is not RCTRootView, type: \(type(of: view))")
        }

        return view
    }

    func updateProperties(_ properties: [String: Any]) {
        var mergedProps = properties
        // Preserve container size
        mergedProps["containerWidth"] = currentContainerSize.width
        mergedProps["containerHeight"] = currentContainerSize.height
        currentProperties = mergedProps
        rootView?.appProperties = mergedProps
    }

    /// Update container size - this is the critical method for XPC service
    /// Since RCTKeyWindow() returns nil in XPC, we pass container size via props
    func updateContainerSize(to size: CGSize) {
        guard size.width > 0 && size.height > 0 else {
            debugLog("updateContainerSize: Invalid size \(size), skipping")
            return
        }

        debugLog("updateContainerSize: \(currentContainerSize) -> \(size)")

        // Update stored size
        currentContainerSize = size

        // Update React Native props with new container size
        var properties = currentProperties
        properties["containerWidth"] = size.width
        properties["containerHeight"] = size.height
        currentProperties = properties

        if let rootView = rootView {
            rootView.appProperties = properties
            debugLog("updateContainerSize: Updated appProperties with new size")
        } else {
            debugLog("updateContainerSize: rootView not ready, size will be applied when created")
        }

        // Also try to update RN Dimensions via bridge event
        updateDimensionsViaEvent(to: size)
    }

    /// Update React Native Dimensions via event
    private func updateDimensionsViaEvent(to size: CGSize) {
        guard let bridge = rootView?.bridge else {
            debugLog("updateDimensionsViaEvent: No bridge available")
            return
        }

        debugLog("updateDimensionsViaEvent: Sending size \(size) to React Native")

        // Create dimensions object matching RCTDeviceInfo format
        let scale = NSScreen.main?.backingScaleFactor ?? 2.0
        let screenDims: [String: Any] = [
            "width": size.width,
            "height": size.height,
            "scale": scale,
            "fontScale": 1.0
        ]

        let windowDims: [String: Any] = [
            "width": size.width,
            "height": size.height,
            "scale": scale,
            "fontScale": 1.0
        ]

        let dimensions: [String: Any] = [
            "window": windowDims,
            "screen": screenDims
        ]

        // Send didUpdateDimensions event through the bridge
        bridge.enqueueJSCall("RCTDeviceEventEmitter", method: "emit", args: ["didUpdateDimensions", dimensions], completion: nil)

        debugLog("updateDimensionsViaEvent: Event sent successfully")
    }
}

// MARK: - Dynamic Class Registration

/// Register CanvasServiceViewController as a subclass of NSServiceViewController at runtime
@_cdecl("registerCanvasServiceViewController")
public func registerCanvasServiceViewController() {
    debugLog("Registering class...")

    // Load ViewBridge framework
    let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"
    guard dlopen(frameworkPath, RTLD_NOW) != nil else {
        debugLog("Failed to load ViewBridge")
        return
    }

    // Get NSServiceViewController class
    guard let superclass = NSClassFromString("NSServiceViewController") else {
        debugLog("NSServiceViewController not found")
        return
    }

    // Check if already registered
    if NSClassFromString("CanvasServiceViewController") != nil {
        debugLog("Already registered")
        return
    }

    // Create new class as subclass of NSServiceViewController
    guard let newClass = objc_allocateClassPair(superclass, "CanvasServiceViewController", 0) else {
        debugLog("Failed to allocate class pair")
        return
    }

    // Add loadView method - creates React Native view
    // 简化架构：每个 XPC service 进程只处理一个 tab
    let loadViewBlock: @convention(block) (AnyObject) -> Void = { (self_) in
        debugLog("🔵 loadView called")

        // 初始化 ServiceReactNativeHost（单例，包含 tab config 监听器）
        _ = ServiceReactNativeHost.shared
        debugLog("ServiceReactNativeHost initialized")

        // Use a reasonable default size instead of screen size
        let initialWidth: CGFloat = 1280
        let initialHeight: CGFloat = 720
        debugLog("Using default initial size: \(initialWidth)x\(initialHeight)")

        let containerView = ResizableContainerView(frame: NSRect(x: 0, y: 0, width: initialWidth, height: initialHeight))
        containerView.wantsLayer = true
        containerView.layer?.backgroundColor = NSColor.clear.cgColor
        containerView.autoresizingMask = []

        // Set as view using KVC
        self_.setValue(containerView, forKey: "view")
        debugLog("Container view set, frame: \(containerView.frame)")

        let targetSize = NSSize(width: initialWidth, height: initialHeight)
        let preferredSizeSelector = NSSelectorFromString("setPreferredContentSize:")
        if self_.responds(to: preferredSizeSelector) {
            self_.setValue(NSValue(size: targetSize), forKey: "preferredContentSize")
            debugLog("preferredContentSize set to \(targetSize)")
        }

        debugLog("Setting up React Native content...")
        setupReactNativeContent(in: containerView)
        debugLog("React Native content setup complete")
    }
    let loadViewImp = imp_implementationWithBlock(loadViewBlock)
    class_addMethod(newClass, #selector(NSViewController.loadView), loadViewImp, "v@:")

    // Add remoteViewSizeDidChange: method - called by ViewBridge when host requests resize
    // ViewBridge 自动同步 Host 端 NSRemoteView 的尺寸，直接使用无需过滤
    let sizeDidChangeBlock: @convention(block) (AnyObject, NSSize) -> Void = { (self_, size) in
        debugLog("remoteViewSizeDidChange -> \(size)")

        if let view = self_.value(forKey: "view") as? NSView {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            view.setFrameSize(size)
            view.needsLayout = true
            CATransaction.commit()
            debugLog("Updated view size to: \(size)")
        }
    }
    let sizeDidChangeImp = imp_implementationWithBlock(sizeDidChangeBlock)
    // Signature: v = void, @ = self, : = selector, {CGSize=dd} = CGSize with two doubles
    class_addMethod(newClass, NSSelectorFromString("remoteViewSizeDidChange:"), sizeDidChangeImp, "v@:{CGSize=dd}")

    // Add allowsImplicitResizeRequests - return YES to accept host-initiated resizes
    // 这样 Host 端的约束变化会传递到 Service 端
    let allowsImplicitBlock: @convention(block) (AnyObject) -> Bool = { _ in
        debugLog("allowsImplicitResizeRequests called -> YES")
        return true  // Accept implicit resize from host
    }
    let allowsImplicitImp = imp_implementationWithBlock(allowsImplicitBlock)
    class_addMethod(newClass, NSSelectorFromString("allowsImplicitResizeRequests"), allowsImplicitImp, "B@:")

    // Add makesExplicitResizeRequests - return YES (we control our size)
    let makesExplicitBlock: @convention(block) (AnyObject) -> Bool = { _ in
        debugLog("makesExplicitResizeRequests called -> YES")
        return true  // We make explicit resize requests
    }
    let makesExplicitImp = imp_implementationWithBlock(makesExplicitBlock)
    class_addMethod(newClass, NSSelectorFromString("makesExplicitResizeRequests"), makesExplicitImp, "B@:")

    // Override setPreferredContentSize: to capture size changes from host
    // 直接使用 Host 传来的尺寸，无需过滤
    let setPreferredSizeBlock: @convention(block) (AnyObject, NSSize) -> Void = { (self_, size) in
        debugLog("setPreferredContentSize -> \(size)")

        if let view = self_.value(forKey: "view") as? NSView {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            view.setFrameSize(size)
            view.needsLayout = true
            CATransaction.commit()
            debugLog("Updated view from setPreferredContentSize to: \(size)")
        }
    }
    let setPreferredSizeImp = imp_implementationWithBlock(setPreferredSizeBlock)
    class_addMethod(newClass, NSSelectorFromString("setPreferredContentSize:"), setPreferredSizeImp, "v@:{CGSize=dd}")

    // Add remoteViewSize getter - return actual view size
    let remoteViewSizeBlock: @convention(block) (AnyObject) -> CGSize = { self_ in
        let size: CGSize
        if let view = self_.value(forKey: "view") as? NSView {
            size = view.frame.size
        } else {
            size = .zero
        }
        debugLog("remoteViewSize called -> \(size)")
        return size
    }
    let remoteViewSizeImp = imp_implementationWithBlock(remoteViewSizeBlock)
    class_addMethod(newClass, NSSelectorFromString("remoteViewSize"), remoteViewSizeImp, "{CGSize=dd}@:")

    // Register the class
    objc_registerClassPair(newClass)
    debugLog("Class registered successfully")
}

// MARK: - React Native Content Setup

/// 简化版 - 每个 XPC service 进程只处理一个 tab
/// 使用 ServiceReactNativeHost.shared 单例
private func setupReactNativeContent(in containerView: NSView) {
    debugLog("Setting up React Native content (single-process mode)")

    // Show loading indicator first
    let loadingLabel = NSTextField(labelWithString: "Starting React Native...")
    loadingLabel.font = NSFont.systemFont(ofSize: 14)
    loadingLabel.textColor = NSColor.secondaryLabelColor
    loadingLabel.alignment = .center
    loadingLabel.translatesAutoresizingMaskIntoConstraints = false
    containerView.addSubview(loadingLabel)

    NSLayoutConstraint.activate([
        loadingLabel.centerXAnchor.constraint(equalTo: containerView.centerXAnchor),
        loadingLabel.centerYAnchor.constraint(equalTo: containerView.centerYAnchor)
    ])

    // Initial properties - will be updated when host sends tab config via IPC
    // Show a simple "XPC Ready" state initially (minimal script to verify RN works)
    let initialTabId = UUID().uuidString  // 临时 ID，会被 Host 发送的 tab config 覆盖
    let initialTab: [String: Any] = [
        "id": initialTabId,
        "title": "XPC Service Ready",
        "backgroundColor": "#1e40af",  // 深蓝色背景，易于区分
        "unifiedScript": """
        const { View, Text } = require('react-native');
        module.exports = function XPCReady() {
            return React.createElement(View, {
                style: { flex: 1, justifyContent: 'center', alignItems: 'center', backgroundColor: '#1e40af' }
            }, [
                React.createElement(Text, {
                    key: 't1',
                    style: { fontSize: 20, fontWeight: 'bold', color: '#fff' }
                }, 'XPC Service Ready'),
                React.createElement(Text, {
                    key: 't2',
                    style: { fontSize: 12, color: '#93c5fd', marginTop: 8 }
                }, 'Waiting for tab config from host...')
            ]);
        };
        """
    ]
    let initialProperties: [String: Any] = [
        "singleTabMode": true,
        "tab": initialTab,
        "currentTabId": initialTabId,
        "isLeftPanelVisible": true,
        "isRightPanelVisible": true
    ]

    // Create RN view in background to not block
    DispatchQueue.global(qos: .userInitiated).async {
        // Small delay to let ViewBridge connection stabilize
        Thread.sleep(forTimeInterval: 0.5)

        DispatchQueue.main.async {
            debugLog("Creating React Native root view (single-process mode)")

            // Remove loading indicator
            loadingLabel.removeFromSuperview()

            // Get container size for React Native
            let containerSize = containerView.bounds.size
            debugLog("Creating RN view with containerSize: \(containerSize)")

            // Create and add React Native view with initial properties
            // 使用进程单例 ServiceReactNativeHost.shared
            let rnView = ServiceReactNativeHost.shared.createRootView(
                initialProperties: initialProperties,
                containerSize: containerSize
            )

            // Use frame-based layout instead of Auto Layout (RCTRootView may not work well with Auto Layout)
            rnView.frame = containerView.bounds
            rnView.autoresizingMask = [.width, .height]
            containerView.addSubview(rnView)

            // Register RN view with ResizableContainerView for resize handling
            if let resizableContainer = containerView as? ResizableContainerView {
                resizableContainer.setRNView(rnView)

                // Connect size change callback to update React Native container size
                // 使用进程单例 ServiceReactNativeHost.shared
                resizableContainer.onSizeChanged = { newSize in
                    debugLog("onSizeChanged callback: \(newSize)")
                    ServiceReactNativeHost.shared.updateContainerSize(to: newSize)
                }

                // Send initial dimensions to React Native (as backup)
                // Note: containerSize is already passed during createRootView
                let initialSize = containerView.bounds.size
                if initialSize.width > 0 && initialSize.height > 0 {
                    // Delay slightly to ensure bridge is ready, then send again
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                        ServiceReactNativeHost.shared.updateContainerSize(to: initialSize)
                    }
                }
            }

            debugLog("React Native view added to container, rnView.frame: \(rnView.frame), container.bounds: \(containerView.bounds)")

            // Force layout update
            rnView.needsLayout = true
            containerView.needsLayout = true
        }
    }
}

// MARK: - Module Initialization

@_cdecl("CanvasViewServiceModuleInit")
public func moduleInit() {
    registerCanvasServiceViewController()
}
