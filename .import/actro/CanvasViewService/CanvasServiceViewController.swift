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
/// Key insight: ViewBridge updates the superview's bounds, so we use Auto Layout
/// to pin this view to the superview and automatically follow size changes.
private class ResizableContainerView: NSView {
    private var rnView: NSView?
    var onSizeChanged: ((CGSize) -> Void)?
    private var cumulativeMagnification: CGFloat = 0
    private var superviewFrameObserver: NSObjectProtocol?

    /// The tabId this container belongs to (for 1:1:1 mapping)
    var tabId: String?

    override var acceptsFirstResponder: Bool { true }

    /// Pin to superview bounds when added to view hierarchy
    /// This is the key fix: ViewBridge updates superview bounds, and Auto Layout
    /// propagates the size change to this view automatically.
    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        debugLog("[\(tabId?.prefix(8) ?? "?")] viewDidMoveToSuperview: superview=\(String(describing: superview))")

        // Clean up old observer
        if let observer = superviewFrameObserver {
            NotificationCenter.default.removeObserver(observer)
            superviewFrameObserver = nil
        }

        guard let superview = superview else { return }

        debugLog("[\(tabId?.prefix(8) ?? "?")] Superview bounds: \(superview.bounds.size)")

        // Pin to superview bounds using Auto Layout
        translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            leadingAnchor.constraint(equalTo: superview.leadingAnchor),
            trailingAnchor.constraint(equalTo: superview.trailingAnchor),
            topAnchor.constraint(equalTo: superview.topAnchor),
            bottomAnchor.constraint(equalTo: superview.bottomAnchor)
        ])

        // Also observe superview frame changes as backup
        superview.postsFrameChangedNotifications = true
        superviewFrameObserver = NotificationCenter.default.addObserver(
            forName: NSView.frameDidChangeNotification,
            object: superview,
            queue: .main
        ) { [weak self] _ in
            guard let self = self, let superview = self.superview else { return }
            debugLog("[\(self.tabId?.prefix(8) ?? "?")] Superview frame changed: \(superview.bounds.size)")
            self.needsLayout = true
        }
    }

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

    deinit {
        if let observer = superviewFrameObserver {
            NotificationCenter.default.removeObserver(observer)
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

    /// Whether a pending size change was applied from containerSizeChanged notification
    var hasPendingSizeApplied: Bool = false

    /// Loading indicator shown while waiting for tab config
    weak var loadingLabel: NSTextField?

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

    private var factoryDelegate: ServiceReactNativeFactoryDelegate?
    private var factory: RCTReactNativeFactory?
    private var tabConfigObserver: NSObjectProtocol?
    private var bridgeCreated = false

    /// Registry of TabViewControllerEntry by tabId (1:1:1 mapping)
    /// Key is the Host's tabId (Tab.id.uuidString)
    private var tabRegistry: [String: TabViewControllerEntry] = [:]

    /// Pending entries waiting for Host to send tabConfig with real tabId
    /// Key is the internal ID generated at loadView time
    private var pendingEntries: [String: TabViewControllerEntry] = [:]

    /// Pending container size changes received before entry was registered
    /// Key is tabId, value is the pending size
    private var pendingSizeChanges: [String: CGSize] = [:]

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

        // Setup listeners first - RCTBridge will be created after Metro config is received
        setupTabConfigListener()
        debugLog("Waiting for Metro config before creating RCTBridge...")
    }

    /// Create RCTBridge after Metro config is received
    private func ensureBridgeCreated() {
        guard !bridgeCreated else { return }
        bridgeCreated = true

        debugLog("Creating RCTBridge with Metro config: \(MetroConfig.hostString)")
        factoryDelegate = ServiceReactNativeFactoryDelegate()
        factory = RCTReactNativeFactory(delegate: factoryDelegate!)
        debugLog("Shared RCTBridge created")
    }

    /// Bind host tabId to a specific pending entry (by internalId) to avoid FIFO 误绑
    func bind(hostTabId: String, toInternalId internalId: String) {
        var pendingSize: CGSize?

        registryLock.lock()
        if let size = pendingSizeChanges.removeValue(forKey: hostTabId) {
            pendingSize = size
        }

        // Prefer pending entry that matches the connection internalId
        if let entry = pendingEntries.removeValue(forKey: internalId) {
            entry.assignTabId(hostTabId)
            tabRegistry[hostTabId] = entry
            registryLock.unlock()

            debugLog("[\(hostTabId.prefix(8))] Bound to connection \(internalId.prefix(8)), registry count: \(tabRegistry.count), pending count: \(pendingEntries.count)")
            if let size = pendingSize {
                debugLog("[\(hostTabId.prefix(8))] 📐 Applying cached container size after bind: \(size)")
                entry.containerSize = size
                entry.hasPendingSizeApplied = true
                entry.containerView?.setFrameSize(size)
                updateDimensionsForEntry(entry)
            }
            return
        }

        // Already registered? Update tabId on container for consistency
        if let entry = tabRegistry[hostTabId] {
            registryLock.unlock()
            entry.assignTabId(hostTabId)
            if let size = pendingSize {
                debugLog("[\(hostTabId.prefix(8))] 📐 Applying cached size to existing entry: \(size)")
                entry.containerSize = size
                entry.hasPendingSizeApplied = true
                entry.containerView?.setFrameSize(size)
                updateDimensionsForEntry(entry)
            }
            return
        }

        registryLock.unlock()
        debugLog("⚠️ [\(hostTabId.prefix(8))] bindHostTabId: no pending entry for internalId \(internalId.prefix(8))")
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

        // If already registered, return existing
        if let existing = tabRegistry[tabId] {
            registryLock.unlock()
            return existing
        }

        // Find the oldest pending entry (FIFO)
        guard let (internalId, entry) = pendingEntries.first else {
            registryLock.unlock()
            debugLog("⚠️ No pending entries to assign tabId: \(tabId.prefix(8))")
            return nil
        }

        // Remove from pending
        pendingEntries.removeValue(forKey: internalId)

        // Assign real tabId and add to registry
        entry.assignTabId(tabId)
        tabRegistry[tabId] = entry

        // Check for any pending size changes for this tabId
        let pendingSize = pendingSizeChanges.removeValue(forKey: tabId)

        registryLock.unlock()

        debugLog("[\(tabId.prefix(8))] Assigned to pending entry, registry count: \(tabRegistry.count), pending count: \(pendingEntries.count)")

        // Apply pending size change if any (outside lock)
        if let size = pendingSize {
            debugLog("[\(tabId.prefix(8))] 📐 Applying cached container size: \(size)")
            entry.containerSize = size
            entry.hasPendingSizeApplied = true
            if let container = entry.containerView {
                container.setFrameSize(size)
            }
        }

        return entry
    }

    // MARK: - Tab Config IPC

    private var metroConfigObserver: NSObjectProtocol?
    private var containerSizeObserver: NSObjectProtocol?
    private var bindTabObserver: NSObjectProtocol?
    private var pendingTabIdObserver: NSObjectProtocol?
    private var readyNotificationTimer: Timer?
    private var hasReceivedTabConfig = false

    /// Queue of pending tabIds sent by Host before connection request
    /// Used to associate each loadView call with the correct Host tabId
    private var pendingTabIdQueue: [String] = []
    private let pendingTabIdLock = NSLock()

    /// Get the next pending tabId from the queue (FIFO)
    func dequeueNextPendingTabId() -> String? {
        pendingTabIdLock.lock()
        defer { pendingTabIdLock.unlock() }
        guard !pendingTabIdQueue.isEmpty else { return nil }
        return pendingTabIdQueue.removeFirst()
    }

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
        ) { [weak self] notification in
            MetroConfig.update(from: notification.userInfo)
            // Create RCTBridge after Metro config is received
            self?.ensureBridgeCreated()
        }
        debugLog("📡 Setting up Metro config listener")

        // Listen for container size changes from Host (backup for ViewBridge's private API)
        containerSizeObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.containerSizeChanged"),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleContainerSizeChanged(notification)
        }
        debugLog("📡 Setting up container size listener")

        // Listen for explicit tabId binding from Host (connection-aware)
        let bindName = "com.canvas.bindTab.\(serviceBundleId)"
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
        debugLog("📡 Setting up bindTab listener: \(bindName)")

        // Listen for pending tabId from Host (sent before connection request)
        let pendingTabIdName = "com.canvas.pendingTabId.\(serviceBundleId)"
        pendingTabIdObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(pendingTabIdName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self = self,
                  let userInfo = notification.userInfo,
                  let tabId = userInfo["tabId"] as? String else { return }

            self.pendingTabIdLock.lock()
            self.pendingTabIdQueue.append(tabId)
            self.pendingTabIdLock.unlock()
            debugLog("📡 Received pendingTabId: \(tabId.prefix(8)), queue size: \(self.pendingTabIdQueue.count)")
        }
        debugLog("📡 Setting up pendingTabId listener: \(pendingTabIdName)")

        // Notify Host that this service is ready (with retry until tab config received)
        postXPCServiceReady()
        startReadyNotificationRetry()
    }

    private func handleContainerSizeChanged(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo["tabId"] as? String,
              let width = userInfo["width"] as? Double,
              let height = userInfo["height"] as? Double else {
            return
        }

        let newSize = CGSize(width: width, height: height)

        // Find the entry for this tab
        if let entry = getTabEntry(tabId: tabId) {
            let logId = entry.tabId?.prefix(8) ?? entry.internalId.prefix(8)
            debugLog("[\(logId)] 📐 Container size changed via notification: \(newSize)")

            // Update container view frame
            if let container = entry.containerView {
                container.setFrameSize(newSize)
            }

            // Update entry's container size and RN dimensions
            updateContainerSize(for: entry, size: newSize)
        } else {
            // Entry not yet registered - cache the size for later
            registryLock.lock()
            pendingSizeChanges[tabId] = newSize
            registryLock.unlock()
            debugLog("[pending:\(tabId.prefix(8))] 📐 Cached container size for later: \(newSize)")
        }
    }

    private func postXPCServiceReady() {
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.xpcServiceReady"),
            object: nil,
            userInfo: ["bundleId": serviceBundleId],
            deliverImmediately: true
        )
        debugLog("📤 Posted xpcServiceReady with bundleId: \(serviceBundleId)")
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
        guard let userInfo = notification.userInfo,
              let configJson = userInfo["config"] as? String,
              let configData = configJson.data(using: .utf8),
              let config = try? JSONSerialization.jsonObject(with: configData) as? [String: Any],
              let tabId = config["tabId"] as? String else {
            debugLog("⚠️ Invalid tab config notification (missing tabId)")
            return
        }

        // Stop retry timer once we receive first config
        hasReceivedTabConfig = true

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

        // If RCTRootView not ready yet, create it now (event-driven)
        guard entry.rootView != nil else {
            debugLog("[\(logId)] RootView not ready, creating now with config")
            createRootViewWithConfig(for: entry, config: config)
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

            // RN bridge may still be loading, re-apply properties multiple times to ensure render
            // This is needed because RCTRootView.setAppProperties only calls runApplication
            // when bridge.valid && !bridge.loading
            for delay in [0.3, 0.8, 1.5] {
                DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak entry] in
                    guard let entry = entry, let rootView = entry.rootView else { return }
                    let props = entry.currentProperties
                    rootView.appProperties = props
                    debugLog("[\(logId)] appProperties re-applied after \(delay)s delay")
                }
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

        // Ensure RCTBridge is created (should have been done when Metro config was received)
        ensureBridgeCreated()

        guard let factory = factory else {
            debugLog("[\(logId)] ⚠️ Factory not created yet - waiting for Metro config")
            return nil
        }

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

    // Add instance variables to store connection/internalId and host tabId
    class_addIvar(newClass, "_tabId", MemoryLayout<NSString>.size, UInt8(log2(Double(MemoryLayout<NSString>.alignment))), "@")  // internalId
    class_addIvar(newClass, "_hostTabId", MemoryLayout<NSString>.size, UInt8(log2(Double(MemoryLayout<NSString>.alignment))), "@")

    guard let tabIdIvar = class_getInstanceVariable(newClass, "_tabId"),
          let hostTabIdIvar = class_getInstanceVariable(newClass, "_hostTabId") else {
        debugLog("⚠️ Failed to get ivars for CanvasServiceViewController")
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
        var userInfo: [String: Any] = [
            "bundleId": host.serviceBundleId,
            "internalId": internalId
        ]
        if let tabId = pendingTabId {
            userInfo["tabId"] = tabId
        }
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.xpcConnectionCreated"),
            object: nil,
            userInfo: userInfo,
            deliverImmediately: true
        )

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
        let tabId = logId(self_)
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
    let deallocBlock: @convention(block) (AnyObject) -> Void = { self_ in
        if let tabIdObj = object_getIvar(self_, tabIdIvar) as? NSString {
            let internalId = tabIdObj as String
            debugLog("[\(logId(self_).prefix(8))] ViewController dealloc (internal \(internalId.prefix(8)))")
            ServiceReactNativeHost.shared.unregisterEntry(internalId: internalId)
        }
        if let superClass = class_getSuperclass(object_getClass(self_)),
           let superMethod = class_getMethodImplementation(superClass, NSSelectorFromString("dealloc")) {
            var superStruct = objc_super(receiver: Unmanaged.passUnretained(self_), super_class: superClass)
            let superDealloc = unsafeBitCast(superMethod, to: (@convention(c) (UnsafeMutablePointer<objc_super>, Selector) -> Void).self)
            superDealloc(&superStruct, NSSelectorFromString("dealloc"))
        }
    }
    class_addMethod(newClass, NSSelectorFromString("dealloc"), imp_implementationWithBlock(deallocBlock), "v@:")

    objc_registerClassPair(newClass)
    debugLog("CanvasServiceViewController class registered")
}

// MARK: - React Native Content Setup

/// Setup loading indicator for a Tab entry (RCTRootView created when config arrives)
private func setupReactNativeContent(for entry: TabViewControllerEntry) {
    let logId = entry.tabId?.prefix(8) ?? entry.internalId.prefix(8)

    guard let containerView = entry.containerView else {
        debugLog("[\(logId)] ⚠️ containerView is nil")
        return
    }

    debugLog("[\(logId)] Setting up loading indicator (waiting for tab config)")

    // Loading indicator - will be removed when RCTRootView is created
    let loadingLabel = NSTextField(labelWithString: "Waiting for tab config...")
    loadingLabel.font = NSFont.systemFont(ofSize: 14)
    loadingLabel.textColor = NSColor.secondaryLabelColor
    loadingLabel.alignment = .center
    loadingLabel.translatesAutoresizingMaskIntoConstraints = false
    containerView.addSubview(loadingLabel)
    NSLayoutConstraint.activate([
        loadingLabel.centerXAnchor.constraint(equalTo: containerView.centerXAnchor),
        loadingLabel.centerYAnchor.constraint(equalTo: containerView.centerYAnchor)
    ])

    // Store reference for later removal
    entry.loadingLabel = loadingLabel

    // RCTRootView will be created when tab config is received (event-driven)
}

/// Create RCTRootView with the received config (called when tab config arrives)
private func createRootViewWithConfig(for entry: TabViewControllerEntry, config: [String: Any]) {
    let logId = entry.tabId?.prefix(8) ?? entry.internalId.prefix(8)

    guard entry.rootView == nil else {
        debugLog("[\(logId)] RootView already exists, skipping creation")
        return
    }

    guard let container = entry.containerView else {
        debugLog("[\(logId)] ⚠️ containerView is nil")
        return
    }

    // Remove loading indicator
    entry.loadingLabel?.removeFromSuperview()
    entry.loadingLabel = nil

    // Get container size - priority:
    // 1. entry.containerSize if already set by pending size change (most up-to-date)
    // 2. config's containerWidth/Height (from tab config)
    // 3. container.bounds.size (fallback)
    var containerSize: CGSize
    if entry.hasPendingSizeApplied {
        // Use size from pending size change (already applied in assignPendingEntry)
        containerSize = entry.containerSize
        debugLog("[\(logId)] Using entry.containerSize (from pending): \(containerSize)")
    } else if let width = config["containerWidth"] as? Double,
              let height = config["containerHeight"] as? Double,
              width > 0 && height > 0 {
        containerSize = CGSize(width: width, height: height)
        debugLog("[\(logId)] Using container size from config: \(containerSize)")
    } else {
        containerSize = container.bounds.size
        debugLog("[\(logId)] Using container bounds size: \(containerSize)")
    }
    entry.containerSize = containerSize

    // Build initial properties from config
    let tabId = config["tabId"] as? String ?? entry.internalId
    var tabDict: [String: Any] = [
        "id": tabId,
        "title": config["title"] as? String ?? "Tab",
        "backgroundColor": config["backgroundColor"] as? String ?? "#1e40af"
    ]
    if let systemAppType = config["systemAppType"] as? String {
        tabDict["systemAppType"] = systemAppType
        debugLog("[\(logId)] Creating RootView with systemAppType: \(systemAppType)")
    }
    if let unifiedScript = config["unifiedScript"] as? String {
        tabDict["unifiedScript"] = unifiedScript
    }
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
        "containerHeight": containerSize.height
    ]

    debugLog("[\(logId)] Creating RCTRootView, size: \(containerSize)")

    guard let rnView = ServiceReactNativeHost.shared.createRootView(
        for: entry,
        initialProperties: initialProperties
    ) else {
        debugLog("[\(logId)] ⚠️ Failed to create RCTRootView")
        return
    }

    // Sync container size first if it differs from bounds
    if container.bounds.size != containerSize {
        debugLog("[\(logId)] Updating container frame to match config size: \(containerSize)")
        container.setFrameSize(containerSize)
    }

    // Set RCTRootView frame to fill container
    let targetFrame = CGRect(origin: .zero, size: containerSize)
    rnView.frame = targetFrame
    rnView.autoresizingMask = [.width, .height]
    container.addSubview(rnView)
    container.setRNView(rnView)

    debugLog("[\(logId)] RCTRootView frame set to: \(rnView.frame)")

    // Size change callback
    container.onSizeChanged = { [weak entry] newSize in
        guard let entry = entry else { return }
        ServiceReactNativeHost.shared.updateContainerSize(for: entry, size: newSize)
    }

    debugLog("[\(logId)] RCTRootView created and added to container")

    // Send initial dimensions after bridge is ready
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak entry] in
        guard let entry = entry else { return }
        ServiceReactNativeHost.shared.updateContainerSize(for: entry, size: entry.containerSize)
    }
}

// MARK: - Module Initialization

@_cdecl("CanvasViewServiceModuleInit")
public func moduleInit() {
    registerCanvasServiceViewController()
}
