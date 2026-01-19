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

    /// Track whether we've received a valid size from Host (via containerSizeChanged or tabConfig)
    /// Before this, the container uses a placeholder size and should NOT propagate it to children
    var hasReceivedHostSize: Bool = false

    override var acceptsFirstResponder: Bool { true }

    /// Setup view when added to hierarchy
    /// NOTE: Do NOT use Auto Layout to pin to superview bounds!
    /// ViewBridge superview bounds include canvas scale transform, which would make
    /// the container too large. Instead, we use explicit frame management via
    /// containerSizeChanged notifications from Host.
    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()

        // Clean up old observer
        if let observer = superviewFrameObserver {
            NotificationCenter.default.removeObserver(observer)
            superviewFrameObserver = nil
        }

        guard let superview = superview else { return }

        // Use autoresizing mask for flexible positioning, but NOT for size
        // Size is controlled by Host's containerSizeChanged notification
        translatesAutoresizingMaskIntoConstraints = true
        autoresizingMask = []  // Don't auto-resize with superview

        // When superview (ViewBridge jail) changes size, keep our origin aligned to top edge (non-flipped superview)
        superview.postsFrameChangedNotifications = true
        superviewFrameObserver = NotificationCenter.default.addObserver(
            forName: NSView.frameDidChangeNotification,
            object: superview,
            queue: .main
        ) { [weak self] _ in
            guard let self = self, let superview = self.superview else { return }
            // 避免在 applyHostSize 内部触发的 superview resize 导致重入/抖动
            guard !self.isApplyingHostSize else { return }
            self.syncOriginWithSuperview(superview)
        }
    }

    /// The authorized size from Host (via containerSizeChanged notification)
    /// We only apply sizes that match this, to filter out ViewBridge's scaled sizes
    var authorizedSize: NSSize?

    override func setFrameSize(_ newSize: NSSize) {
        // 基础过滤：避免异常尺寸把视图系统拖死
        guard newSize.width > 0 && newSize.height > 0 else { return }
        guard newSize.width <= 16384 && newSize.height <= 16384 else { return }

        // 一旦收到 Host 的权威尺寸，只允许该尺寸（避免 ViewBridge/jail 的非预期 setFrame 形成抖动/闭环）
        if hasReceivedHostSize, let authorizedSize {
            guard abs(newSize.width - authorizedSize.width) < 0.5,
                  abs(newSize.height - authorizedSize.height) < 0.5 else {
                return
            }
        }

        // Skip if size unchanged
        guard frame.size != newSize else { return }

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        super.setFrameSize(newSize)
        rnView?.frame = bounds
        CATransaction.commit()
    }

    /// Reference to the ViewController for updating preferredContentSize dynamically
    weak var viewController: AnyObject?

    /// Keep Container at jail origin (0,0) - NSRemoteView displays from this position
    private func syncOriginWithSuperview(_ superview: NSView) {
        guard hasReceivedHostSize else { return }

        // Container must stay at (0,0) because NSRemoteView displays jail content from origin
        guard frame.origin.x != 0 || frame.origin.y != 0 else { return }

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        super.setFrameOrigin(NSPoint(x: 0, y: 0))
        CATransaction.commit()
    }

    /// Notify ViewBridge of desired content size via preferredContentSize.
    /// Note: We do NOT resize the jail view - it's meant to be a large buffer.
    /// Container positioning within jail handles the visual alignment.
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

        // 只更新 preferredContentSize，不修改 jail view 尺寸
        // Jail view 是 ViewBridge 的内部缓冲区，应该保持大尺寸
        if let vc = viewController {
            vc.setValue(NSValue(size: clamped), forKey: "preferredContentSize")
        }
    }

    private static func computeMaximumJailSize() -> NSSize {
        let screens = NSScreen.screens
        guard !screens.isEmpty else {
            return NSSize(width: 4096, height: 4096)
        }

        var maxWidth: CGFloat = 0
        var maxHeight: CGFloat = 0
        for screen in screens {
            let size = screen.frame.size
            maxWidth = max(maxWidth, size.width)
            maxHeight = max(maxHeight, size.height)
        }

        // 防御性上限：避免异常值导致 ViewBridge jail 被扩到不可控
        maxWidth = min(maxWidth, 16384)
        maxHeight = min(maxHeight, 16384)

        return NSSize(width: maxWidth, height: maxHeight)
    }

    /// Called when Host sends a valid size (via containerSizeChanged or tabConfig)
    /// This marks the container as ready to propagate sizes to children
    func applyHostSize(_ newSize: NSSize) {
        hasReceivedHostSize = true
        authorizedSize = newSize

        // 禁用 AppKit/Core Animation 隐式动画
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        if let superview {
            isApplyingHostSize = true
            // 通知 ViewBridge 我们需要的尺寸
            syncJailSize(superview: superview, desiredSize: newSize)
            isApplyingHostSize = false
        }

        // Container 始终放在 Jail 的原点 (0,0)
        // NSRemoteView 从 Jail 的 (0,0) 开始显示，所以 Container 必须在这个位置
        // 之前尝试将 Container 放在 Jail "顶部"（originY = jailHeight - containerHeight）
        // 但这导致 NSRemoteView 看到的大部分区域是空白
        super.setFrameOrigin(NSPoint(x: 0, y: 0))
        super.setFrameSize(newSize)

        // 触发子视图更新
        rnView?.frame = bounds

        CATransaction.commit()

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
        // Always load from local bundle file (no Metro dev server dependency)
        return Bundle.main.url(forResource: "main.rctnapp", withExtension: "jsbundle")
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
    /// Reference to the ViewController (runtime-created Objective-C class)
    weak var viewController: AnyObject?
    var rootView: RCTRootView?
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

    /// 去抖动：待处理的尺寸更新
    private var pendingSizeUpdate: (tabId: String, size: CGSize)?
    /// 去抖动：定时器
    private var sizeDebounceTimer: Timer?
    /// 去抖动间隔 - 等待尺寸稳定后再应用
    private static let sizeDebounceInterval: TimeInterval = 0.025  // 25ms

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
        // Disable default packager server access check (we use dynamic port, not 8081)
        // This prevents "Connection refused" errors to localhost:8081/status
        RCTBundleURLProviderAllowPackagerServerAccess(false)
        #endif

        // Setup listeners and create RCTBridge (loads from local bundle)
        setupTabConfigListener()
    }

    /// Create RCTBridge (loads from local bundle file)
    private func ensureBridgeCreated() {
        guard !bridgeCreated else { return }
        bridgeCreated = true

        debugLog("Creating RCTBridge (loading from local bundle)")
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
        return entry
    }

    /// Unregister a Tab when its ViewController is destroyed
    func unregisterEntry(internalId: String) {
        registryLock.lock()
        defer { registryLock.unlock() }

        // Try to find by internalId in pending
        if let entry = pendingEntries.removeValue(forKey: internalId) {
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
        // Create RCTBridge immediately - apps load from local bundles (no Metro dependency)
        ensureBridgeCreated()

        // Listen for tab config with service-specific notification
        let notificationName = "com.canvas.tabConfig.\(serviceBundleId)"
        tabConfigObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(notificationName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleTabConfigNotification(notification)
        }

        // Listen for container size changes from Host (service-specific notification)
        let containerSizeName = "com.canvas.containerSizeChanged.\(serviceBundleId)"
        containerSizeObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(containerSizeName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleContainerSizeChanged(notification)
        }

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
        }

        // Notify Host that this service is ready (with retry until tab config received)
        postXPCServiceReady()
        startReadyNotificationRetry()
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
            return
        }

        // Extract requestId for log correlation
        let requestId = sizeInfo["requestId"] as? String ?? "?"
        // Extract isOverviewMode for compact view in system apps
        let isOverviewMode = sizeInfo["isOverviewMode"] as? Bool ?? false

        // 取整，避免浮点精度问题
        let newSize = CGSize(width: round(width), height: round(height))

        // 注意：DistributedNotificationCenter 的回调已经在 main queue 上执行
        // 不需要再 DispatchQueue.main.async，这样可以减少一个 run loop 周期的延迟

        // 去重：相同尺寸且 isOverviewMode 未变化时不重复处理
        if let entry = self.getTabEntry(tabId: tabId) {
            let currentOverviewMode = entry.currentProperties["isOverviewMode"] as? Bool ?? false
            let sizeUnchanged = entry.containerSize == newSize
            let overviewUnchanged = currentOverviewMode == isOverviewMode

            if sizeUnchanged && overviewUnchanged {
                NSLog("📐 [SVC] containerSizeChanged SKIPPED (same size & mode): req=%@ tab=%@ size=%.0fx%.0f isOverview=%@",
                      requestId, String(tabId.prefix(8)), newSize.width, newSize.height, isOverviewMode ? "true" : "false")
                return
            }
            NSLog("📐 [SVC] containerSizeChanged APPLYING: req=%@ tab=%@ oldSize=%.0fx%.0f newSize=%.0fx%.0f isOverview=%@ (sizeChanged=%@ modeChanged=%@)",
                  requestId, String(tabId.prefix(8)), entry.containerSize.width, entry.containerSize.height,
                  newSize.width, newSize.height, isOverviewMode ? "true" : "false",
                  sizeUnchanged ? "NO" : "YES", overviewUnchanged ? "NO" : "YES")
        } else {
            NSLog("📐 [SVC] containerSizeChanged NO ENTRY: req=%@ tab=%@ size=%.0fx%.0f (caching)",
                  requestId, String(tabId.prefix(8)), newSize.width, newSize.height)
        }

        // 直接应用尺寸（同步），不再去抖动（Host 端已经节流 25ms，无需双重延迟）
        self.applySizeUpdate(tabId: tabId, size: newSize, requestId: requestId, isOverviewMode: isOverviewMode)
    }

    /// 应用尺寸更新（同步，无延迟）
    private func applySizeUpdate(tabId: String, size: CGSize, requestId: String = "?", isOverviewMode: Bool? = nil) {
        if let entry = getTabEntry(tabId: tabId) {
            // 检查尺寸或 isOverviewMode 是否真的变化了
            let sizeChanged = entry.containerSize != size
            let overviewChanged = isOverviewMode != nil && entry.currentProperties["isOverviewMode"] as? Bool != isOverviewMode

            guard sizeChanged || overviewChanged else { return }

            // 🔑 关键：立即更新 entry.containerSize，防止竞态条件
            // 如果等待 applyHostSize 的回调链来更新，后续快速到达的通知
            // 可能在回调执行前检查状态，导致错误的 "same size" 判断
            entry.containerSize = size

            // Update isOverviewMode in RN props if provided
            if let isOverview = isOverviewMode {
                var properties = entry.currentProperties
                properties["isOverviewMode"] = isOverview
                properties["containerWidth"] = size.width
                properties["containerHeight"] = size.height
                entry.currentProperties = properties
                entry.rootView?.appProperties = properties
            }

            // 先更新 viewController 的 preferredContentSize
            // 这会让 ViewBridge 知道我们需要更大的 jail view
            if let vc = entry.viewController {
                vc.setValue(NSValue(size: size), forKey: "preferredContentSize")
            }

            if let container = entry.containerView {
                // applyHostSize → setFrameSize → onSizeChanged → updateContainerSize
                container.applyHostSize(size)
            }

            // 通知 Host 尺寸更新完成（事件驱动，替代固定延迟）
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
        let notificationName = "com.canvas.sizeApplied.\(serviceBundleId)"
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
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.xpcServiceReady"),
            object: nil,
            userInfo: ["bundleId": serviceBundleId],
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
        NSLog("[XPC] 📥 handleTabConfigNotification received")

        guard let userInfo = notification.userInfo else {
            NSLog("[XPC] ⚠️ handleTabConfigNotification: userInfo is nil")
            return
        }

        guard let configJson = userInfo["config"] as? String else {
            let keysStr = userInfo.keys.map { String(describing: $0) }.joined(separator: ", ")
            NSLog("[XPC] ⚠️ handleTabConfigNotification: config not found in userInfo, keys: %@", keysStr)
            return
        }

        guard let configData = configJson.data(using: .utf8),
              let config = try? JSONSerialization.jsonObject(with: configData) as? [String: Any] else {
            NSLog("[XPC] ⚠️ handleTabConfigNotification: failed to parse JSON")
            return
        }

        guard let tabId = config["tabId"] as? String else {
            NSLog("[XPC] ⚠️ handleTabConfigNotification: tabId not found in config")
            return
        }

        let systemAppType = config["systemAppType"] as? String ?? "nil"
        NSLog("[XPC] 📥 tabConfig for tab=%@ systemAppType=%@", String(tabId.prefix(8)), systemAppType)

        // Stop retry timer once we receive first config
        hasReceivedTabConfig = true

        // Try to find existing entry, or assign a pending entry
        let entry: TabViewControllerEntry?
        if let existing = getTabEntry(tabId: tabId) {
            NSLog("[XPC] ✅ Found existing entry for tab=%@", String(tabId.prefix(8)))
            entry = existing
        } else {
            // First config for this tabId - assign a pending entry
            entry = assignPendingEntry(tabId: tabId)
            if entry != nil {
                NSLog("[XPC] ✅ Assigned pending entry for tab=%@", String(tabId.prefix(8)))
            } else {
                NSLog("[XPC] ⚠️ No pending entry available for tab=%@ (pendingEntries empty)", String(tabId.prefix(8)))
            }
        }

        if let entry = entry {
            applyConfigToEntry(entry, config: config)
        } else {
            NSLog("[XPC] ⚠️ Cannot apply config - no entry for tab=%@", String(tabId.prefix(8)))
        }
    }

    private func applyConfigToEntry(_ entry: TabViewControllerEntry, config: [String: Any]) {
        // If RCTRootView not ready yet, create it now (event-driven)
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

        if let rootView = entry.rootView {
            rootView.appProperties = properties

            // RN bridge may still be loading, re-apply properties multiple times to ensure render
            for delay in [0.3, 0.8, 1.5] {
                DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak entry] in
                    guard let entry = entry, let rootView = entry.rootView else { return }
                    rootView.appProperties = entry.currentProperties
                }
            }
        }

        // Update RN dimensions
        updateDimensionsForEntry(entry)
    }

    // MARK: - RCTRootView Creation

    /// Create a new RCTRootView for a specific Tab
    /// Each Tab gets its own RCTRootView, sharing the same RCTBridge
    func createRootView(for entry: TabViewControllerEntry, initialProperties: [String: Any]) -> RCTRootView? {
        // Ensure RCTBridge is created (should have been done when Metro config was received)
        ensureBridgeCreated()

        guard let factory = factory else {
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
            return nil
        }

        rootView.sizeFlexibility = .none
        rootView.backgroundColor = NSColor.clear
        entry.rootView = rootView

        return rootView
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

/// Setup container for a Tab entry (RCTRootView created when config arrives)
private func setupReactNativeContent(for entry: TabViewControllerEntry) {
    guard entry.containerView != nil else {
        return
    }
    // RCTRootView will be created when tab config is received (event-driven)
}

/// Create RCTRootView with the received config (called when tab config arrives)
private func createRootViewWithConfig(for entry: TabViewControllerEntry, config: [String: Any]) {
    let logTabId = config["tabId"] as? String ?? entry.internalId
    let logSystemAppType = config["systemAppType"] as? String ?? "nil"
    NSLog("[XPC] 🔧 createRootViewWithConfig: tab=%@ systemAppType=%@", String(logTabId.prefix(8)), logSystemAppType)

    guard entry.rootView == nil else {
        NSLog("[XPC] ⚠️ createRootViewWithConfig: rootView already exists for tab=%@", String(logTabId.prefix(8)))
        return
    }

    guard let container = entry.containerView else {
        NSLog("[XPC] ⚠️ createRootViewWithConfig: containerView is nil for tab=%@", String(logTabId.prefix(8)))
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
        NSLog("[XPC] ⚠️ createRootViewWithConfig: createRootView returned nil for tab=%@", String(logTabId.prefix(8)))
        return
    }

    NSLog("[XPC] ✅ RCTRootView created for tab=%@ size=%.0fx%.0f", String(logTabId.prefix(8)), containerSize.width, containerSize.height)

    // Set RCTRootView frame to fill container
    let targetFrame = CGRect(origin: .zero, size: containerSize)
    rnView.frame = targetFrame
    rnView.autoresizingMask = []
    container.addSubview(rnView)
    container.setRNView(rnView)

    // Apply Host size AFTER rnView is set (applyHostSize updates rnView.frame)
    container.applyHostSize(containerSize)

    // Force layout to ensure RN content is visible immediately
    container.needsLayout = true
    container.layoutSubtreeIfNeeded()

    // Size change callback
    container.onSizeChanged = { [weak entry] newSize in
        guard let entry = entry else { return }
        ServiceReactNativeHost.shared.updateContainerSize(for: entry, size: newSize)
    }

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
