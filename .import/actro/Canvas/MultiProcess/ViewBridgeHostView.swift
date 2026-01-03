//
//  ViewBridgeHostView.swift
//  Canvas
//
//  Host view that displays content from a ViewBridge XPC service.
//  Uses NSRemoteViewController to request and embed remote views.
//
//  新架构：Remote view size 跟随 TabPanel 实际尺寸
//  - Focus 模式：接近全屏尺寸
//  - Overview 模式：卡片尺寸
//  - 动画过程：尺寸平滑过渡
//  - RN 自适应布局
//

import Cocoa

// MARK: - ViewBridge Host View

/// A view that hosts content from a ViewBridge XPC service.
/// Remote view size 跟随 TabPanel 实际尺寸，RN 自适应布局。
public class ViewBridgeHostView: NSView {

    // MARK: - Properties

    /// The remote view controller received from the service
    private var remoteViewController: NSViewController?

    /// The service bundle identifier
    private let serviceBundleIdentifier: String

    /// The view controller class name in the service
    private let serviceViewControllerClassName: String

    /// Loading state
    private var isLoading = false
    private var loadingIndicator: NSProgressIndicator?
    private var statusLabel: NSTextField?

    /// Current content size - 跟随 TabPanel 实际尺寸
    private var currentContentSize: CGSize = CGSize(width: 1280, height: 720)

    /// 节流：上次更新时间
    private var lastResizeTime: Date = .distantPast
    /// 节流：待处理的尺寸更新
    private var pendingResize: CGSize?
    /// 节流：定时器
    private var resizeThrottleTimer: Timer?

    // MARK: - Initialization

    /// Initialize with service configuration
    public init(
        serviceBundleIdentifier bundleIdentifier: String,
        viewControllerClassName className: String
    ) {
        self.serviceBundleIdentifier = bundleIdentifier
        self.serviceViewControllerClassName = className
        super.init(frame: .zero)
        setupView()
    }

    /// Convenience initializer with default Canvas service
    public convenience init() {
        self.init(
            serviceBundleIdentifier: "chat.askaway.CanvasViewService.CanvasViewService",
            viewControllerClassName: "CanvasServiceViewController"
        )
    }

    /// Convenience initializer with initial content size
    public convenience init(initialContentSize: CGSize) {
        self.init()
        self.currentContentSize = initialContentSize
        NSLog("ViewBridgeHostView: init with contentSize=%.1fx%.1f", initialContentSize.width, initialContentSize.height)
    }

    required init?(coder: NSCoder) {
        self.serviceBundleIdentifier = "com.anthropic.Canvas.CanvasViewService"
        self.serviceViewControllerClassName = "CanvasServiceViewController"
        super.init(coder: coder)
        setupView()
    }

    // MARK: - Setup

    private func setupView() {
        wantsLayer = true
        layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
        layer?.masksToBounds = true  // 裁剪超出部分

        // Add loading indicator
        let indicator = NSProgressIndicator()
        indicator.style = .spinning
        indicator.controlSize = .regular
        indicator.translatesAutoresizingMaskIntoConstraints = false
        addSubview(indicator)

        let label = NSTextField(labelWithString: "Connecting to ViewBridge service...")
        label.font = NSFont.systemFont(ofSize: 12)
        label.textColor = NSColor.secondaryLabelColor
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)

        NSLayoutConstraint.activate([
            indicator.centerXAnchor.constraint(equalTo: centerXAnchor),
            indicator.centerYAnchor.constraint(equalTo: centerYAnchor, constant: -10),
            label.centerXAnchor.constraint(equalTo: centerXAnchor),
            label.topAnchor.constraint(equalTo: indicator.bottomAnchor, constant: 10)
        ])

        loadingIndicator = indicator
        statusLabel = label

        NSLog("ViewBridgeHostView: Initialized for service \(serviceBundleIdentifier)")
    }

    // MARK: - Size Control

    /// 返回 intrinsicContentSize
    public override var intrinsicContentSize: NSSize {
        return currentContentSize
    }

    /// 确保子视图使用 currentContentSize
    public override func layout() {
        super.layout()
        // NSRemoteView 使用 currentContentSize，忽略 SwiftUI 可能设置的其他尺寸
        if let vc = remoteViewController {
            let targetFrame = CGRect(origin: .zero, size: currentContentSize)
            if vc.view.frame != targetFrame {
                vc.view.frame = targetFrame
                NSLog("ViewBridgeHostView: layout() -> %.0fx%.0f", currentContentSize.width, currentContentSize.height)
            }
        }
    }

    /// 接受外部尺寸设置
    public override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
    }

    // MARK: - View Lifecycle

    public override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()

        if window != nil && remoteViewController == nil && !isLoading {
            requestRemoteViewController()
        }
    }

    // MARK: - Remote View Controller Request

    /// Request the remote view controller from the ViewBridge service
    public func requestRemoteViewController() {
        guard !isLoading else {
            NSLog("ViewBridgeHostView: Already loading")
            return
        }

        isLoading = true
        loadingIndicator?.startAnimation(nil)
        statusLabel?.stringValue = "Connecting to ViewBridge service..."

        NSLog("ViewBridgeHostView: Requesting \(serviceViewControllerClassName) from \(serviceBundleIdentifier)")

        // Load ViewBridge framework
        let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"
        guard dlopen(frameworkPath, RTLD_NOW) != nil else {
            handleError("Failed to load ViewBridge framework")
            return
        }

        // Get NSRemoteViewController class
        guard let remoteVCClass = NSClassFromString("NSRemoteViewController") else {
            handleError("NSRemoteViewController class not found")
            return
        }

        // Call requestViewController:fromServiceWithBundleIdentifier:connectionHandler:
        let selector = NSSelectorFromString("requestViewController:fromServiceWithBundleIdentifier:connectionHandler:")

        guard remoteVCClass.responds(to: selector) else {
            handleError("requestViewController method not found")
            return
        }

        // Create connection handler block
        let handler: @convention(block) (NSViewController?, Error?) -> Void = { [weak self] viewController, error in
            DispatchQueue.main.async {
                self?.handleConnectionResult(viewController: viewController, error: error)
            }
        }

        // Invoke the class method
        let method = remoteVCClass.method(for: selector)
        typealias RequestFunc = @convention(c) (AnyClass, Selector, String, String, Any) -> Void
        let requestFunc = unsafeBitCast(method, to: RequestFunc.self)

        requestFunc(
            remoteVCClass,
            selector,
            serviceViewControllerClassName,
            serviceBundleIdentifier,
            handler
        )
    }

    // MARK: - Connection Handling

    private func handleConnectionResult(viewController: NSViewController?, error: Error?) {
        isLoading = false
        loadingIndicator?.stopAnimation(nil)

        if let error = error {
            handleError("Connection failed: \(error.localizedDescription)")
            return
        }

        guard let vc = viewController else {
            handleError("No view controller returned")
            return
        }

        NSLog("ViewBridgeHostView: Received remote view controller: \(type(of: vc))")

        // Remove loading UI
        loadingIndicator?.removeFromSuperview()
        statusLabel?.removeFromSuperview()

        // Embed the remote view controller's view
        remoteViewController = vc

        let remoteView = vc.view

        // 设置 NSRemoteView 的尺寸为 currentContentSize（视口尺寸）
        remoteView.translatesAutoresizingMaskIntoConstraints = true
        remoteView.autoresizingMask = []  // 禁用自动调整，由 layout() 控制
        remoteView.frame = CGRect(origin: .zero, size: currentContentSize)

        addSubview(remoteView)

        // 确保 ViewBridgeHostView 自身也使用正确的尺寸
        super.setFrameSize(currentContentSize)

        NSLog("ViewBridgeHostView: Remote view embedded, size=\(currentContentSize)")
    }

    // MARK: - Content Size Management

    /// 节流间隔（毫秒）- 防止 resize storm
    private static let resizeThrottleInterval: TimeInterval = 0.05  // 50ms

    /// Update the content size
    /// 使用节流机制防止 resize storm
    public func updateContentSize(_ newSize: CGSize) {
        // Validate reasonable size
        guard newSize.width >= 50 && newSize.height >= 50 &&
              newSize.width <= 4096 && newSize.height <= 4096 else {
            return
        }

        // Ignore small changes (< 1px)
        let sizeDiff = abs(newSize.width - currentContentSize.width) + abs(newSize.height - currentContentSize.height)
        guard sizeDiff >= 1 else {
            return
        }

        // 节流：如果距离上次更新时间太短，延迟处理
        let now = Date()
        let timeSinceLastResize = now.timeIntervalSince(lastResizeTime)

        if timeSinceLastResize < Self.resizeThrottleInterval {
            // 记录待处理的尺寸，稍后处理
            pendingResize = newSize
            scheduleThrottledResize()
            return
        }

        // 立即应用尺寸更新
        applyContentSize(newSize)
    }

    /// 调度节流后的 resize
    private func scheduleThrottledResize() {
        // 如果已有定时器，不重复创建
        guard resizeThrottleTimer == nil else { return }

        resizeThrottleTimer = Timer.scheduledTimer(withTimeInterval: Self.resizeThrottleInterval, repeats: false) { [weak self] _ in
            guard let self = self, let pendingSize = self.pendingResize else { return }
            self.resizeThrottleTimer = nil
            self.pendingResize = nil
            self.applyContentSize(pendingSize)
        }
    }

    /// 实际应用尺寸更新
    private func applyContentSize(_ newSize: CGSize) {
        lastResizeTime = Date()
        currentContentSize = newSize

        fputs("📐 ViewBridgeHostView.applyContentSize: \(newSize)\n", stderr)
        fflush(stderr)

        // 更新 NSRemoteView 的尺寸
        if let vc = remoteViewController {
            let remoteView = vc.view

            // 更新 NSRemoteView 的 frame
            remoteView.frame = CGRect(origin: .zero, size: newSize)

            fputs("📐 ViewBridgeHostView: remoteView size = \(remoteView.frame.size)\n", stderr)

            // 尝试通过 NSRemoteViewController 的私有方法通知 service 端尺寸变化
            // 方法1: _setRemoteViewSize:
            let setRemoteSizeSelector = NSSelectorFromString("_setRemoteViewSize:")
            fputs("📐 _setRemoteViewSize: responds=\(vc.responds(to: setRemoteSizeSelector))\n", stderr)
            if vc.responds(to: setRemoteSizeSelector) {
                vc.perform(setRemoteSizeSelector, with: NSValue(size: newSize))
            }

            // 方法2: setPreferredContentSize: (标准方法)
            let setPreferredSelector = NSSelectorFromString("setPreferredContentSize:")
            fputs("📐 setPreferredContentSize: responds=\(vc.responds(to: setPreferredSelector))\n", stderr)
            if vc.responds(to: setPreferredSelector) {
                vc.setValue(NSValue(size: newSize), forKey: "preferredContentSize")
            }

            // 方法3: 尝试直接发送 remoteViewSizeDidChange: 到 service (通过 NSInvocation)
            let notifySizeSelector = NSSelectorFromString("_notifyRemoteViewOfSizeChange:")
            fputs("📐 _notifyRemoteViewOfSizeChange: responds=\(vc.responds(to: notifySizeSelector))\n", stderr)
            if vc.responds(to: notifySizeSelector) {
                vc.perform(notifySizeSelector, with: NSValue(size: newSize))
            }

            fflush(stderr)

            // 强制布局更新
            remoteView.needsLayout = true
            remoteView.layoutSubtreeIfNeeded()
        }

        // 通知 intrinsicContentSize 变化并触发重新布局
        invalidateIntrinsicContentSize()
        needsLayout = true
    }

    private func handleError(_ message: String) {
        NSLog("ViewBridgeHostView: Error - \(message)")

        isLoading = false
        loadingIndicator?.stopAnimation(nil)
        statusLabel?.stringValue = "Error: \(message)"
        statusLabel?.textColor = NSColor.systemRed

        // Show error state
        layer?.backgroundColor = NSColor.systemRed.withAlphaComponent(0.1).cgColor
    }

    // MARK: - Cleanup

    public func disconnect() {
        if let vc = remoteViewController {
            // Try to call disconnect on the remote VC
            let selector = NSSelectorFromString("disconnect")
            if vc.responds(to: selector) {
                vc.perform(selector)
            }
            vc.view.removeFromSuperview()
            remoteViewController = nil
            NSLog("ViewBridgeHostView: Disconnected from remote service")
        }
    }

    deinit {
        resizeThrottleTimer?.invalidate()
        resizeThrottleTimer = nil
        disconnect()
    }

    // MARK: - Snapshot

    /// Capture a snapshot of the current view content
    public func captureSnapshot() -> NSImage? {
        let size = currentContentSize
        guard size.width > 0 && size.height > 0 else { return nil }

        // 优先使用 NSRemoteView 的私有 snapshot API（更可靠地抓到跨进程内容）
        if let remoteView = remoteViewController?.view as? NSObject {
            let snapshotSelector = NSSelectorFromString("snapshot")
            if remoteView.responds(to: snapshotSelector),
               let result = remoteView.perform(snapshotSelector)?.takeUnretainedValue() {
                let cgImage = result as! CGImage
                let image = NSImage(cgImage: cgImage, size: size)
                NSLog("ViewBridgeHostView: Captured NSRemoteView snapshot \(size)")
                return image
            }
        }

        // 回退：渲染 layer（有时拿不到 remote content）
        guard let layer = layer else { return nil }
        let image = NSImage(size: size)
        image.lockFocus()
        if let context = NSGraphicsContext.current?.cgContext {
            layer.render(in: context)
        }
        image.unlockFocus()

        NSLog("ViewBridgeHostView: Captured layer snapshot \(size)")
        return image
    }
}

// MARK: - ViewBridge Availability Check

extension ViewBridgeHostView {

    /// Check if ViewBridge is available on this system
    public static var isAvailable: Bool {
        let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"
        guard dlopen(frameworkPath, RTLD_NOW) != nil else {
            return false
        }
        return NSClassFromString("NSRemoteViewController") != nil
    }
}

// MARK: - SwiftUI Wrapper

import SwiftUI
import Combine

/// Coordinator that holds reference to ViewBridgeHostView
final class ViewBridgeCoordinator {
    weak var hostView: ViewBridgeHostView?

    func captureSnapshot() -> NSImage? {
        return hostView?.captureSnapshot()
    }
}

/// Manager that holds coordinators for all tabs
/// 不使用 @Published 避免在 view body 评估时触发状态更新
final class ViewBridgeCoordinatorManager: ObservableObject {
    // 不使用 @Published - coordinators 的变化不需要触发 UI 更新
    private var coordinators: [UUID: ViewBridgeCoordinator] = [:]

    func coordinator(for tabId: UUID) -> ViewBridgeCoordinator {
        if let existing = coordinators[tabId] {
            return existing
        }
        let newCoordinator = ViewBridgeCoordinator()
        coordinators[tabId] = newCoordinator
        fputs("📦 CoordinatorManager: created coordinator for tab \(tabId.uuidString.prefix(8))\n", stderr)
        return newCoordinator
    }

    func removeCoordinator(for tabId: UUID) {
        if coordinators.removeValue(forKey: tabId) != nil {
            fputs("🗑️ CoordinatorManager: removed coordinator for tab \(tabId.uuidString.prefix(8))\n", stderr)
        }
    }
}

/// SwiftUI wrapper for ViewBridgeHostView
/// Remote view size 跟随 TabPanel 实际尺寸
struct ViewBridgeTabView: NSViewRepresentable {
    let tabId: UUID
    let contentSize: CGSize  // TabPanel 实际尺寸
    let coordinator: ViewBridgeCoordinator

    func makeNSView(context: Context) -> ViewBridgeHostView {
        NSLog("🆕 ViewBridgeTabView.makeNSView: tab=%@, size=%.0fx%.0f", String(tabId.uuidString.prefix(8)), contentSize.width, contentSize.height)
        let view = ViewBridgeHostView(initialContentSize: contentSize)
        coordinator.hostView = view
        return view
    }

    func updateNSView(_ nsView: ViewBridgeHostView, context: Context) {
        // 输出每次尺寸更新到 stderr
        fputs("📐 ViewBridgeTabView.updateNSView: contentSize=\(contentSize)\n", stderr)
        coordinator.hostView = nsView
        nsView.updateContentSize(contentSize)
    }

    static func dismantleNSView(_ nsView: ViewBridgeHostView, coordinator: ()) {
        NSLog("❌ ViewBridgeTabView.dismantleNSView: DISCONNECTING!")
        nsView.disconnect()
    }
}

// MARK: - ViewBridge Multi-Process Tab View

/// A view that displays tab content via ViewBridge XPC service
/// Remote view size 跟随 TabPanel 实际尺寸，RN 自适应
struct ViewBridgeMultiProcessTabView: View {
    let tab: Tab
    let containerSize: CGSize  // TabPanel 实际尺寸
    let coordinator: ViewBridgeCoordinator
    var isFocused: Bool = false  // 是否当前 focused（只有 focused 的 tab 才发送配置）

    @State private var isLoading = true
    @State private var errorMessage: String?

    init(tab: Tab, containerSize: CGSize, coordinator: ViewBridgeCoordinator, isFocused: Bool = false) {
        self.tab = tab
        self.containerSize = containerSize
        self.coordinator = coordinator
        self.isFocused = isFocused
        fputs("🔵 ViewBridgeMultiProcessTabView.init: tab=\(tab.id.uuidString.prefix(8)), containerSize=\(containerSize), isFocused=\(isFocused)\n", stderr)
        fflush(stderr)
    }

    var body: some View {
        let _ = fputs("🔵 ViewBridgeMultiProcessTabView.body: containerSize=\(containerSize)\n", stderr)
        ZStack(alignment: .topLeading) {
            if ViewBridgeHostView.isAvailable {
                ViewBridgeTabView(
                    tabId: tab.id,
                    contentSize: containerSize,
                    coordinator: coordinator
                )
                // 稳定的 id 确保 View 不会因为 containerSize 变化而被重建
                .id(tab.id)
                .frame(width: containerSize.width, height: containerSize.height, alignment: .topLeading)
                .clipped()
                .onAppear {
                    isLoading = false
                    // 只有 focused 的 tab 才发送配置到 XPC service
                    if isFocused {
                        sendTabConfigToService()
                    }
                }
                .onChange(of: isFocused) { _, newValue in
                    // 当 tab 变成 focused 时，发送配置
                    if newValue {
                        fputs("📤 Tab became focused, sending config: \(tab.id.uuidString.prefix(8))\n", stderr)
                        sendTabConfigToService()
                    }
                }
                .onChange(of: tab.unifiedScript) { _, _ in
                    // Re-send config when script changes (e.g., reload), only if focused
                    if isFocused {
                        sendTabConfigToService()
                    }
                }
            } else {
                unavailableView
            }

            if isLoading {
                loadingView
            }

            if let error = errorMessage {
                errorView(error)
            }
        }
        .frame(width: containerSize.width, height: containerSize.height, alignment: .topLeading)
    }

    private var loadingView: some View {
        VStack(spacing: 12) {
            ProgressView()
                .scaleEffect(0.8)
            Text("Connecting to ViewBridge service...")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor).opacity(0.9))
    }

    private var unavailableView: some View {
        VStack(spacing: 16) {
            Image(systemName: "xmark.circle")
                .font(.system(size: 48))
                .foregroundColor(.red)
            Text("ViewBridge Not Available")
                .font(.headline)
            Text("The ViewBridge framework is not available on this system.")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    /// Send tab configuration to the ViewBridge service via DistributedNotificationCenter
    private func sendTabConfigToService() {
        fputs("📤 Sending tab config to service: tabId=\(tab.id.uuidString.prefix(8))\n", stderr)

        // Build tab config dictionary
        var tabConfig: [String: Any] = [
            "tabId": tab.id.uuidString,
            "title": tab.title,
            "backgroundColor": tab.backgroundColor ?? "#1a1a2e",
            "containerWidth": containerSize.width,
            "containerHeight": containerSize.height
        ]

        // Add optional scripts
        if let unifiedScript = tab.unifiedScript {
            tabConfig["unifiedScript"] = unifiedScript
        }
        if let leftPanelScript = tab.leftPanelScript {
            tabConfig["leftPanelScript"] = leftPanelScript
        }
        if let rightPanelScript = tab.rightPanelScript {
            tabConfig["rightPanelScript"] = rightPanelScript
        }
        if let guestScript = tab.guestScript {
            tabConfig["guestScript"] = guestScript
        }

        // Add system app type if present (RN expects "systemAppType")
        if let systemTabType = tab.systemTabType {
            tabConfig["systemAppType"] = systemTabType.rawValue
        }

        // Serialize to JSON and send
        if let jsonData = try? JSONSerialization.data(withJSONObject: tabConfig),
           let jsonString = String(data: jsonData, encoding: .utf8) {
            fputs("📤 Tab config JSON: \(jsonString.prefix(200))...\n", stderr)

            DistributedNotificationCenter.default().postNotificationName(
                NSNotification.Name("com.canvas.tabConfig"),
                object: nil,
                userInfo: ["config": jsonString],
                deliverImmediately: true
            )
        }
    }

    private func errorView(_ message: String) -> some View {
        VStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 32))
                .foregroundColor(.orange)
            Text("Connection Error")
                .font(.headline)
            Text(message)
                .font(.system(size: 12))
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}
