//
//  CanvasServiceViewController.swift
//  CanvasViewService
//
//  NSServiceViewController subclass that hosts React Native content for ViewBridge.
//  Uses Objective-C runtime to properly inherit from private NSServiceViewController.
//
//  Architecture: 1:1:1 Mapping
//  - Each Tab has exactly one ViewBridge connection
//  - Each ViewBridge connection has exactly one CanvasServiceViewController instance
//  - Each CanvasServiceViewController has exactly one RCTRootView
//  - All RCTRootViews share the same RCTBridge (managed by ServiceReactNativeHost singleton)
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
    NSLog("[CanvasViewService] %@", message)
    fputs("[CanvasViewService] \(message)\n", stderr)
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

/// Container view for each Tab's RCTRootView
/// ViewBridge automatically syncs Host's NSRemoteView frame here
private class ResizableContainerView: NSView {
    private var rnView: NSView?
    var onSizeChanged: ((CGSize) -> Void)?
    private var cumulativeMagnification: CGFloat = 0

    /// The tabId this container belongs to (for 1:1:1 mapping)
    var tabId: String?

    override var acceptsFirstResponder: Bool { true }

    override func setFrameSize(_ newSize: NSSize) {
        debugLog("[\(tabId?.prefix(8) ?? "?")] Container setFrameSize: \(newSize)")
        super.setFrameSize(newSize)

        if let rnView = rnView {
            rnView.frame = bounds
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
                    NSNotification.Name("com.canvas.pinchToOverview"),
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
}

// MARK: - Metro Config (received from Host via DistributedNotificationCenter)

/// Holds Metro bundler configuration received from Canvas Host
/// Updated dynamically when Host's MetroManager broadcasts config
private enum MetroConfig {
    /// Default Metro host (fallback if no config received)
    static var host: String = "127.0.0.1"
    static var port: UInt16 = 8081

    static var hostString: String {
        "\(host):\(port)"
    }

    /// Update config from notification userInfo
    static func update(from userInfo: [AnyHashable: Any]?) {
        guard let info = userInfo else { return }
        if let host = info["host"] as? String {
            Self.host = host
        }
        if let port = info["port"] as? UInt16 {
            Self.port = port
        } else if let port = info["port"] as? Int {
            Self.port = UInt16(port)
        }
        debugLog("MetroConfig updated: \(Self.hostString)")
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

    override func bundleURL() -> URL? {
        #if DEBUG
        // Use Metro config received from Host (or fallback to default)
        return RCTBundleURLProvider.jsBundleURL(
            forBundleRoot: "index",
            packagerHost: MetroConfig.hostString,
            enableDev: true,
            enableMinification: false,
            inlineSourceMap: false
        )
        #else
        return Bundle.main.url(forResource: "main.rctnapp", withExtension: "jsbundle")
        #endif
    }

    override func sourceURL(for bridge: RCTBridge) -> URL? {
        return bundleURL()
    }
}

// MARK: - Tab View Controller Entry

/// Represents a single Tab's ViewController and RCTRootView
/// Key component for 1:1:1 architecture
private class TabViewControllerEntry {
    /// The tabId from Host (assigned when first config is received)
    /// Initially nil until Host sends tabConfig with the real tabId
    var tabId: String?

    /// Temporary ID for internal tracking before real tabId is known
    let internalId: String

    weak var containerView: ResizableContainerView?
    var rootView: RCTRootView?
    var currentProperties: [String: Any] = [:]
    var containerSize: CGSize = .zero
    var pendingConfig: [String: Any]?

    /// Whether this entry is waiting for its first tabConfig from Host
    var isPending: Bool { tabId == nil }

    init(internalId: String, containerView: ResizableContainerView) {
        self.internalId = internalId
        self.containerView = containerView
        containerView.tabId = internalId  // Use internal ID for logging initially
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

/// Manages shared RCTBridge and per-Tab RCTRootViews
/// Singleton per XPC service process
private class ServiceReactNativeHost {
    static let shared = ServiceReactNativeHost()

    private let factoryDelegate: ServiceReactNativeFactoryDelegate
    private let factory: RCTReactNativeFactory
    private var tabConfigObserver: NSObjectProtocol?

    /// Registry of TabViewControllerEntry by tabId (1:1:1 mapping)
    /// Key is the Host's tabId (Tab.id.uuidString)
    private var tabRegistry: [String: TabViewControllerEntry] = [:]

    /// Pending entries waiting for Host to send tabConfig with real tabId
    /// Key is the internal ID generated at loadView time
    private var pendingEntries: [String: TabViewControllerEntry] = [:]

    private let registryLock = NSLock()

    /// This service's bundle identifier for notification routing
    let serviceBundleId: String

    private init() {
        // In XPC services, Bundle.main returns the host app's bundle, not the XPC service's bundle.
        // We need to get the XPC service's own bundle identifier from its bundle path.
        // The executable path format: .../RCTViewService0.xpc/Contents/MacOS/CanvasViewService
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

        #if DEBUG
        RCTRedBoxSetEnabled(false)
        #endif

        factoryDelegate = ServiceReactNativeFactoryDelegate()
        factory = RCTReactNativeFactory(delegate: factoryDelegate)
        debugLog("Shared RCTBridge created")

        setupTabConfigListener()
    }

    // MARK: - Tab Registry (1:1:1 Mapping)

    /// Register a new Tab's ViewController (initially as pending)
    /// The entry will be moved to tabRegistry when Host sends tabConfig with real tabId
    func registerPendingEntry(internalId: String, containerView: ResizableContainerView) -> TabViewControllerEntry {
        registryLock.lock()
        defer { registryLock.unlock() }

        let entry = TabViewControllerEntry(internalId: internalId, containerView: containerView)
        pendingEntries[internalId] = entry
        debugLog("[pending:\(internalId.prefix(8))] Entry registered, pending count: \(pendingEntries.count)")
        return entry
    }

    /// Unregister a Tab when its ViewController is destroyed
    func unregisterEntry(internalId: String) {
        registryLock.lock()
        defer { registryLock.unlock() }

        // Try to find by internalId in pending
        if let entry = pendingEntries.removeValue(forKey: internalId) {
            debugLog("[pending:\(internalId.prefix(8))] Pending entry removed")
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
                debugLog("[\(tabId.prefix(8))] Entry removed from registry")
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

    /// Find the oldest pending entry and assign it the given tabId
    /// Returns the entry if successful, nil if no pending entries
    private func assignPendingEntry(tabId: String) -> TabViewControllerEntry? {
        registryLock.lock()
        defer { registryLock.unlock() }

        // If already registered, return existing
        if let existing = tabRegistry[tabId] {
            return existing
        }

        // Find the oldest pending entry (FIFO)
        guard let (internalId, entry) = pendingEntries.first else {
            debugLog("⚠️ No pending entries to assign tabId: \(tabId.prefix(8))")
            return nil
        }

        // Remove from pending
        pendingEntries.removeValue(forKey: internalId)

        // Assign real tabId and add to registry
        entry.assignTabId(tabId)
        tabRegistry[tabId] = entry

        debugLog("[\(tabId.prefix(8))] Assigned to pending entry, registry count: \(tabRegistry.count), pending count: \(pendingEntries.count)")
        return entry
    }

    // MARK: - Tab Config IPC

    private var metroConfigObserver: NSObjectProtocol?

    private func setupTabConfigListener() {
        // Listen for tab config with service-specific notification
        let notificationName = "com.canvas.tabConfig.\(serviceBundleId)"
        debugLog("📡 Setting up listener for: \(notificationName)")

        tabConfigObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(notificationName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleTabConfigNotification(notification)
        }

        // Listen for Metro config broadcasts from Host
        metroConfigObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.metro.config"),
            object: nil,
            queue: .main
        ) { notification in
            MetroConfig.update(from: notification.userInfo)
        }
        debugLog("📡 Setting up Metro config listener")

        // Notify Host that this service is ready
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.xpcServiceReady"),
            object: nil,
            userInfo: ["bundleId": serviceBundleId],
            deliverImmediately: true
        )
        debugLog("📤 Posted xpcServiceReady with bundleId: \(serviceBundleId)")
    }

    private func handleTabConfigNotification(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let configJson = userInfo["config"] as? String,
              let configData = configJson.data(using: .utf8),
              let config = try? JSONSerialization.jsonObject(with: configData) as? [String: Any],
              let tabId = config["tabId"] as? String else {
            debugLog("⚠️ Invalid tab config notification (missing tabId)")
            return
        }

        debugLog("📥 Received config for tab: \(tabId.prefix(8))")

        // Try to find existing entry, or assign a pending entry
        let entry: TabViewControllerEntry?
        if let existing = getTabEntry(tabId: tabId) {
            entry = existing
        } else {
            // First config for this tabId - assign a pending entry
            entry = assignPendingEntry(tabId: tabId)
        }

        if let entry = entry {
            applyConfigToEntry(entry, config: config)
        } else {
            debugLog("⚠️ Tab \(tabId.prefix(8)) has no pending entry to assign")
        }
    }

    private func applyConfigToEntry(_ entry: TabViewControllerEntry, config: [String: Any]) {
        let logId = entry.tabId?.prefix(8) ?? entry.internalId.prefix(8)
        debugLog("[\(logId)] Applying config: \(config["title"] ?? "?")")

        // If RCTRootView not ready yet, cache config
        guard entry.rootView != nil else {
            debugLog("[\(logId)] RootView not ready, caching config")
            entry.pendingConfig = config
            return
        }

        // Use Host's tabId (should be assigned by now)
        let tabId = entry.tabId ?? entry.internalId

        // Build tab properties
        var tabDict: [String: Any] = [
            "id": tabId,
            "title": config["title"] as? String ?? "Untitled",
            "backgroundColor": config["backgroundColor"] as? String ?? "#1a1a2e"
        ]

        // System app type
        if let systemAppType = config["systemAppType"] as? String {
            tabDict["systemAppType"] = systemAppType
            debugLog("[\(logId)] System app: \(systemAppType)")
        }

        // Scripts
        if let s = config["unifiedScript"] as? String { tabDict["unifiedScript"] = s }
        if let s = config["leftPanelScript"] as? String { tabDict["leftPanelScript"] = s }
        if let s = config["rightPanelScript"] as? String { tabDict["rightPanelScript"] = s }
        if let s = config["guestScript"] as? String { tabDict["guestScript"] = s }

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
        let properties: [String: Any] = [
            "singleTabMode": true,
            "tab": tabDict,
            "currentTabId": tabId,
            "isLeftPanelVisible": config["leftPanelVisible"] as? Bool ?? true,
            "isRightPanelVisible": config["rightPanelVisible"] as? Bool ?? true,
            "containerWidth": entry.containerSize.width,
            "containerHeight": entry.containerSize.height
        ]

        entry.currentProperties = properties

        // Debug: Log the properties being set
        let systemAppType = (tabDict["systemAppType"] as? String) ?? "nil"
        debugLog("[\(logId)] Setting appProperties: systemAppType=\(systemAppType), rootView=\(entry.rootView != nil ? "exists" : "nil")")

        if let rootView = entry.rootView {
            rootView.appProperties = properties
            debugLog("[\(logId)] appProperties updated (systemAppType=\(systemAppType))")

            // RN bridge may still be loading, re-apply properties after a delay to ensure render
            // This is needed because RCTRootView.setAppProperties only calls runApplication
            // when bridge.valid && !bridge.loading
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak entry] in
                guard let entry = entry, let rootView = entry.rootView else { return }
                let props = entry.currentProperties
                rootView.appProperties = props
                debugLog("[\(logId)] appProperties re-applied after delay")
            }
        } else {
            debugLog("[\(logId)] ⚠️ rootView is nil, cannot update appProperties")
        }

        // Update RN dimensions
        updateDimensionsForEntry(entry)
    }

    // MARK: - RCTRootView Creation

    /// Create a new RCTRootView for a specific Tab
    /// Each Tab gets its own RCTRootView, sharing the same RCTBridge
    func createRootView(for entry: TabViewControllerEntry, initialProperties: [String: Any]) -> RCTRootView? {
        let logId = entry.tabId?.prefix(8) ?? entry.internalId.prefix(8)
        debugLog("[\(logId)] Creating RCTRootView")

        var properties = initialProperties
        properties["containerWidth"] = entry.containerSize.width
        properties["containerHeight"] = entry.containerSize.height
        entry.currentProperties = properties

        let view = factory.rootViewFactory.view(
            withModuleName: "ActrApp",
            initialProperties: properties,
            launchOptions: nil
        )

        guard let rootView = view as? RCTRootView else {
            debugLog("[\(logId)] ⚠️ Failed to create RCTRootView")
            return nil
        }

        rootView.sizeFlexibility = .none
        rootView.backgroundColor = NSColor.clear
        entry.rootView = rootView

        debugLog("[\(logId)] RCTRootView created successfully")

        // Apply pending config if any
        if let pending = entry.pendingConfig {
            entry.pendingConfig = nil
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self, weak entry] in
                guard let entry = entry else { return }
                self?.applyConfigToEntry(entry, config: pending)
            }
        }

        return rootView
    }

    /// Update container size for a specific Tab
    func updateContainerSize(for entry: TabViewControllerEntry, size: CGSize) {
        guard size.width > 0 && size.height > 0 else { return }

        let logId = entry.tabId?.prefix(8) ?? entry.internalId.prefix(8)
        debugLog("[\(logId)] updateContainerSize: \(entry.containerSize) -> \(size)")
        entry.containerSize = size

        var properties = entry.currentProperties
        properties["containerWidth"] = size.width
        properties["containerHeight"] = size.height
        entry.currentProperties = properties

        entry.rootView?.appProperties = properties
        updateDimensionsForEntry(entry)
    }

    private func updateDimensionsForEntry(_ entry: TabViewControllerEntry) {
        guard let bridge = entry.rootView?.bridge else { return }

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

        bridge.enqueueJSCall("RCTDeviceEventEmitter", method: "emit", args: ["didUpdateDimensions", dimensions], completion: nil)
    }
}

// MARK: - Dynamic Class Registration

@_cdecl("registerCanvasServiceViewController")
public func registerCanvasServiceViewController() {
    debugLog("Registering CanvasServiceViewController class...")

    let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"
    guard dlopen(frameworkPath, RTLD_NOW) != nil else {
        debugLog("Failed to load ViewBridge")
        return
    }

    guard let superclass = NSClassFromString("NSServiceViewController") else {
        debugLog("NSServiceViewController not found")
        return
    }

    if NSClassFromString("CanvasServiceViewController") != nil {
        debugLog("Already registered")
        return
    }

    guard let newClass = objc_allocateClassPair(superclass, "CanvasServiceViewController", 0) else {
        debugLog("Failed to allocate class pair")
        return
    }

    // Add instance variable to store tabId
    class_addIvar(newClass, "_tabId", MemoryLayout<NSString>.size, UInt8(log2(Double(MemoryLayout<NSString>.alignment))), "@")

    // loadView - creates React Native view for this Tab
    let loadViewBlock: @convention(block) (AnyObject) -> Void = { self_ in
        // Generate internal ID for tracking (Host will send the real tabId later)
        let internalId = UUID().uuidString
        debugLog("[pending:\(internalId.prefix(8))] loadView called")

        // Store internal ID in instance variable (for cleanup on dealloc)
        object_setIvar(self_, class_getInstanceVariable(type(of: self_) as! AnyClass, "_tabId")!, internalId as NSString)

        // Initialize shared host (creates shared RCTBridge)
        let host = ServiceReactNativeHost.shared

        let initialWidth: CGFloat = 1280
        let initialHeight: CGFloat = 720

        let containerView = ResizableContainerView(frame: NSRect(x: 0, y: 0, width: initialWidth, height: initialHeight))
        containerView.wantsLayer = true
        containerView.layer?.backgroundColor = NSColor.clear.cgColor
        containerView.autoresizingMask = []

        // Register as pending entry (will be assigned real tabId when Host sends config)
        let entry = host.registerPendingEntry(internalId: internalId, containerView: containerView)
        entry.containerSize = CGSize(width: initialWidth, height: initialHeight)

        self_.setValue(containerView, forKey: "view")

        let targetSize = NSSize(width: initialWidth, height: initialHeight)
        if self_.responds(to: NSSelectorFromString("setPreferredContentSize:")) {
            self_.setValue(NSValue(size: targetSize), forKey: "preferredContentSize")
        }

        // Setup React Native content for this Tab
        setupReactNativeContent(for: entry)
    }
    let loadViewImp = imp_implementationWithBlock(loadViewBlock)
    class_addMethod(newClass, #selector(NSViewController.loadView), loadViewImp, "v@:")

    // remoteViewSizeDidChange: - ViewBridge calls this when host resizes
    let sizeDidChangeBlock: @convention(block) (AnyObject, NSSize) -> Void = { self_, size in
        guard let tabIdObj = object_getIvar(self_, class_getInstanceVariable(type(of: self_) as! AnyClass, "_tabId")!) as? NSString else {
            return
        }
        let tabId = tabIdObj as String
        debugLog("[\(tabId.prefix(8))] remoteViewSizeDidChange: \(size)")

        if let view = self_.value(forKey: "view") as? NSView {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            view.setFrameSize(size)
            view.needsLayout = true
            CATransaction.commit()
        }
    }
    let sizeDidChangeImp = imp_implementationWithBlock(sizeDidChangeBlock)
    class_addMethod(newClass, NSSelectorFromString("remoteViewSizeDidChange:"), sizeDidChangeImp, "v@:{CGSize=dd}")

    // allowsImplicitResizeRequests
    let allowsImplicitBlock: @convention(block) (AnyObject) -> Bool = { _ in true }
    class_addMethod(newClass, NSSelectorFromString("allowsImplicitResizeRequests"), imp_implementationWithBlock(allowsImplicitBlock), "B@:")

    // makesExplicitResizeRequests
    let makesExplicitBlock: @convention(block) (AnyObject) -> Bool = { _ in true }
    class_addMethod(newClass, NSSelectorFromString("makesExplicitResizeRequests"), imp_implementationWithBlock(makesExplicitBlock), "B@:")

    // setPreferredContentSize:
    let setPreferredSizeBlock: @convention(block) (AnyObject, NSSize) -> Void = { self_, size in
        if let view = self_.value(forKey: "view") as? NSView {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            view.setFrameSize(size)
            view.needsLayout = true
            CATransaction.commit()
        }
    }
    class_addMethod(newClass, NSSelectorFromString("setPreferredContentSize:"), imp_implementationWithBlock(setPreferredSizeBlock), "v@:{CGSize=dd}")

    // remoteViewSize
    let remoteViewSizeBlock: @convention(block) (AnyObject) -> CGSize = { self_ in
        if let view = self_.value(forKey: "view") as? NSView {
            return view.frame.size
        }
        return .zero
    }
    class_addMethod(newClass, NSSelectorFromString("remoteViewSize"), imp_implementationWithBlock(remoteViewSizeBlock), "{CGSize=dd}@:")

    // dealloc - cleanup when ViewController is destroyed
    let deallocBlock: @convention(block) (AnyObject) -> Void = { self_ in
        if let tabIdObj = object_getIvar(self_, class_getInstanceVariable(type(of: self_) as! AnyClass, "_tabId")!) as? NSString {
            let internalId = tabIdObj as String
            debugLog("[\(internalId.prefix(8))] ViewController dealloc")
            ServiceReactNativeHost.shared.unregisterEntry(internalId: internalId)
        }
    }
    // Note: dealloc needs special handling, using release pattern instead
    // For now, rely on notification or explicit cleanup

    objc_registerClassPair(newClass)
    debugLog("CanvasServiceViewController class registered")
}

// MARK: - React Native Content Setup

/// Setup React Native content for a specific Tab entry
private func setupReactNativeContent(for entry: TabViewControllerEntry) {
    let logId = entry.tabId?.prefix(8) ?? entry.internalId.prefix(8)

    guard let containerView = entry.containerView else {
        debugLog("[\(logId)] ⚠️ containerView is nil")
        return
    }

    debugLog("[\(logId)] Setting up React Native content")

    // Loading indicator
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

    // Initial properties with "loading" type (avoids loading askit)
    // Use internalId for initial properties (Host tabId not yet known)
    let initialTabId = entry.tabId ?? entry.internalId
    let initialTab: [String: Any] = [
        "id": initialTabId,
        "title": "XPC Service Ready",
        "backgroundColor": "#1e40af",
        "systemAppType": "loading"
    ]
    let initialProperties: [String: Any] = [
        "singleTabMode": true,
        "tab": initialTab,
        "currentTabId": initialTabId,
        "isLeftPanelVisible": true,
        "isRightPanelVisible": true
    ]

    // Create RN view asynchronously
    // Capture logId for use in async blocks
    let asyncLogId = logId
    DispatchQueue.global(qos: .userInitiated).async {
        Thread.sleep(forTimeInterval: 0.3)

        DispatchQueue.main.async {
            loadingLabel.removeFromSuperview()

            guard let container = entry.containerView else { return }
            let containerSize = container.bounds.size
            entry.containerSize = containerSize

            debugLog("[\(asyncLogId)] Creating RCTRootView, size: \(containerSize)")

            guard let rnView = ServiceReactNativeHost.shared.createRootView(
                for: entry,
                initialProperties: initialProperties
            ) else {
                debugLog("[\(asyncLogId)] ⚠️ Failed to create RCTRootView")
                return
            }

            rnView.frame = container.bounds
            rnView.autoresizingMask = [.width, .height]
            container.addSubview(rnView)
            container.setRNView(rnView)

            // Size change callback
            container.onSizeChanged = { [weak entry] newSize in
                guard let entry = entry else { return }
                ServiceReactNativeHost.shared.updateContainerSize(for: entry, size: newSize)
            }

            debugLog("[\(asyncLogId)] RCTRootView added to container")

            // Send initial dimensions after bridge is ready
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak entry] in
                guard let entry = entry else { return }
                ServiceReactNativeHost.shared.updateContainerSize(for: entry, size: entry.containerSize)
            }
        }
    }
}

// MARK: - Module Initialization

@_cdecl("CanvasViewServiceModuleInit")
public func moduleInit() {
    registerCanvasServiceViewController()
}
