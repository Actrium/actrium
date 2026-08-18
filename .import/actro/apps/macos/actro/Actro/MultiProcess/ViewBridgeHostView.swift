//
//  ViewBridgeHostView.swift
//  Actro
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
import SwiftUI
import Combine

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
    let line = "[\(timestamp)] [HOST] \(message)\n"
    NSLog("[\(timestamp)] [HOST] %@", message)
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

// MARK: - ViewBridge Host View

/// A view that hosts content from a ViewBridge XPC service.
/// Remote view size 跟随 TabPanel 实际尺寸，RN 自适应布局。
public class ViewBridgeHostView: NSView {

    // MARK: - Properties

    /// The remote view controller received from the service
    private var remoteViewController: NSViewController?

    /// The service bundle identifier
    private let serviceBundleIdentifier: String

    /// Public accessor for service bundle identifier (used by coordinator for re-sending pendingTabId)
    var serviceId: String { serviceBundleIdentifier }

    /// The view controller class name in the service
    private let serviceViewControllerClassName: String

    /// Loading state
    private var isLoading = false

    /// Current content size - 跟随 TabPanel 实际尺寸
    /// Accessible by coordinator for sending size notifications
    var currentContentSize: CGSize = CGSize(width: 1280, height: 720)

    /// Callback when XPC connection is established
    public var onConnectionEstablished: (() -> Void)?

    /// Check if remote view controller is connected (public accessor)
    public var hasRemoteViewController: Bool {
        remoteViewController != nil
    }

    /// Reference to coordinator for lifecycle management
    /// When SwiftUI replaces the view, we only disconnect if coordinator.hostView still points to us
    weak var coordinator: ViewBridgeCoordinator?

    /// 防重复：上次发送的尺寸
    private var lastSentNotificationSize: CGSize?

    /// Resize start size capture
    private var resizeStartContentSize: NSSize?

    /// 使用 flipped 坐标系，与 ContainerView 和 SwiftUI 保持一致
    /// 确保 remoteView 从左上角 (0,0) 开始正确布局
    public override var isFlipped: Bool { true }

    /// 是否允许 hit testing（Overview 模式下为 false，让手势穿透到下层 Grid）
    /// 从 coordinator.isOverviewMode 获取状态
    var allowsHitTesting: Bool = true {
        didSet {
            // 🔑 在 Overview 模式下禁用拖拽目标，让拖拽事件穿透到下层 Grid
            if !allowsHitTesting {
                unregisterDraggedTypes()
            }
        }
    }

    /// Override hitTest to allow events to pass through in Overview mode
    /// This enables tab dragging in Overview mode
    public override func hitTest(_ point: NSPoint) -> NSView? {
        // In Overview mode, let events pass through to the underlying Grid
        if !allowsHitTesting {
            return nil
        }
        return super.hitTest(point)
    }

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
        ipcLog("🆕 ViewBridgeHostView init: service=\(bundleIdentifier.components(separatedBy: ".").last ?? "?")")
    }

    /// Convenience initializer with default Actro service
    public convenience init() {
        self.init(
            serviceBundleIdentifier: "io.actrium.actro.viewservice",
            viewControllerClassName: "ActroServiceViewController"
        )
    }

    /// Convenience initializer with initial content size
    public convenience init(initialContentSize: CGSize) {
        self.init()
        self.currentContentSize = initialContentSize
        NSLog("ViewBridgeHostView: init with contentSize=%.1fx%.1f", initialContentSize.width, initialContentSize.height)
    }

    /// Convenience initializer with custom bundle identifier and content size
    /// Used by XPC service pool for multi-process isolation
    public convenience init(serviceBundleIdentifier: String, initialContentSize: CGSize) {
        self.init(
            serviceBundleIdentifier: serviceBundleIdentifier,
            viewControllerClassName: "ActroServiceViewController"
        )
        self.currentContentSize = initialContentSize
        ipcLog("📐 ViewBridgeHostView: contentSize=\(Int(initialContentSize.width))x\(Int(initialContentSize.height))")
    }

    required init?(coder: NSCoder) {
        self.serviceBundleIdentifier = "io.actrium.actro.viewservice"
        self.serviceViewControllerClassName = "ActroServiceViewController"
        super.init(coder: coder)
        setupView()
    }

    // MARK: - Coordinate System

    // 注意：ViewBridgeHostView 使用标准 AppKit 坐标系
    // ContainerView 是 flipped 的，所以 hostView 会被放在左上角
    // 但 hostView 内部使用标准坐标系，这样 NSRemoteView 的内容才能正确显示

    // MARK: - Setup

    private func setupView() {
        wantsLayer = true
        layer?.masksToBounds = true  // 裁剪超出边界的内容，避免拖尾
        // 禁用 layer 隐式动画
        layer?.actions = ["bounds": NSNull(), "position": NSNull(), "frame": NSNull()]

        // Disable automatic subview resizing to allow manual control in updateRemoteViewLayout
        autoresizesSubviews = false

        // Register for live resize notifications to capture start size
        NotificationCenter.default.addObserver(forName: .windowLiveResizeStarted, object: nil, queue: .main) { [weak self] _ in
            guard let self = self else { return }
            // Capture size at start of drag
            self.resizeStartContentSize = self.bounds.size
            // Also ensure we set this immediately to prevent initial jump
            if let remoteView = self.remoteViewController?.view {
                // Force sync immediate state
                let current = remoteView.frame.size
                if current.width > 0 && current.height > 0 {
                     self.resizeStartContentSize = current
                }
            }
        }

        NotificationCenter.default.addObserver(forName: .windowLiveResizeEnded, object: nil, queue: .main) { [weak self] _ in
            self?.resizeStartContentSize = nil
            // Force a final layout update to restore full size
            self?.needsLayout = true
        }

        NSLog("ViewBridgeHostView: Initialized for service \(serviceBundleIdentifier)")
        ipcLog("🆕 ViewBridgeHostView init for service=\(serviceBundleIdentifier.components(separatedBy: ".").last ?? "?")")
    }

    // MARK: - Size Control

    /// 返回 intrinsicContentSize
    public override var intrinsicContentSize: NSSize {
        return currentContentSize
    }

    /// 布局时确保 remoteView 填满整个 hostView
    public override func layout() {
        super.layout()
        // 🔑 强制使用 bounds.size 同步 remoteView，不依赖 currentContentSize
        if let remoteView = remoteViewController?.view {
            let targetSize = bounds.size
            if targetSize.width > 0 && targetSize.height > 0 {
                let expectedFrame = NSRect(origin: .zero, size: targetSize)
                if remoteView.frame != expectedFrame {
                    // 禁用隐式动画
                    CATransaction.begin()
                    CATransaction.setDisableActions(true)
                    remoteView.frame = expectedFrame
                    CATransaction.commit()
                }
            }
        }
    }

    /// 捕获直接设置 frame 的情况
    /// 🔑 关键：每次 frame 变化都立即同步 remoteView，不做节流
    /// Safari/Chrome 使用同进程渲染没有这个问题，我们必须尽可能快地同步
    public override var frame: NSRect {
        didSet {
            // 任何尺寸变化都立即同步 remoteView
            let remoteFrame = remoteViewController?.view.frame ?? .zero
            if abs(remoteFrame.width - frame.width) > 0.5 || abs(remoteFrame.height - frame.height) > 0.5 {
                // 禁用隐式动画
                CATransaction.begin()
                CATransaction.setDisableActions(true)
                remoteViewController?.view.frame = NSRect(origin: .zero, size: frame.size)
                CATransaction.commit()

                // 同时更新 currentContentSize，确保下次 updateRemoteViewLayout 使用正确的值
                currentContentSize = frame.size
            }
        }
    }

    /// 接受外部尺寸设置
    public override func setFrameSize(_ newSize: NSSize) {
        let oldSize = frame.size
        let remoteOldSize = remoteViewController?.view.frame.size ?? .zero

        // 🔑 关键修复：检测并拒绝 @2x 像素尺寸
        // AppKit 在某些情况下（wantsLayer=true）会传入 @2x 像素尺寸
        // 如果接受这个尺寸，hostView 会占据 @2x 空间，但 remoteView 只有 1x
        let scaleFactor = window?.backingScaleFactor ?? 2.0  // 默认假设 Retina

        // 检测：如果 newSize 明显大于 currentContentSize（超过 1.5x），可能是 @2x
        // 使用 superview.bounds 作为参考（superview 尺寸应该是正确的 point 尺寸）
        // 🔑 修正：如果 superview.bounds 太小（例如还没更新），则不作为参考
        // 只有当 superview.bounds 看起来"合理"（不是 0 且不比 currentContentSize 小太多）才使用
        let referenceSize = superview?.bounds.size ?? currentContentSize
        let isReferenceValid = referenceSize.width > 10 && referenceSize.width >= currentContentSize.width * 0.8
        let effectiveReference = isReferenceValid ? referenceSize : currentContentSize

        let looksLikePixels = scaleFactor > 1.0 &&
            effectiveReference.width > 10 &&
            newSize.width > effectiveReference.width * 1.5 &&
            newSize.height > effectiveReference.height * 1.5

        if looksLikePixels {
            // 拒绝 @2x 尺寸，使用 superview.bounds 作为正确尺寸
            // 如果 reference 不可用，则保持 currentContentSize 或使用 newSize / scale
            let fallbackSize: NSSize
            if isReferenceValid {
                fallbackSize = effectiveReference
            } else {
                fallbackSize = NSSize(width: newSize.width / scaleFactor, height: newSize.height / scaleFactor)
            }
            ipcLog("⚠️ setFrameSize: rejected @2x size \(Int(newSize.width))x\(Int(newSize.height)), using fallback \(Int(fallbackSize.width))x\(Int(fallbackSize.height))")

            CATransaction.begin()
            CATransaction.setDisableActions(true)
            super.setFrameSize(fallbackSize)
            CATransaction.commit()
        } else {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            super.setFrameSize(newSize)
            CATransaction.commit()
        }

        // 使用 bounds.size 更新 remoteView（bounds 是 point 尺寸）
        updateRemoteViewLayoutWithBounds()

        // 🔍 诊断日志：检查 setFrameSize 的调用
        let remoteNewSize = remoteViewController?.view.frame.size ?? .zero
        if abs(newSize.width - oldSize.width) > 1 || abs(newSize.height - oldSize.height) > 1 {
            ipcLog("🔧 setFrameSize: \(Int(oldSize.width))x\(Int(oldSize.height)) → \(Int(newSize.width))x\(Int(newSize.height)), bounds=\(Int(bounds.width))x\(Int(bounds.height)), remote: \(Int(remoteOldSize.width))x\(Int(remoteOldSize.height)) → \(Int(remoteNewSize.width))x\(Int(remoteNewSize.height)), currentContentSize=\(Int(currentContentSize.width))x\(Int(currentContentSize.height))")
        }
    }

    /// 使用 bounds.size 更新 remoteView frame（用于 setFrameSize 回调）
    private func updateRemoteViewLayoutWithBounds() {
        guard let remoteView = remoteViewController?.view else { return }

        // bounds.size 是 point 尺寸（与 frame.size 相同，但更可靠）
        let targetSize = bounds.size
        guard targetSize.width > 0 && targetSize.height > 0 else { return }

        let expectedFrame = NSRect(origin: .zero, size: targetSize)
        if remoteView.frame != expectedFrame {
            ipcLog("🔧 updateRemoteViewLayoutWithBounds: resizing remoteView \(Int(remoteView.frame.width))x\(Int(remoteView.frame.height)) → \(Int(targetSize.width))x\(Int(targetSize.height))")

            // 禁用隐式动画
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            remoteView.frame = expectedFrame
            CATransaction.commit()

            // 确保 currentContentSize 也同步更新
            if currentContentSize != targetSize {
                currentContentSize = targetSize
            }
        }
    }

    /// Update remote view frame to fill bounds
    private func updateRemoteViewLayout() {
        guard let remoteView = remoteViewController?.view else { return }

        // 🔑 关键修复：使用 currentContentSize 而不是 bounds.size
        // bounds.size 在 Retina 显示器上可能返回 @2x 像素尺寸
        // currentContentSize 是由 SwiftUI 正确计算的 point 尺寸
        let targetSize = currentContentSize

        // 忽略无效尺寸
        guard targetSize.width > 0 && targetSize.height > 0 else {
            return
        }

        // 强制从 (0,0) 开始，确保填满整个 hostView
        let expectedFrame = NSRect(origin: .zero, size: targetSize)
        if remoteView.frame != expectedFrame {
            // 禁用隐式动画
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            remoteView.frame = expectedFrame
            CATransaction.commit()
        }
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

        NSLog("ViewBridgeHostView: Requesting \(serviceViewControllerClassName) from \(serviceBundleIdentifier)")

        // Live tabs use RendererProcessPool. Keep the legacy ViewBridge
        // request path for leftover snapshot helpers only.
        requestRemoteViewControllerDirect()
    }

    /// 用于捕获新连接的 internalId
    private var pendingConnectionInternalId: String?
    private var connectionCreatedObserverForDirect: NSObjectProtocol?

    /// 直接请求远程 ViewController（不使用 prewarm）
    private func requestRemoteViewControllerDirect() {
        NSLog("ViewBridgeHostView: Requesting remote ViewController directly... (no prewarm)")

        // 🔑 设置监听器来捕获新连接的 internalId
        // 这样即使 prewarm 已经创建了 entry，我们也能通过 bindTab 正确绑定
        setupConnectionCreatedListener()

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
                self?.handleConnectionResultWithBinding(viewController: viewController, error: error)
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

    /// 设置监听器捕获新连接的 internalId
    private func setupConnectionCreatedListener() {
        // 移除旧的监听器
        if let observer = connectionCreatedObserverForDirect {
            DistributedNotificationCenter.default().removeObserver(observer)
        }

        pendingConnectionInternalId = nil

        connectionCreatedObserverForDirect = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.actro.xpcConnectionCreated"),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self = self,
                  let userInfo = notification.userInfo,
                  let bundleId = userInfo["bundleId"] as? String,
                  let internalId = userInfo["internalId"] as? String else { return }

            // 过滤：只处理本 HostView 对应的 service
            guard bundleId == self.serviceBundleIdentifier else { return }

            // 过滤：只处理属于本进程的通知
            if let hostPid = userInfo["hostProcessId"] as? Int32,
               hostPid != ActroHostProcessId {
                return
            }

            NSLog("ViewBridgeHostView: 🔗 Captured internalId=%@ for direct connection", String(internalId.prefix(8)))
            self.pendingConnectionInternalId = internalId

            // 移除监听器（只需要捕获一次）
            if let observer = self.connectionCreatedObserverForDirect {
                DistributedNotificationCenter.default().removeObserver(observer)
                self.connectionCreatedObserverForDirect = nil
            }
        }
    }

    /// 处理新连接结果，并发送 bindTab 通知
    private func handleConnectionResultWithBinding(viewController: NSViewController?, error: Error?) {
        // 等待一小段时间让 xpcConnectionCreated 通知到达
        // 使用 RunLoop 确保通知能被处理
        let startTime = Date()
        while pendingConnectionInternalId == nil && Date().timeIntervalSince(startTime) < 0.5 {
            RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.01))
        }

        // 发送 bindTab 通知（如果有 internalId）
        if let internalId = pendingConnectionInternalId, let tabId = coordinator?.tabId {
            let bindName = "com.actro.bindTab.\(ActroHostProcessId).\(serviceBundleIdentifier)"
            DistributedNotificationCenter.default().postNotificationName(
                NSNotification.Name(bindName),
                object: nil,
                userInfo: [
                    RuntimeBridgeContract.tabId: tabId.uuidString,
                    "internalId": internalId,
                    "hostProcessId": ActroHostProcessId
                ],
                deliverImmediately: true
            )
            NSLog("ViewBridgeHostView: 📤 Sent bindTab (direct): tabId=%@, internalId=%@",
                  String(tabId.uuidString.prefix(8)), String(internalId.prefix(8)))
        } else {
            NSLog("⚠️ ViewBridgeHostView: No internalId captured for direct connection, relying on FIFO")
        }

        // 清理
        pendingConnectionInternalId = nil
        if let observer = connectionCreatedObserverForDirect {
            DistributedNotificationCenter.default().removeObserver(observer)
            connectionCreatedObserverForDirect = nil
        }

        // 继续原来的处理
        handleConnectionResult(viewController: viewController, error: error)
    }

    // MARK: - Connection Handling

    private func handleConnectionResult(viewController: NSViewController?, error: Error?) {
        isLoading = false

        if let error = error {
            handleError("Connection failed: \(error.localizedDescription)")
            return
        }

        guard let vc = viewController else {
            handleError("No view controller returned")
            return
        }

        NSLog("ViewBridgeHostView: Received remote view controller: \(type(of: vc))")

        // Embed the remote view controller's view
        remoteViewController = vc

        let remoteView = vc.view

        // 使用 autoresizingMask 而非 Auto Layout
        // Auto Layout 与 flipped 坐标系可能有交互问题
        let targetFrame = NSRect(origin: .zero, size: currentContentSize)
        remoteView.frame = targetFrame
        // Disable autoresizing mask as we manage layout manually
        remoteView.autoresizingMask = []

        // 🔑 确保 remoteView 的 layer 坐标系匹配 flipped 父视图
        remoteView.wantsLayer = true
        remoteView.layer?.isGeometryFlipped = true
        // 禁用 remoteView layer 的隐式动画
        remoteView.layer?.actions = ["bounds": NSNull(), "position": NSNull(), "frame": NSNull()]

        addSubview(remoteView)

        // Configure ViewBridge content size range
        let maxSize = CGSize(width: 4096, height: 4096)
        let minSize = CGSize(width: 50, height: 50)
        let updateSelector = NSSelectorFromString("updateContentMinSize:maxSize:")

        // Try to set size range on VC
        if vc.responds(to: updateSelector) {
            let method = (vc as AnyObject).method(for: updateSelector)
            typealias UpdateSizeFunc = @convention(c) (AnyObject, Selector, CGSize, CGSize) -> Void
            let updateFunc = unsafeBitCast(method, to: UpdateSizeFunc.self)
            updateFunc(vc as AnyObject, updateSelector, minSize, maxSize)
        }

        // Try to set size range on View
        if remoteView.responds(to: updateSelector) {
            let method = remoteView.method(for: updateSelector)
            typealias UpdateSizeFunc = @convention(c) (AnyObject, Selector, CGSize, CGSize) -> Void
            let updateFunc = unsafeBitCast(method, to: UpdateSizeFunc.self)
            updateFunc(remoteView, updateSelector, minSize, maxSize)
        }

        // Try to set on VC.remoteView property
        let remoteViewSelector = NSSelectorFromString("remoteView")
        if vc.responds(to: remoteViewSelector),
           let actualRemoteView = vc.perform(remoteViewSelector)?.takeUnretainedValue() as? NSView,
           actualRemoteView.responds(to: updateSelector) {
            let method = actualRemoteView.method(for: updateSelector)
            typealias UpdateSizeFunc = @convention(c) (AnyObject, Selector, CGSize, CGSize) -> Void
            let updateFunc = unsafeBitCast(method, to: UpdateSizeFunc.self)
            updateFunc(actualRemoteView, updateSelector, minSize, maxSize)
        }

        // 确保 ViewBridgeHostView 自身也使用正确的尺寸
        super.setFrameSize(currentContentSize)

        NSLog("ViewBridgeHostView: Remote view embedded, size=%.0fx%.0f", currentContentSize.width, currentContentSize.height)

        // 通知连接已建立 - coordinator 会处理重发 pendingTabId
        onConnectionEstablished?()
    }

    // MARK: - Content Size Management

    /// 更新本地 frame 和 preferredContentSize
    /// 由 coordinator.applyViewportSize 调用，不发送通知（通知由 coordinator 统一处理）
    func applyFrameSize(_ newSize: CGSize) {
        currentContentSize = newSize

        // 🔑 关键修复：同步更新 hostView 和 remoteView 的 frame
        // 之前只更新 remoteView，导致 remoteView 超出 hostView 边界被裁剪
        // 现在同时更新 hostView 的 frame，确保两者一致
        let newFrame = NSRect(origin: frame.origin, size: newSize)
        if frame.size != newSize {
            frame = newFrame
        }

        // 更新 remoteView frame
        updateRemoteViewLayout()

        let isLiveResizing = WindowResizeState.shared.isLiveResizing

        // 同步到 NSRemoteViewController（始终更新，让 Service 知道目标尺寸）
        if let vc = remoteViewController {
            let current = vc.preferredContentSize
            if abs(current.width - newSize.width) > 0.5 || abs(current.height - newSize.height) > 0.5 {
                vc.preferredContentSize = newSize
            }
        }

        // 调试日志 - 包含 origin 信息以诊断位置问题
        let remoteFrame = remoteViewController?.view.frame ?? .zero
        let remoteBounds = remoteViewController?.view.bounds ?? .zero
        let selfFrame = frame
        let selfBounds = bounds
        let superFrame = superview?.frame ?? .zero
        let superBounds = superview?.bounds ?? .zero
        let windowFrame = window?.frame ?? .zero
        let convertedOrigin = superview?.convert(frame.origin, to: nil) ?? .zero  // 转换到窗口坐标

        // 🔍 检查 layer 状态
        let remoteLayer = remoteViewController?.view.layer
        let layerBounds = remoteLayer?.bounds ?? .zero
        let layerFrame = remoteLayer?.frame ?? .zero
        let layerTransform = remoteLayer?.transform ?? CATransform3DIdentity
        let isIdentityTransform = CATransform3DIsIdentity(layerTransform)

        ipcLog("📐 applyFrameSize: newSize=\(Int(newSize.width))x\(Int(newSize.height)) self.frame=(\(Int(selfFrame.origin.x)),\(Int(selfFrame.origin.y)),\(Int(selfFrame.width))x\(Int(selfFrame.height))) self.bounds=\(Int(selfBounds.width))x\(Int(selfBounds.height)) remoteFrame=(\(Int(remoteFrame.origin.x)),\(Int(remoteFrame.origin.y)),\(Int(remoteFrame.width))x\(Int(remoteFrame.height))) remoteBounds=\(Int(remoteBounds.width))x\(Int(remoteBounds.height)) layer.frame=\(Int(layerFrame.width))x\(Int(layerFrame.height)) layer.bounds=\(Int(layerBounds.width))x\(Int(layerBounds.height)) identityTransform=\(isIdentityTransform)")

        // 🔑 强制刷新 remoteView，确保 NSRemoteView 重新渲染
        // 这在窗口 zoom 动画后特别重要
        forceRemoteViewRefresh()

        // 🔍 延迟检查：查看是否有回调修改了尺寸
        let expectedSize = newSize
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            let actualSelfFrame = self.frame
            let actualRemoteFrame = self.remoteViewController?.view.frame ?? .zero
            if abs(actualSelfFrame.width - expectedSize.width) > 1 ||
               abs(actualSelfFrame.height - expectedSize.height) > 1 ||
               abs(actualRemoteFrame.width - expectedSize.width) > 1 ||
               abs(actualRemoteFrame.height - expectedSize.height) > 1 {
                ipcLog("⚠️ POST-applyFrameSize: SIZE CHANGED! expected=\(Int(expectedSize.width))x\(Int(expectedSize.height)) actual.self=\(Int(actualSelfFrame.width))x\(Int(actualSelfFrame.height)) actual.remote=\(Int(actualRemoteFrame.width))x\(Int(actualRemoteFrame.height))")
            }
        }
    }

    private func handleError(_ message: String) {
        NSLog("ViewBridgeHostView: Error - \(message)")
        isLoading = false
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
        // Only disconnect if the coordinator still points to us
        // When SwiftUI replaces the view, coordinator.hostView will point to the new view
        if coordinator == nil || coordinator?.hostView === self {
            disconnect()
        } else {
            NSLog("ViewBridgeHostView: deinit skipped disconnect (replaced by new view)")
        }
    }

    // MARK: - Diagnostics

    /// 强制刷新 remoteView 显示
    /// 使用多种策略确保 NSRemoteView 重新渲染其内容
    func forceRemoteViewRefresh() {
        guard let remoteView = remoteViewController?.view else { return }

        let targetSize = bounds.size
        let remoteFrame = remoteView.frame
        ipcLog("🔄 forceRemoteViewRefresh: hostBounds=\(Int(targetSize.width))x\(Int(targetSize.height)) remoteFrame=\(Int(remoteFrame.width))x\(Int(remoteFrame.height))")

        // 🔑 策略 0: "Frame Jitter" - 先设置不同尺寸再设置正确尺寸
        // 这可以强制 NSRemoteView 重新同步其内容，打破可能的缓存
        let jitterSize = CGSize(width: targetSize.width - 1, height: targetSize.height - 1)
        remoteView.frame = NSRect(origin: .zero, size: jitterSize)
        remoteView.frame = NSRect(origin: .zero, size: targetSize)
        ipcLog("🔧 forceRemoteViewRefresh: jitter applied, final frame=\(Int(targetSize.width))x\(Int(targetSize.height))")

        // 策略 1: 标记 view 需要重新显示和布局
        remoteView.needsDisplay = true
        remoteView.needsLayout = true

        // 策略 2: 同时刷新 layer
        if let layer = remoteView.layer {
            layer.setNeedsDisplay()
            layer.setNeedsLayout()
        }

        // 策略 3: 遍历子视图，强制它们也刷新
        for subview in remoteView.subviews {
            subview.needsDisplay = true
            subview.needsLayout = true
            subview.layer?.setNeedsDisplay()
        }

        // 策略 4: 触发 window 重绘
        remoteView.window?.displayIfNeeded()

        // 策略 5: 延迟再次刷新（确保动画完成后）
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in
            guard let self = self else { return }
            guard let remoteView = self.remoteViewController?.view else { return }
            let finalSize = self.bounds.size
            // 再次 jitter
            let jitter = CGSize(width: finalSize.width - 1, height: finalSize.height - 1)
            remoteView.frame = NSRect(origin: .zero, size: jitter)
            remoteView.frame = NSRect(origin: .zero, size: finalSize)
            ipcLog("🔧 forceRemoteViewRefresh (delayed): jitter applied, final frame=\(Int(finalSize.width))x\(Int(finalSize.height))")
            remoteView.needsDisplay = true
            remoteView.needsLayout = true
            remoteView.layer?.setNeedsDisplay()
            remoteView.window?.displayIfNeeded()
        }
    }

    /// 打印视图层级的 frame 状态（用于调试 resize 问题）
    func logViewHierarchy(prefix: String) {
        let containerFrame = superview?.frame ?? .zero
        let hostFrame = frame
        let remoteFrame = remoteViewController?.view.frame ?? .zero
        let remoteBounds = remoteViewController?.view.bounds ?? .zero
        // 检查 remoteView 的子视图
        var subviewInfo = ""
        if let remoteView = remoteViewController?.view {
            for (i, subview) in remoteView.subviews.enumerated() {
                subviewInfo += " sub[\(i)]=(\(Int(subview.frame.origin.x)),\(Int(subview.frame.origin.y)),\(Int(subview.frame.width))x\(Int(subview.frame.height)))"
            }
        }
        ipcLog("🔍 \(prefix): container=(\(Int(containerFrame.origin.x)),\(Int(containerFrame.origin.y)),\(Int(containerFrame.width))x\(Int(containerFrame.height))) host=(\(Int(hostFrame.origin.x)),\(Int(hostFrame.origin.y)),\(Int(hostFrame.width))x\(Int(hostFrame.height))) remote=(\(Int(remoteFrame.origin.x)),\(Int(remoteFrame.origin.y)),\(Int(remoteFrame.width))x\(Int(remoteFrame.height))) remoteBounds=\(Int(remoteBounds.width))x\(Int(remoteBounds.height))\(subviewInfo)")
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

// MARK: - Live Resize State

/// 全局窗口 resize 状态 - 拖动期间抑制跨进程 IPC 通知
final class WindowResizeState: ObservableObject {
    static let shared = WindowResizeState()
    private init() {}

    /// 是否正在 live resize（拖动窗口边缘）
    @Published var isLiveResizing: Bool = false

    /// 当前正在 resize 的 tab ID（用于匹配遮罩显示）- Focus 模式
    @Published var activeTabId: UUID?

    /// 是否正在等待 Service 端的 sizeApplied 通知
    /// true = 显示快照遮罩，false = 隐藏遮罩
    @Published var isWaitingForSizeApplied: Bool = false
}

extension Notification.Name {
    static let windowLiveResizeStarted = Notification.Name("windowLiveResizeStarted")
    static let windowLiveResizeEnded = Notification.Name("windowLiveResizeEnded")
}

/// Coordinator that holds reference to ViewBridgeHostView
/// 管理 XPC 连接状态，生命周期由 ViewBridgeCoordinatorManager 管理
/// 集中处理所有尺寸同步逻辑 - 这是 Host 端尺寸更新的唯一入口
final class ViewBridgeCoordinator {
    weak var hostView: ViewBridgeHostView?
    /// Browser-style Renderer host. During migration this coexists with the old
    /// ViewBridge reference, but a Tab owns at most one active rendering path.
    weak var rendererHostView: RendererSurfaceHostView?
    /// Callback when XPC connection is established
    var onConnectionEstablished: (() -> Void)?

    // XPC 状态 - 移到这里避免 SwiftUI @State 被重置
    // 🔑 四个条件都满足时才能发送初始 config：
    //    1. allocatedServiceId != nil（已分配 XPC Service）
    //    2. xpcServiceReady == true（Service 已 ready）
    //    3. hasConnection == true（ViewBridge 连接已建立）
    //    4. currentViewportSize 有效（> 10x10）

    /// 已分配的 XPC Service ID
    /// 使用 didSet 在分配后自动尝试发送初始 config
    var allocatedServiceId: String? {
        didSet {
            if allocatedServiceId != nil && oldValue == nil {
                let tabIdShort = tabId?.uuidString.prefix(8) ?? "nil"
                let serviceShort = allocatedServiceId?.components(separatedBy: ".").last ?? "nil"
                ipcLog("🎯 allocatedServiceId=\(serviceShort) for tab=\(tabIdShort)")
                trySendInitialConfig()
            }
        }
    }

    /// XPC Service 是否已 ready（收到 xpcServiceReady 通知）
    /// 使用 didSet 在状态变化时自动尝试发送初始 config
    var xpcServiceReady: Bool = false {
        didSet {
            if xpcServiceReady && !oldValue {
                let tabIdShort = tabId?.uuidString.prefix(8) ?? "nil"
                ipcLog("📡 xpcServiceReady=true for tab=\(tabIdShort)")
                trySendInitialConfig()
            }
        }
    }

    /// ViewBridge 连接是否已建立
    /// 使用 didSet 在状态变化时自动尝试发送初始 config
    var hasConnection: Bool = false {
        didSet {
            if hasConnection && !oldValue {
                let tabIdShort = tabId?.uuidString.prefix(8) ?? "nil"
                ipcLog("🔗 hasConnection=true for tab=\(tabIdShort)")

                // 🔑 连接建立后立即发送 bootstrap tabConfig（不带 PID）
                // 这个通知让 Service 获取 hostProcessId 并设置带 PID 的监听器
                // 同时包含 tabId，替代之前的 pendingTabId 机制
                sendBootstrapTabConfig()

                trySendInitialConfig()
            }
        }
    }

    /// 发送 bootstrap tabConfig（不带 hostProcessId 前缀）
    /// 用于在 ViewBridge 连接建立后立即通知 Service：
    /// 1. hostProcessId - 让 Service 能设置带 PID 的监听器
    /// 2. tabId - 让 Service 绑定 pending entry
    /// 3. 基本配置 - 让 Service 可以开始渲染
    private func sendBootstrapTabConfig() {
        // Renderer IPC has an explicit request/response handshake and does not
        // need the legacy DistributedNotification bootstrap.
        if rendererHostView != nil {
            return
        }
        guard let tabId = tabId,
              let serviceId = allocatedServiceId else {
            ipcLog("⚠️ sendBootstrapTabConfig: missing tabId or allocatedServiceId")
            return
        }

        let size = currentViewportSize
        let tabIdShort = String(tabId.uuidString.prefix(8))

        // 构建 bootstrap config（包含 hostProcessId）
        var config: [String: Any] = [
            RuntimeBridgeContract.tabId: tabId.uuidString,
            "hostProcessId": ActroHostProcessId,
            RuntimeBridgeContract.containerWidth: size.width,
            RuntimeBridgeContract.containerHeight: size.height,
            "isOverviewMode": isOverviewMode,
            "isBootstrap": true  // 标记这是 bootstrap config
        ]

        // 使用 coordinator 存储的完整 tab 配置
        let unifiedScriptPreview = tabConfig?.unifiedScript.map { String($0.prefix(50)) + "..." } ?? "nil"
        ipcLog("📋 sendBootstrapTabConfig: tabConfig=\(tabConfig != nil ? "SET" : "NIL"), systemAppType=\(tabConfig?.systemAppType ?? "nil"), unifiedScript=\(unifiedScriptPreview), isOverviewMode=\(isOverviewMode), bookmarks=\(tabConfig?.bookmarks.count ?? 0), history=\(tabConfig?.recentHistory.count ?? 0)")

        // 🔑 如果 tabConfig 还没设置，标记为待发送，等 tabConfig 设置后再发
        guard let tabConfig = self.tabConfig else {
            pendingBootstrap = true
            ipcLog("⏳ sendBootstrapTabConfig: tabConfig not set, marking pending")
            return
        }

        if true {  // keep scope for tabConfig
            config[RuntimeBridgeContract.title] = tabConfig.title
            config["backgroundColor"] = tabConfig.backgroundColor
            if let systemAppType = tabConfig.systemAppType {
                config[RuntimeBridgeContract.systemAppType] = systemAppType
            }
            if let script = tabConfig.unifiedScript { config["unifiedScript"] = script }
            if let script = tabConfig.leftPanelScript { config["leftPanelScript"] = script }
            config["leftPanelVisible"] = tabConfig.leftPanelVisible
            config["rightPanelVisible"] = tabConfig.rightPanelVisible
            config["reloadKey"] = tabConfig.reloadKey
            // 🔑 StartPage 需要的数据
            if !tabConfig.bookmarks.isEmpty { config["bookmarks"] = tabConfig.bookmarks }
            if !tabConfig.recentHistory.isEmpty { config["recentHistory"] = tabConfig.recentHistory }
        }

        guard let jsonData = try? JSONSerialization.data(withJSONObject: config),
              let jsonString = String(data: jsonData, encoding: .utf8) else {
            ipcLog("⚠️ sendBootstrapTabConfig: JSON serialization failed")
            return
        }

        // 🔑 使用不带 PID 的通知名，确保 Service 能立即收到
        let notificationName = "com.actro.tabConfig.\(serviceId)"
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: ["config": jsonString],
            deliverImmediately: true
        )

        ipcLog("📤 Sent bootstrap tabConfig: tab=\(tabIdShort) size=\(Int(size.width))x\(Int(size.height)) notificationName=\(notificationName)")
    }
    var xpcReadyObserver: NSObjectProtocol?
    var connectionCreatedObserver: NSObjectProtocol?
    var liveResizeEndedObserver: NSObjectProtocol?
    /// Observer for Service's "listener ready" notification (to resolve race condition)
    var listenerReadyObserver: NSObjectProtocol?
    var tabId: UUID?

    /// Tab 配置数据 - 用于发送 bootstrap tabConfig
    /// 由 ViewBridgeMultiProcessTabView 初始化时设置
    var tabConfig: XPCTabConfig? {
        didSet {
            // 🔑 如果 bootstrap 之前因为缺少 tabConfig 而失败，现在重试
            if tabConfig != nil && pendingBootstrap && hasConnection {
                pendingBootstrap = false
                ipcLog("📤 Retrying bootstrap after tabConfig set")
                sendBootstrapTabConfig()
            }
        }
    }

    /// 标记是否有待发送的 bootstrap（因为 tabConfig 还没设置而延迟）
    private var pendingBootstrap = false

    /// Whether the tab is in Overview mode (for compact view in system apps)
    /// Default to false since BrowserViewModel.init sets actroMode = .focus
    var isOverviewMode: Bool = false

    /// 当前 viewport 尺寸 - 存储在 coordinator 中以便闭包能访问最新值
    /// 默认值 50x50 反映 Overview 模式下可能的小卡片尺寸
    /// 使用 didSet 在尺寸首次变为有效值时尝试发送初始 config
    var currentViewportSize: CGSize = CGSize(width: 50, height: 50) {
        didSet {
            // 只在尺寸从无效变为有效时触发（避免频繁触发）
            let wasValid = oldValue.width > 10 && oldValue.height > 10
            let isValid = currentViewportSize.width > 10 && currentViewportSize.height > 10
            if isValid && !wasValid {
                let tabIdShort = tabId?.uuidString.prefix(8) ?? "nil"
                ipcLog("📐 viewportSize valid for tab=\(tabIdShort): \(Int(currentViewportSize.width))x\(Int(currentViewportSize.height))")
                trySendInitialConfig()
            }
        }
    }
    /// 上次发送 tabConfig 时的尺寸 - 用于防止重复发送
    var lastSentConfigSize: CGSize?
    /// 上次发送 tabConfig 的 reloadKey - 用于防止重复发送
    var lastSentReloadKey: Int?
    /// 上次发送 containerSizeChanged 时的尺寸 - 用于防止重复发送
    private var lastSentContainerSize: CGSize?

    /// Reset lastSentContainerSize to force re-send notification
    /// Called when isOverviewMode changes to ensure Service receives the update
    func resetLastSentContainerSize() {
        lastSentContainerSize = nil
    }

    /// 🔑 尝试发送初始 tab config
    /// 检查四个条件：allocatedServiceId、xpcServiceReady、hasConnection、viewportSize
    /// 任何一个 didSet 触发时都会调用此方法，当所有条件满足时发送 config
    func trySendInitialConfig() {
        let tabIdShort = tabId?.uuidString.prefix(8) ?? "nil"
        let size = currentViewportSize
        let sizeValid = size.width > 10 && size.height > 10

        // 打印当前状态
        ipcLog("🔍 trySendInitialConfig tab=\(tabIdShort): serviceId=\(allocatedServiceId != nil ? "✓" : "✗"), xpcReady=\(xpcServiceReady ? "✓" : "✗"), conn=\(hasConnection ? "✓" : "✗"), size=\(sizeValid ? "✓" : "✗")(\(Int(size.width))x\(Int(size.height)))")

        // 检查四个条件
        guard allocatedServiceId != nil else { return }
        guard hasConnection else { return }
        guard xpcServiceReady else { return }
        guard sizeValid else { return }

        // 防止重复发送
        guard size != lastSentContainerSize else {
            ipcLog("ℹ️ trySendInitialConfig tab=\(tabIdShort): already sent, skipping")
            return
        }

        ipcLog("🚀 trySendInitialConfig: ALL CONDITIONS MET for tab=\(tabIdShort), size=\(Int(size.width))x\(Int(size.height))")
        lastSentContainerSize = size
        sendContainerSizeNotification(size: size)
    }

    /// 节流：上次更新时间
    private var lastUpdateTime: Date = .distantPast
    /// 节流：待处理的尺寸更新
    private var pendingSize: CGSize?
    /// 节流：定时器
    private var throttleTimer: Timer?
    /// 节流间隔 - 限制发送频率（约 40fps）
    private static let throttleInterval: TimeInterval = 0.025  // 25ms

    // MARK: - 集中的尺寸更新入口

    /// Live resize 开始监听器
    private var liveResizeStartedObserver: NSObjectProtocol?

    /// 设置 live resize 监听器（只设置一次）
    private func setupLiveResizeObserverIfNeeded() {
        // 设置 resize 开始监听器
        if liveResizeStartedObserver == nil {
            liveResizeStartedObserver = NotificationCenter.default.addObserver(
                forName: .windowLiveResizeStarted,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                guard let self = self,
                      let hostView = self.hostView,
                      let tabId = self.tabId,
                      hostView.window != nil else { return }  // 只有可见的 hostView 才截取快照

                // 在 resize 开始时截取快照，存入 TabSnapshotCacheManager
                // 复用模式切换的快照存储（基于当前视口尺寸，通常是大视口 = Slot A）
                if let snapshot = hostView.captureSnapshot() {
                    TabSnapshotCacheManager.shared.setFocusSnapshot(snapshot, for: tabId)
                    WindowResizeState.shared.activeTabId = tabId
                }
            }
        }

        // 设置 resize 结束监听器
        // 不直接读取尺寸！SwiftUI layout 可能还没完成
        // 而是启动 debounce 机制，等待尺寸稳定后再发送
        if liveResizeEndedObserver == nil {
            liveResizeEndedObserver = NotificationCenter.default.addObserver(
                forName: .windowLiveResizeEnded,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                guard let self = self else { return }

                ipcLog("🔔 windowLiveResizeEnded received, tab=\(self.tabId?.uuidString.prefix(8) ?? "nil")")

                // 🔍 诊断：延迟 500ms 后打印完整的视图层级状态
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                    guard let self = self, let hostView = self.hostView else { return }
                    hostView.logViewHierarchy(prefix: "DIAG 500ms")
                }

                // 只在 resize 结束后进入等待状态，避免 live resize 期间卡在旧快照
                if let tabId = self.tabId {
                    WindowResizeState.shared.activeTabId = tabId
                    WindowResizeState.shared.isWaitingForSizeApplied = true
                }

                // 标记进入 debounce 状态
                self.isWaitingForFinalSize = true

                // 启动 NSView frame 变化监听器
                // 这可以捕获 SwiftUI GeometryReader 可能遗漏的 frame 更新
                self.setupFrameChangeObserverIfNeeded()

                // 启动 debounce 定时器
                self.scheduleFinalSizeDebounce()
            }
        }

        // 设置 sizeApplied 监听器（Service 尺寸更新完成后清除快照）
        setupSizeAppliedListenerIfNeeded()
    }

    /// sizeApplied 监听器（Service 完成尺寸更新后的通知）
    private var sizeAppliedObserver: NSObjectProtocol?

    /// hostView frame 变化监听器（捕获 SwiftUI GeometryReader 可能遗漏的变化）
    private var frameDidChangeObserver: NSObjectProtocol?

    /// 是否处于 resize 结束后的 debounce 期
    private var isWaitingForFinalSize = false
    /// 最终尺寸 debounce 定时器
    private var finalSizeDebounceTimer: Timer?
    /// Debounce 时间（等待尺寸稳定）
    private static let finalSizeDebounceInterval: TimeInterval = 0.10  // 100ms - 减少等待时间

    /// 上次发送尺寸的时间（用于检测是否需要补发）
    private var lastSizeNotificationTime: Date = .distantPast
    /// 补发窗口期（在此时间内收到新尺寸变化，需要补发）
    private static let resendWindowInterval: TimeInterval = 1.0  // 1 秒内的变化需要补发

    /// 设置 hostView frame 变化监听器
    /// 在 debounce 期间捕获任何 frame 变化，并重置 debounce 定时器
    private func setupFrameChangeObserverIfNeeded() {
        // 移除旧的监听器
        if let observer = frameDidChangeObserver {
            NotificationCenter.default.removeObserver(observer)
            frameDidChangeObserver = nil
        }

        guard let hostView = hostView else { return }

        // 启用 frame 变化通知
        hostView.postsFrameChangedNotifications = true

        frameDidChangeObserver = NotificationCenter.default.addObserver(
            forName: NSView.frameDidChangeNotification,
            object: hostView,
            queue: .main
        ) { [weak self] notification in
            guard let self = self,
                  self.isWaitingForFinalSize,
                  let view = notification.object as? NSView else { return }

            // 🔑 修复 @2x 问题：bounds 可能返回像素尺寸，需要转换为 point 尺寸
            let rawBounds = view.bounds.size
            let scaleFactor = view.window?.backingScaleFactor ?? 1.0

            // 检测是否是 @2x：如果与当前尺寸相差约 2 倍，则需要转换
            let newSize: CGSize
            if scaleFactor > 1.0 && rawBounds.width > self.currentViewportSize.width * 1.8 {
                // bounds 是像素尺寸，转换为 point 尺寸
                newSize = CGSize(width: rawBounds.width / scaleFactor, height: rawBounds.height / scaleFactor)
            } else {
                newSize = rawBounds
            }

            // 忽略无效尺寸
            guard newSize.width > 50 && newSize.height > 50 else { return }

            // Frame 变化了，重置 debounce 定时器
            self.currentViewportSize = newSize
            self.scheduleFinalSizeDebounce()
        }
    }

    /// 移除 frame 变化监听器
    private func removeFrameChangeObserver() {
        if let observer = frameDidChangeObserver {
            NotificationCenter.default.removeObserver(observer)
            frameDidChangeObserver = nil
        }
    }

    /// 设置 sizeApplied 监听器（事件驱动清除快照）
    private func setupSizeAppliedListenerIfNeeded() {
        guard sizeAppliedObserver == nil,
              let serviceId = allocatedServiceId else { return }

        let notificationName = "com.actro.sizeApplied.\(ActroHostProcessId).\(serviceId)"
        sizeAppliedObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(notificationName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self = self,
                  let userInfo = notification.userInfo,
                  let notifyTabId = userInfo[RuntimeBridgeContract.tabId] as? String,
                  notifyTabId == self.tabId?.uuidString else { return }

            // Extract requestId for log correlation
            let requestId = userInfo["requestId"] as? String ?? "?"
            let width = userInfo["width"] as? Double ?? 0
            let height = userInfo["height"] as? Double ?? 0
            NSLog("📐 [HOST←SVC] sizeApplied req=%@ size=%.0fx%.0f", requestId, width, height)

            // 取消超时定时器（如果有）
            self.snapshotTimeoutTimer?.invalidate()
            self.snapshotTimeoutTimer = nil

            // Service 已完成尺寸更新，关闭快照遮罩
            if WindowResizeState.shared.isWaitingForSizeApplied {
                WindowResizeState.shared.isWaitingForSizeApplied = false
            }
        }
    }

    /// 快照清除超时定时器（备用方案）
    private var snapshotTimeoutTimer: Timer?

    /// 启动快照超时定时器（备用方案，防止 Service 通知丢失）
    private func scheduleSnapshotTimeout() {
        snapshotTimeoutTimer?.invalidate()
        snapshotTimeoutTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: false) { _ in
            if WindowResizeState.shared.isWaitingForSizeApplied {
                WindowResizeState.shared.isWaitingForSizeApplied = false
            }
        }
    }

    /// 启动最终尺寸 debounce 定时器
    /// 在 debounce 期间如果有新尺寸变化，定时器会被重置
    /// 当定时器触发时（尺寸稳定），发送最终尺寸到 Service
    private func scheduleFinalSizeDebounce() {
        finalSizeDebounceTimer?.invalidate()
        ipcLog("⏱️ scheduleFinalSizeDebounce: starting \(Int(Self.finalSizeDebounceInterval * 1000))ms debounce timer")
        finalSizeDebounceTimer = Timer.scheduledTimer(withTimeInterval: Self.finalSizeDebounceInterval, repeats: false) { [weak self] _ in
            guard let self = self else { return }

            self.isWaitingForFinalSize = false
            ipcLog("⏱️ scheduleFinalSizeDebounce: timer fired")

            // 🔑 关键修复：使用 hostView 的实际 frame 作为最终尺寸
            // 之前使用 currentViewportSize，但它可能与 hostView.frame 不一致
            // （SwiftUI 可能在最后修改了 hostView.frame 但没有更新 currentViewportSize）
            let hostViewSize = self.hostView?.frame.size ?? self.currentViewportSize
            let finalSize = CGSize(
                width: floor(hostViewSize.width),
                height: floor(hostViewSize.height)
            )

            ipcLog("⏱️ scheduleFinalSizeDebounce: hostViewSize=\(Int(hostViewSize.width))x\(Int(hostViewSize.height)), currentViewportSize=\(Int(self.currentViewportSize.width))x\(Int(self.currentViewportSize.height)), finalSize=\(Int(finalSize.width))x\(Int(finalSize.height)), hasServiceId=\(self.allocatedServiceId != nil), hasConn=\(self.hasConnection), xpcReady=\(self.xpcServiceReady)")

            // 验证并发送
            if finalSize.width > 50 && finalSize.height > 50 {
                // 同步更新 coordinator 状态
                self.currentViewportSize = finalSize

                // 同时更新本地 frame
                self.hostView?.applyFrameSize(finalSize)

                // 🔑 检查四个条件都满足才能发送通知
                guard self.allocatedServiceId != nil,
                      self.hasConnection,
                      self.xpcServiceReady else {
                    ipcLog("⚠️ scheduleFinalSizeDebounce: conditions not met, skipping notification")
                    return
                }

                self.lastSentContainerSize = finalSize
                self.lastSizeNotificationTime = Date()
                ipcLog("📤 scheduleFinalSizeDebounce: sending containerSizeNotification size=\(Int(finalSize.width))x\(Int(finalSize.height))")
                self.sendContainerSizeNotification(size: finalSize)

                // 🔑 强制 remoteView 刷新显示
                // NSRemoteView 有时不会自动刷新其内容
                self.hostView?.forceRemoteViewRefresh()
            }

            // 移除 frame 变化监听器（debounce 结束）
            self.removeFrameChangeObserver()

            // 启动 sizeApplied 超时定时器
            self.scheduleSnapshotTimeout()
        }
    }

    /// 统一的尺寸更新方法 - 所有尺寸变化都应该通过这个方法
    /// 使用节流机制限制发送频率，大变化立即应用
    /// 必须在主线程调用
    func updateViewportSize(_ newSize: CGSize) {
        // 确保监听器已设置
        setupLiveResizeObserverIfNeeded()
        // 确保在主线程执行
        guard Thread.isMainThread else {
            DispatchQueue.main.async { [weak self] in
                self?.updateViewportSize(newSize)
            }
            return
        }

        // 🔑 使用 floor() 取整，与 WorkspaceMainViewport/WorkspaceView 保持一致
        // 避免 round() vs floor() 差异导致 1px 抖动
        let roundedSize = CGSize(
            width: floor(newSize.width),
            height: floor(newSize.height)
        )

        // 验证尺寸合法性
        guard roundedSize.width >= 50 && roundedSize.height >= 50 &&
              roundedSize.width <= 4096 && roundedSize.height <= 4096 else {
            return
        }

        // 忽略相同尺寸（取整后比较）
        guard roundedSize != currentViewportSize else {
            // 如果处于 debounce 状态但尺寸没变，仍需重置 debounce 定时器
            // 这处理了 SwiftUI 可能多次发送相同尺寸的情况
            if isWaitingForFinalSize {
                scheduleFinalSizeDebounce()
            }
            return
        }

        // 更新本地状态
        let previousSize = currentViewportSize
        currentViewportSize = roundedSize

        // 🔑 关键修改：始终立即更新 hostView frame + 发送通知到 Service
        // 之前的 debounce/节流逻辑导致 Host 和 Service 尺寸不同步
        // 简化为：立即应用所有尺寸变化
        applyViewportSize(roundedSize)
    }

    /// 调度节流后的更新（必须在主线程调用）
    private func scheduleThrottledUpdate() {
        guard throttleTimer == nil else { return }

        // 确保 Timer 在主线程的 RunLoop 上调度
        throttleTimer = Timer.scheduledTimer(withTimeInterval: Self.throttleInterval, repeats: false) { [weak self] _ in
            DispatchQueue.main.async {
                guard let self = self, let pendingSize = self.pendingSize else { return }
                self.throttleTimer = nil
                self.pendingSize = nil
                self.applyViewportSize(pendingSize)
            }
        }
    }

    /// 实际应用尺寸更新（必须在主线程调用）
    private func applyViewportSize(_ size: CGSize) {
        lastUpdateTime = Date()

        // 1. 更新 hostView 的本地 frame（始终执行）
        hostView?.applyFrameSize(size)
        rendererHostView?.apply(size: size, isOverviewMode: isOverviewMode)

        // 2. 🔑 检查四个条件都满足才能发送通知
        //    否则由 didSet 触发的 trySendInitialConfig 会处理
        guard allocatedServiceId != nil else { return }
        guard hasConnection else { return }
        guard xpcServiceReady else { return }

        // 3. 发送 containerSizeChanged 通知到 Service（防重复）
        //    🔑 resize 期间也发送，让 RN 层动态响应尺寸变化
        guard size != lastSentContainerSize else { return }
        lastSentContainerSize = size
        sendContainerSizeNotification(size: size)
    }

    func captureSnapshot() -> NSImage? {
        rendererHostView?.captureSnapshot() ?? hostView?.captureSnapshot()
    }

    /// Send a containerSizeChanged notification to the XPC service
    /// 内部方法 - 由 applyViewportSize 调用
    /// 🔑 直接发送 DistributedNotification（不走 IPCBridgeAdapter，因为 tabStates 未注册）
    private func sendContainerSizeNotification(size: CGSize) {
        guard let tabId = tabId,
              let serviceId = allocatedServiceId else {
            NSLog("⚠️ [Coordinator] Cannot send containerSizeChanged - no tabId or serviceId")
            return
        }

        if rendererHostView != nil {
            RendererProcessPool.shared.updateViewport(
                tabId: tabId,
                size: size,
                isOverviewMode: isOverviewMode
            )
            return
        }

        // Generate request ID for log correlation
        let requestId = Int.random(in: 10000...99999)

        // 构建 sizeInfo 数据
        let sizeInfo: [String: Any] = [
            RuntimeBridgeContract.tabId: tabId.uuidString,
            "width": size.width,
            "height": size.height,
            "requestId": requestId,
            "isOverviewMode": isOverviewMode
        ]

        guard let jsonData = try? JSONSerialization.data(withJSONObject: sizeInfo),
              let jsonString = String(data: jsonData, encoding: .utf8) else {
            NSLog("⚠️ [Coordinator] Failed to serialize sizeInfo")
            return
        }

        // 🔑 直接发送 DistributedNotification，格式与 Service 端监听匹配
        let notificationName = "com.actro.containerSizeChanged.\(ActroHostProcessId).\(serviceId)"

        ipcLog("📐 [HOST→SVC] containerSizeChanged req=\(requestId) tab=\(String(tabId.uuidString.prefix(8))) size=\(Int(size.width))x\(Int(size.height)) isOverviewMode=\(isOverviewMode) name=\(notificationName)")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: ["sizeInfo": jsonString],
            deliverImmediately: true
        )
    }

    /// Send left panel visibility change to XPC service via DistributedNotification
    /// Called when user toggles the left panel via address bar menu icon
    func sendLeftPanelVisibilityChanged(_ visible: Bool) {
        guard let tabId = tabId,
              let serviceId = allocatedServiceId else {
            ipcLog("⚠️ sendLeftPanelVisibilityChanged: missing tabId or allocatedServiceId")
            return
        }

        if let rendererHostView, let tabConfig {
            var config = tabConfig.toDictionary(isOverviewMode: isOverviewMode)
            config["leftPanelVisible"] = visible
            rendererHostView.apply(config: config)
            return
        }

        let payload: [String: Any] = [
            RuntimeBridgeContract.tabId: tabId.uuidString,
            "isLeftPanelVisible": visible
        ]

        guard let jsonData = try? JSONSerialization.data(withJSONObject: payload),
              let jsonString = String(data: jsonData, encoding: .utf8) else {
            ipcLog("⚠️ sendLeftPanelVisibilityChanged: failed to serialize payload")
            return
        }

        let notificationName = "com.actro.uiStateChanged.\(ActroHostProcessId).\(serviceId)"

        ipcLog("📤 [HOST→SVC] leftPanelVisibility tab=\(String(tabId.uuidString.prefix(8))) visible=\(visible) name=\(notificationName)")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: ["payload": jsonString],
            deliverImmediately: true
        )
    }

    /// 清理 XPC 资源
    func cleanup() {
        // 清理节流定时器
        throttleTimer?.invalidate()
        throttleTimer = nil
        pendingSize = nil

        // 清理 final size debounce 定时器
        finalSizeDebounceTimer?.invalidate()
        finalSizeDebounceTimer = nil
        isWaitingForFinalSize = false

        // 清理 frame 变化监听器
        removeFrameChangeObserver()

        if let observer = xpcReadyObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
            xpcReadyObserver = nil
        }
        if let observer = connectionCreatedObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
            connectionCreatedObserver = nil
        }
        if let observer = listenerReadyObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
            listenerReadyObserver = nil
        }
        if let observer = liveResizeStartedObserver {
            NotificationCenter.default.removeObserver(observer)
            liveResizeStartedObserver = nil
        }
        if let observer = liveResizeEndedObserver {
            NotificationCenter.default.removeObserver(observer)
            liveResizeEndedObserver = nil
        }
        if let observer = sizeAppliedObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
            sizeAppliedObserver = nil
        }
        snapshotTimeoutTimer?.invalidate()
        snapshotTimeoutTimer = nil
        // 断开 ViewBridge 连接
        hostView?.disconnect()
        hostView = nil
        rendererHostView?.prepareForReuse()
        rendererHostView = nil

        if let tabId = tabId, allocatedServiceId != nil {
            RendererProcessPool.shared.releaseRenderer(for: tabId)
            ipcLog("📦 Coordinator cleanup: Tab \(String(tabId.uuidString.prefix(8))) released renderer")
        }
        allocatedServiceId = nil
        xpcServiceReady = false
        hasConnection = false
    }

    deinit {
        cleanup()
    }
}

/// Manager that holds coordinators for all tabs
/// 集中管理 isOverviewMode 状态，订阅 BrowserViewModel.actroMode 的变化
/// 所有组件应该从这里获取/更新 isOverviewMode，而不是分散管理
final class ViewBridgeCoordinatorManager: ObservableObject {
    // 不使用 @Published - coordinators 的变化不需要触发 UI 更新
    private var coordinators: [UUID: ViewBridgeCoordinator] = [:]

    /// Global overview mode state - used when creating new coordinators
    /// 由 actroMode 订阅自动更新，外部不应直接修改
    /// 默认 false 因为 BrowserViewModel.init 中设置 actroMode = .focus
    private(set) var currentOverviewMode: Bool = false

    /// Combine subscription for actroMode changes
    private var actroModeSubscription: AnyCancellable?

    /// Singleton for global access
    static let shared = ViewBridgeCoordinatorManager()

    /// Write debug message to IPC log file
    private func writeToIPCLog(_ message: String) {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss.SSS"
        let timestamp = formatter.string(from: Date())
        let line = "[\(timestamp)] [HOST] \(message)\n"
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

    /// 绑定到 BrowserViewModel，订阅 actroMode 变化
    /// 应该在 app 启动时调用一次
    func bindToViewModel(_ viewModel: any ActroModePublisher) {
        // 取消之前的订阅（如果有）
        actroModeSubscription?.cancel()

        // 立即同步当前状态（Focus 模式 → isOverviewMode = false）
        // 注意：必须调用 updateOverviewMode 来更新已存在的 coordinators
        let isOverview = !viewModel.actroMode.isInFocusMode
        writeToIPCLog("📐 [ViewBridgeCoordinatorManager] bindToViewModel: isInFocusMode=\(viewModel.actroMode.isInFocusMode), isOverview=\(isOverview)")
        NSLog("📐 [ViewBridgeCoordinatorManager] bindToViewModel: actroMode.isInFocusMode=%@, isOverview=%@, currentOverviewMode=%@",
              viewModel.actroMode.isInFocusMode ? "true" : "false",
              isOverview ? "true" : "false",
              currentOverviewMode ? "true" : "false")

        // 强制更新（即使值相同），确保已存在的 coordinators 也被更新
        forceUpdateOverviewMode(isOverview)

        // 订阅 actroMode 变化
        actroModeSubscription = viewModel.actroModePublisher
            .dropFirst() // 跳过初始值（已经在上面同步了）
            .sink { [weak self] (newMode: ActroMode) in
                let isOverview = !newMode.isInFocusMode
                self?.writeToIPCLog("📐 [actroModeSubscription] newMode=\(newMode), isOverview=\(isOverview)")
                self?.updateOverviewMode(isOverview)
            }

        writeToIPCLog("📐 [ViewBridgeCoordinatorManager] Bound to ViewModel, isOverviewMode=\(isOverview)")
        NSLog("📐 [ViewBridgeCoordinatorManager] Bound to ViewModel, isOverviewMode=%@",
              isOverview ? "true" : "false")
    }

    func coordinator(for tabId: UUID) -> ViewBridgeCoordinator {
        if let existing = coordinators[tabId] {
            return existing
        }
        let newCoordinator = ViewBridgeCoordinator()
        // Set isOverviewMode based on current global state
        newCoordinator.isOverviewMode = currentOverviewMode
        coordinators[tabId] = newCoordinator
        return newCoordinator
    }

    func removeCoordinator(for tabId: UUID) {
        coordinators.removeValue(forKey: tabId)
    }

    /// 内部方法：更新所有 coordinators 的 isOverviewMode 并通知 services
    /// 由 actroMode 订阅自动调用，外部不应直接调用
    private func updateOverviewMode(_ isOverview: Bool) {
        // 避免重复更新
        guard currentOverviewMode != isOverview else { return }
        applyOverviewMode(isOverview)
    }

    /// 强制更新所有 coordinators 的 isOverviewMode（不检查当前值）
    /// 用于初始绑定时确保已存在的 coordinators 也被更新
    private func forceUpdateOverviewMode(_ isOverview: Bool) {
        applyOverviewMode(isOverview)
    }

    /// 实际执行 isOverviewMode 更新的核心逻辑
    /// 🔑 注意：此方法只更新 isOverviewMode 标志和重置 lastSentContainerSize
    ///    不在此处发送 containerSizeChanged 通知，因为此时 coordinator.currentViewportSize
    ///    仍然是旧的 Focus 模式尺寸。正确的尺寸通知会在 SwiftUI 重新渲染时通过
    ///    onChange(of: currentSize) → updateViewportSize → sendContainerSizeNotification 发送
    private func applyOverviewMode(_ isOverview: Bool) {
        // Update global state for newly created coordinators
        currentOverviewMode = isOverview
        // Write to IPC log file for debugging
        writeToIPCLog("📐 [applyOverviewMode] isOverview=\(isOverview) coordinatorCount=\(coordinators.count)")
        NSLog("📐 [applyOverviewMode] isOverview=%@ coordinatorCount=%d", isOverview ? "true" : "false", coordinators.count)

        for (tabId, coordinator) in coordinators {
            writeToIPCLog("📐 [applyOverviewMode] tab=\(tabId.uuidString.prefix(8)) hasConnection=\(coordinator.hasConnection) serviceId=\(coordinator.allocatedServiceId ?? "nil")")
            NSLog("📐 [applyOverviewMode] tab=%@ hasConnection=%@ serviceId=%@",
                  String(tabId.uuidString.prefix(8)),
                  coordinator.hasConnection ? "true" : "false",
                  coordinator.allocatedServiceId ?? "nil")
            coordinator.isOverviewMode = isOverview
            // Reset lastSentContainerSize to force re-send notification when SwiftUI updates the view
            // with the correct cardSize. This ensures the next updateViewportSize() call sends the notification.
            coordinator.resetLastSentContainerSize()
            // 🔑 不在此处发送通知！coordinator.currentViewportSize 是旧的 Focus 模式尺寸
            // SwiftUI 会在重新渲染时通过 onChange(of: currentSize) 发送正确的 cardSize
        }
    }
}

// MARK: - ViewBridge Host View Manager

/// 全局管理 ViewBridgeHostView 实例，避免视图重建时丢失 XPC 连接
/// 类似 WebViewHolderManager 的模式
@MainActor
final class ViewBridgeHostViewManager {
    static let shared = ViewBridgeHostViewManager()
    private var hostViews: [UUID: ViewBridgeHostView] = [:]
    /// Cached snapshots for Overview mode
    private var snapshots: [UUID: NSImage] = [:]

    private init() {}

    /// Get or create a ViewBridgeHostView for a tab
    func hostView(for tabId: UUID, serviceBundleIdentifier: String, coordinator: ViewBridgeCoordinator, contentSize: CGSize, allowsHitTesting: Bool = true) -> ViewBridgeHostView {
        if let existing = hostViews[tabId] {
            // Update coordinator reference
            existing.coordinator = coordinator
            coordinator.hostView = existing
            // Update hit testing state (Overview mode disables hit testing)
            existing.allowsHitTesting = allowsHitTesting
            // 通过 coordinator 统一处理尺寸更新（包括通知 Service）
            coordinator.updateViewportSize(contentSize)
            return existing
        }

        let new = ViewBridgeHostView(serviceBundleIdentifier: serviceBundleIdentifier, initialContentSize: contentSize)
        new.coordinator = coordinator
        new.allowsHitTesting = allowsHitTesting
        coordinator.hostView = new
        hostViews[tabId] = new
        return new
    }

    /// Get existing host view without creating a new one
    func existingHostView(for tabId: UUID) -> ViewBridgeHostView? {
        hostViews[tabId]
    }

    func remove(tabId: UUID) {
        if let removed = hostViews.removeValue(forKey: tabId) {
            removed.disconnect()
        }
        snapshots.removeValue(forKey: tabId)
    }

    // MARK: - Snapshot Management

    /// Take a snapshot of the ViewBridge content for Overview mode
    /// Note: TabSnapshotCacheManager is now managed by WorkspaceMainView.exitToOverview()
    func takeSnapshot(for tabId: UUID) {
        guard let hostView = hostViews[tabId] else { return }
        if let snapshot = hostView.captureSnapshot() {
            // Legacy: store in local cache only
            snapshots[tabId] = snapshot
        }
    }

    /// Get cached snapshot for Overview mode
    /// Returns from TabSnapshotCacheManager (V3.0) if available, falls back to local cache
    func snapshot(for tabId: UUID) -> NSImage? {
        // V3.0: prefer filtered thumbnail from TabSnapshotCacheManager
        if let filtered = TabSnapshotCacheManager.shared.filteredThumbnail(for: tabId) {
            return filtered
        }
        // Fallback to local cache (legacy)
        return snapshots[tabId]
    }
}

// MARK: - Container View

/// 容器视图（使用 flipped 坐标系以匹配 SwiftUI）
/// SwiftUI 使用原点在左上角的坐标系，NSView 需要匹配
private class ContainerView: NSView {
    override var isFlipped: Bool { true }

    /// 是否允许 hit testing（由 SwiftUI 通过 userInteractionEnabled 控制）
    /// Overview 模式下设为 false，让手势穿透到下层的 Grid
    var allowsHitTesting: Bool = true {
        didSet {
            // 🔑 在 Overview 模式下禁用拖拽目标，让拖拽事件穿透到下层 Grid
            if !allowsHitTesting {
                unregisterDraggedTypes()
            }
        }
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.masksToBounds = true
        layer?.actions = ["bounds": NSNull(), "position": NSNull(), "frame": NSNull()]
        // 让子视图随容器 resize
        autoresizesSubviews = true
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)

        // 禁用隐式动画，确保一步到位
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // 🔑 强制同步所有子视图的 frame（autoresizingMask 不可靠）
        for subview in subviews {
            if subview.frame.size != newSize {
                subview.frame = NSRect(origin: .zero, size: newSize)
            }
        }

        CATransaction.commit()
    }

    /// 重写 hitTest 以支持条件性手势穿透
    /// Overview 模式下返回 nil，让事件传递到下层视图
    override func hitTest(_ point: NSPoint) -> NSView? {
        if !allowsHitTesting {
            return nil  // 穿透到下层
        }
        return super.hitTest(point)
    }
}

// MARK: - Persistent ViewBridge View

/// 持久化的 ViewBridge 视图包装器
/// 使用 ViewBridgeHostViewManager 获取持久化的 hostView，避免 SwiftUI 重建时丢失 XPC 连接
/// 类似 PersistentWebView 的模式
struct PersistentViewBridgeView: NSViewRepresentable {
    let tabId: UUID
    let serviceBundleIdentifier: String
    let coordinator: ViewBridgeCoordinator
    let size: CGSize
    /// 是否允许 hit testing（Overview 模式下为 false，让手势穿透）
    var allowsHitTesting: Bool = true

    func makeNSView(context: Context) -> NSView {
        let container = ContainerView(frame: NSRect(origin: .zero, size: size))
        container.allowsHitTesting = allowsHitTesting
        coordinator.tabId = tabId

        // 从 Manager 获取持久化的 hostView
        let hostView = ViewBridgeHostViewManager.shared.hostView(
            for: tabId,
            serviceBundleIdentifier: serviceBundleIdentifier,
            coordinator: coordinator,
            contentSize: size,
            allowsHitTesting: allowsHitTesting
        )

        // 设置连接建立回调
        // 🔑 直接在这里设置 hasConnection，因为 onAppear 中的 setupConnectionCallback
        // 可能在连接建立后才执行（时序问题）
        // 🔑 同时调用 trySendInitialConfig()，确保即使 onConnectionEstablished 为 nil 也能发送 config
        hostView.onConnectionEstablished = { [weak coordinator] in
            guard let coordinator = coordinator else { return }
            NSLog("🔗 [PersistentView] Connection established for tab %@, setting hasConnection=true",
                  String(tabId.uuidString.prefix(8)))
            coordinator.hasConnection = true

            // 🔑 通知 IPCBridgeAdapter，同步状态到 IPCInteractionManager
            IPCBridgeAdapter.shared.markTabConnected(tabId)

            // 先尝试直接发送 config（解决 onConnectionEstablished 可能为 nil 的时序问题）
            coordinator.trySendInitialConfig()
            // 再调用可选的回调（UnifiedReactNativeLayer 可能设置了额外的处理）
            coordinator.onConnectionEstablished?()
        }

        // 如果 hostView 已有 superview（被重用），先移除
        let wasReused = hostView.superview != nil
        if wasReused {
            NSLog("🔄 [PersistentView] hostView reused for tab %@, old superview=%@",
                  String(tabId.uuidString.prefix(8)), String(describing: hostView.superview))
            hostView.removeFromSuperview()
        }

        // 使用 autoresizingMask - 强制从 (0,0) 开始
        // 🔑 使用 size 参数（point 尺寸），不用 container.bounds.size（可能是 @2x 像素尺寸）
        hostView.frame = NSRect(origin: .zero, size: size)
        hostView.autoresizingMask = [.width, .height]
        container.addSubview(hostView)

        NSLog("🔧 [PersistentView] makeNSView: tab=%@, containerSize=%@, hostView.frame=%@, hasRemote=%d",
              String(tabId.uuidString.prefix(8)), NSStringFromSize(size),
              NSStringFromRect(hostView.frame), hostView.hasRemoteViewController ? 1 : 0)

        return container
    }

    func updateNSView(_ container: NSView, context: Context) {
        // 确保 coordinator.tabId 已设置
        if coordinator.tabId == nil {
            coordinator.tabId = tabId
        }

        // 🔑 更新 hit testing 状态（Overview 模式下禁用，让手势穿透）
        if let containerView = container as? ContainerView {
            containerView.allowsHitTesting = allowsHitTesting
        }

        // 🔑 关键修复：使用传入的 size 参数，而不是 container.bounds.size
        // SwiftUI 使用 .position() 定位时，可能不会更新 container.bounds
        // 传入的 size 参数才是正确的目标尺寸（Overview 模式下是 cardSize）
        let targetSize = size

        // 🔑 修复 @2x 问题：container.bounds 在 Retina 显示器上可能返回像素尺寸
        // 需要检测并转换为 point 尺寸
        let rawBounds = container.bounds.size
        let scaleFactor = container.window?.backingScaleFactor ?? 1.0

        // 检测 bounds 是否是 @2x：如果 bounds 约等于 targetSize * scaleFactor，则需要转换
        let boundsLooksLikePixels = scaleFactor > 1.0 &&
            abs(rawBounds.width - targetSize.width * scaleFactor) < 2 &&
            abs(rawBounds.height - targetSize.height * scaleFactor) < 2

        let containerBounds: CGSize
        if boundsLooksLikePixels {
            // bounds 是像素尺寸，转换为 point 尺寸
            containerBounds = CGSize(width: rawBounds.width / scaleFactor, height: rawBounds.height / scaleFactor)
            ipcLog("⚠️ updateNSView: converted @\(Int(scaleFactor))x bounds: \(Int(rawBounds.width))x\(Int(rawBounds.height)) → \(Int(containerBounds.width))x\(Int(containerBounds.height))")
        } else {
            containerBounds = rawBounds
        }

        let isLiveResizing = WindowResizeState.shared.isLiveResizing

        // 调试日志
        ipcLog("📐 updateNSView: tab=\(String(tabId.uuidString.prefix(8))), targetSize=\(Int(targetSize.width))x\(Int(targetSize.height)), containerBounds=\(Int(containerBounds.width))x\(Int(containerBounds.height)), rawBounds=\(Int(rawBounds.width))x\(Int(rawBounds.height)), scale=\(scaleFactor), allowsHitTesting=\(allowsHitTesting)")

        guard targetSize.width > 10 && targetSize.height > 10 else {
            ipcLog("⚠️ updateNSView: skipping - targetSize too small")
            return
        }

        // 🔑 简化逻辑：始终优先使用 SwiftUI 传入的 targetSize
        // 之前尝试在 live resize 期间使用 bounds，但 bounds 可能返回 @2x 像素尺寸，造成问题
        // targetSize 是由 SwiftUI 正确计算的 point 尺寸，更可靠
        let effectiveSize = targetSize

        // 🔑 关键：无论 hostView 是否存在，都更新 coordinator 的尺寸
        // 这确保 currentViewportSize 始终是正确的 point 尺寸
        // 之前只在 hostView 存在时才更新，导致初始化时 currentViewportSize 可能被其他代码路径设置为 @2x
        coordinator.updateViewportSize(effectiveSize)

        // 🔑 关键改进：始终更新 hostView frame，包括 live resize 期间
        // 之前在 live resize 期间跳过更新，可能导致 resize 结束后
        // hostView.frame 与 SwiftUI 期望的尺寸不一致
        if let hostView = container.subviews.first as? ViewBridgeHostView {
            let hostSize = hostView.frame.size
            let sizeDiff = abs(hostSize.width - effectiveSize.width) + abs(hostSize.height - effectiveSize.height)
            if sizeDiff > 0.5 {
                // 使用 CATransaction 避免隐式动画
                CATransaction.begin()
                CATransaction.setDisableActions(true)
                hostView.frame = NSRect(origin: .zero, size: effectiveSize)
                CATransaction.commit()
            }
            // Update hit testing state on hostView as well
            hostView.allowsHitTesting = allowsHitTesting
        }
    }

    static func dismantleNSView(_ container: NSView, coordinator: ()) {
        // 不移除 hostView - 它由 Manager 持有
        // 下次 makeNSView 时会重用同一个 hostView
        ipcLog("⚠️ PersistentViewBridgeView.dismantleNSView called (hostView retained by manager)")
    }
}

// MARK: - Nine Grid Debug Overlay

/// 九宫格调试视图 - 用于可视化视口位置
struct NineGridDebugOverlay: View {
    let size: CGSize

    var body: some View {
        ZStack {
            // 网格线
            GeometryReader { proxy in
                let drawingSize = proxy.size
                let cellWidth = drawingSize.width / 3
                let cellHeight = drawingSize.height / 3

                ZStack {
                    Path { path in
                        for i in 1..<3 {
                            let x = cellWidth * CGFloat(i)
                            path.move(to: CGPoint(x: x, y: 0))
                            path.addLine(to: CGPoint(x: x, y: drawingSize.height))
                        }
                        for i in 1..<3 {
                            let y = cellHeight * CGFloat(i)
                            path.move(to: CGPoint(x: 0, y: y))
                            path.addLine(to: CGPoint(x: drawingSize.width, y: y))
                        }
                    }
                    .stroke(.red.opacity(0.5), lineWidth: 2)

                    Path(CGRect(x: 1, y: 1, width: drawingSize.width - 2, height: drawingSize.height - 2))
                        .stroke(.red, lineWidth: 2)

                    Path { path in
                        let centerX = drawingSize.width / 2
                        let centerY = drawingSize.height / 2
                        path.move(to: CGPoint(x: centerX - 20, y: centerY))
                        path.addLine(to: CGPoint(x: centerX + 20, y: centerY))
                        path.move(to: CGPoint(x: centerX, y: centerY - 20))
                        path.addLine(to: CGPoint(x: centerX, y: centerY + 20))
                    }
                    .stroke(.blue, lineWidth: 2)
                }
            }

            // 格子编号
            VStack(spacing: 0) {
                ForEach(0..<3) { row in
                    HStack(spacing: 0) {
                        ForEach(0..<3) { col in
                            let index = row * 3 + col + 1
                            Text("\(index)")
                                .font(.system(size: 48, weight: .bold))
                                .foregroundColor(.red.opacity(0.3))
                                .frame(maxWidth: .infinity, maxHeight: .infinity)
                        }
                    }
                }
            }

            // 尺寸信息
            VStack {
                HStack {
                    Spacer()
                    Text(String(format: "%.0f × %.0f", size.width, size.height))
                        .font(.system(size: 14, weight: .medium, design: .monospaced))
                        .foregroundColor(.white)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(Color.red.opacity(0.7))
                        .cornerRadius(4)
                        .padding(8)
                }
                Spacer()
            }
        }
        // 约束 frame 为 size，避免继承父视图的尺寸导致网格过大
        .frame(width: size.width, height: size.height)
        .allowsHitTesting(false) // 不阻止交互
    }
}

// MARK: - Unified React Native Layer

/// 统一的 ViewBridge 内容层 - 始终渲染，永不销毁
///
// MARK: - Conditional Position Modifier

/// 条件性应用 `.position()` 修饰符
/// - Focus 模式: 不应用 position，让视图自然填充父容器（避免坐标计算问题）
/// - Overview 模式: 应用 position 定位到卡片位置
private struct ConditionalPositionModifier: ViewModifier {
    let usePosition: Bool
    let x: CGFloat
    let y: CGFloat

    func body(content: Content) -> some View {
        if usePosition {
            // Overview/过渡模式：使用 position 绝对定位到卡片位置
            content.position(x: x, y: y)
        } else {
            // Focus 模式：填充父容器，对齐到左上角
            // 🔑 必须使用 frame(maxWidth/maxHeight) 让 NSViewRepresentable 获得正确的 bounds
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
    }
}

/// 🔑 核心设计：解决模式切换时 ViewBridge 连接断开的问题
/// - PersistentViewBridgeView 始终挂载，hostView 永不从容器中移除
/// - scale 始终为 1，不使用 scaleEffect
/// - 尺寸根据模式变化（Media Query 风格，Service 端渲染对应尺寸内容）
/// - Focus 模式：size=viewportSize, position=(0,0)
/// - Overview 模式：size=cardSize, position=cardPosition
/// - 动画切换期间：opacity=0，由 Animation 层快照遮盖
struct UnifiedReactNativeLayer: View {
    let tab: Tab
    let fingerprint: String
    let viewportSize: CGSize      // Focus 模式的尺寸（全屏）
    let cardSize: CGSize          // Overview 模式的尺寸（卡片大小）
    let cardPosition: CGPoint     // Overview 模式的位置（卡片左上角）
    let isFocused: Bool           // 当前 tab 是否处于 Focus 状态
    let isOverviewMode: Bool      // 当前是否处于 Overview 模式（用于边框显示）
    let isTransitioning: Bool     // 是否正在过渡动画中
    let isDragSource: Bool        // 是否为拖动源（蓝色虚线边框）
    let isDropTarget: Bool        // 是否为拖放目标（绿色实线边框）
    let coordinator: ViewBridgeCoordinator

    // System app data (for StartPageApp)
    var bookmarks: [[String: Any]] = []
    var recentHistory: [[String: Any]] = []

    init(
        tab: Tab,
        fingerprint: String,
        viewportSize: CGSize,
        cardSize: CGSize,
        cardPosition: CGPoint,
        isFocused: Bool,
        isOverviewMode: Bool,
        isTransitioning: Bool,
        isDragSource: Bool = false,
        isDropTarget: Bool = false,
        coordinator: ViewBridgeCoordinator,
        bookmarks: [[String: Any]] = [],
        recentHistory: [[String: Any]] = []
    ) {
        self.tab = tab
        self.fingerprint = fingerprint
        self.viewportSize = viewportSize
        self.cardSize = cardSize
        self.cardPosition = cardPosition
        self.isFocused = isFocused
        self.isOverviewMode = isOverviewMode
        self.isTransitioning = isTransitioning
        self.isDragSource = isDragSource
        self.isDropTarget = isDropTarget
        self.coordinator = coordinator
        self.bookmarks = bookmarks
        self.recentHistory = recentHistory
        // Note: coordinator setup moved to onAppear to avoid redundant side effects
        // during SwiftUI body re-evaluation (init called multiple times per tab)
    }

    /// 当前尺寸
    /// 🔑 过渡期间保持 Focus 尺寸，避免闪烁（因为动画层会遮盖）
    private var currentSize: CGSize {
        // 过渡期间：保持大尺寸，让动画层遮盖
        // 非过渡期间：根据 isFocused 选择尺寸
        if isTransitioning {
            return viewportSize  // 始终使用 Focus 尺寸
        }
        return isFocused ? viewportSize : cardSize
    }

    /// 当前位置（左上角）
    /// 🔑 过渡期间保持 Focus 位置，避免闪烁（因为动画层会遮盖）
    private var currentPosition: CGPoint {
        // 过渡期间：保持原点位置，让动画层遮盖
        // 非过渡期间：根据 isFocused 选择位置
        if isTransitioning {
            return .zero  // 始终使用 Focus 位置
        }
        return isFocused ? .zero : cardPosition
    }

    // XPC 状态
    @State private var isLoading = true
    @State private var errorMessage: String?
    @State private var hasValidConfigSent = false
    /// 🔑 触发 SwiftUI 重渲染的状态（coordinator 变化不会自动触发）
    @State private var serviceAllocated = false

    // 监听 resize 状态，用于显示快照遮罩
    @ObservedObject private var resizeState = WindowResizeState.shared

    // 调试模式开关
    private static let showDebugGrid = false

    /// 检查 service 是否已分配（使用 @State 触发 SwiftUI 重渲染）
    private var isServiceAllocated: Bool {
        serviceAllocated && coordinator.allocatedServiceId != nil
    }

    var body: some View {
        // 🔍 调试日志
        let _ = NSLog("🔍 [UnifiedRN] body: tab=%@, isFocused=%d, transitioning=%d, size=%.0fx%.0f, pos=(%.0f,%.0f)",
                      String(tab.id.uuidString.prefix(8)),
                      isFocused ? 1 : 0,
                      isTransitioning ? 1 : 0,
                      currentSize.width, currentSize.height,
                      currentPosition.x, currentPosition.y)

        ZStack(alignment: .topLeading) {
            // Renderer surface stays mounted while Focus/Overview only changes
            // its viewport and placement.
            if isServiceAllocated, coordinator.allocatedServiceId != nil {
                PersistentRendererProcessView(
                    tabId: tab.id,
                    fingerprint: fingerprint,
                    coordinator: coordinator,
                    size: currentSize,
                    allowsHitTesting: isFocused && !isTransitioning,
                    config: currentRendererConfig
                )
                .frame(width: currentSize.width, height: currentSize.height, alignment: .topLeading)
                .onAppear {
                    isLoading = false
                    coordinator.hasConnection = true
                }
            } else if let error = errorMessage {
                Text(error)
                    .foregroundColor(.red)
                    .frame(width: currentSize.width, height: currentSize.height)
            } else {
                ProgressView("Allocating process...")
                    .controlSize(.regular)
                    .frame(width: currentSize.width, height: currentSize.height)
            }

            // 九宫格调试 overlay（仅 Focus 模式且非过渡期间）
            if Self.showDebugGrid && isFocused && !isTransitioning {
                NineGridDebugOverlay(size: currentSize)
            }

            // 🔑 resize 期间不再需要遮罩，因为尺寸变化会实时传递给 RN 层
            // 只保留 sizeApplied 等待期的遮罩（用于 resize 结束后的短暂过渡）
            if isFocused && !isTransitioning,
               resizeState.isWaitingForSizeApplied,
               resizeState.activeTabId == tab.id,
               let snapshot = TabSnapshotCacheManager.shared.focusSnapshot(for: tab.id) {
                Image(nsImage: snapshot)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fill)
                    .frame(width: currentSize.width, height: currentSize.height)
                    .clipped()
                    .allowsHitTesting(false)
            }

            // 🔑 Overview 模式下的卡片边框（放在 ZStack 内部确保尺寸正确）
            // 使用 CardStyle.cornerRadius 与 WebTab 卡片对齐
            // 边框样式与 SlotView 一致：拖动源蓝色虚线，拖放目标绿色实线，默认白色细线
            if isOverviewMode && !isTransitioning {
                if isDragSource {
                    // 拖动源：蓝色虚线（与 SlotView 一致）
                    RoundedRectangle(cornerRadius: CardStyle.cornerRadius)
                        .stroke(
                            Color.blue.opacity(0.6),
                            style: StrokeStyle(lineWidth: 2, dash: [5, 5])
                        )
                        .allowsHitTesting(false)
                } else if isDropTarget {
                    // 拖放目标：绿色实线（与 SlotView 一致）
                    RoundedRectangle(cornerRadius: CardStyle.cornerRadius)
                        .stroke(Color.green.opacity(0.7), lineWidth: 2)
                        .allowsHitTesting(false)
                } else {
                    // 默认：白色细线边框
                    RoundedRectangle(cornerRadius: CardStyle.cornerRadius)
                        .stroke(Color.white.opacity(0.15), lineWidth: 1)
                        .allowsHitTesting(false)
                }
            }
        }
        .frame(width: currentSize.width, height: currentSize.height, alignment: .topLeading)
        // 🔑 Overview 模式添加圆角和阴影，与 TabPanel/NewTabPanel 统一
        .cornerRadius(isFocused ? 0 : CardStyle.cornerRadius)
        .shadow(color: isFocused ? .clear : Color.black.opacity(0.3), radius: 5, x: 0, y: 2)
        // 🔑 定位策略：
        // - Focus 模式: 不使用 position，通过 frame(maxWidth/maxHeight) 自然填充父容器
        // - Overview/过渡模式: 使用 position 定位到卡片位置（position 使用中心点坐标）
        .modifier(ConditionalPositionModifier(
            usePosition: !isFocused || isTransitioning,
            x: currentPosition.x + currentSize.width / 2,
            y: currentPosition.y + currentSize.height / 2
        ))
        // 🔑 禁用 Live 层的所有隐式动画（由 Animation 层快照遮盖）
        .animation(nil, value: currentSize)
        .animation(nil, value: currentPosition)
        .animation(nil, value: isTransitioning)
        // 🔑 过渡期间完全透明，动画结束后才显示
        .opacity(isTransitioning ? 0 : 1)
        // 🔑 z-index 控制：
        // - Focus 模式: zIndex(100) 在最前，确保交互时在最前
        // - Overview 模式: zIndex(50) 在 actro 前面，内容可见
        //   通过 NSView 的 hitTest() 返回 nil 让拖拽手势穿透到 TabGridView
        .zIndex(isFocused ? 100 : 50)
        // Focus 模式下允许交互
        .allowsHitTesting(isFocused && !isTransitioning)
        .onAppear {
            // Coordinator setup (moved from init to avoid redundant calls during SwiftUI body re-evaluation)
            coordinator.tabId = tab.id
            coordinator.isOverviewMode = !isFocused
            coordinator.tabConfig = XPCTabConfig(
                tabId: tab.id,
                title: tab.title,
                backgroundColor: tab.backgroundColor,
                containerSize: isFocused ? viewportSize : cardSize,
                systemAppType: tab.systemTabType?.rawValue,
                unifiedScript: tab.unifiedScript,
                leftPanelScript: tab.leftPanelScript,
                leftPanelVisible: tab.leftPanelDefaultVisible,
                rightPanelVisible: tab.rightPanelDefaultVisible,
                bookmarks: bookmarks,
                recentHistory: recentHistory,
                reloadKey: tab.reloadNonce
            )
            coordinator.currentViewportSize = currentSize
            ipcLog("📱 UnifiedReactNativeLayer.onAppear: tab=\(String(tab.id.uuidString.prefix(8))), systemTabType=\(tab.systemTabType?.rawValue ?? "nil"), isFocused=\(isFocused)")
            setupConnectionCallback()
            allocateServiceIfNeeded()
            if coordinator.hasConnection && coordinator.xpcServiceReady && isCoordinatorViewportSizeValid {
                sendTabConfigIfReady()
            }
        }
        .onChange(of: tab.systemTabType) { _, newValue in
            // 🔑 系统 Tab 类型变化时（如从 startPage 切换到 history），重新发送 config
            guard coordinator.hasConnection && coordinator.xpcServiceReady else { return }
            NSLog("🔄 [UnifiedRN] systemTabType changed to %@, resending config",
                  newValue?.rawValue ?? "nil")
            // 🔑 重置 lastSentConfigSize 强制重新发送（因为尺寸可能没变但内容变了）
            coordinator.lastSentConfigSize = nil
            sendTabConfigIfReady()
        }
        .onChange(of: tab.unifiedScript) { oldValue, newValue in
            // 🔑 unifiedScript 变化时（如从 startPage 切换到 ACTR app），重新发送 config
            let oldPreview = oldValue.map { String($0.prefix(30)) + "..." } ?? "nil"
            let newPreview = newValue.map { String($0.prefix(30)) + "..." } ?? "nil"
            NSLog("🔄 [UnifiedRN] onChange(unifiedScript): old=\(oldPreview), new=\(newPreview), hasConn=\(coordinator.hasConnection), xpcReady=\(coordinator.xpcServiceReady)")
            guard coordinator.hasConnection && coordinator.xpcServiceReady else {
                NSLog("⚠️ [UnifiedRN] unifiedScript changed but connection not ready, skipping")
                return
            }
            NSLog("🔄 [UnifiedRN] unifiedScript changed, resending config")
            coordinator.lastSentConfigSize = nil
            sendTabConfigIfReady()
        }
        .onChange(of: tab.reloadNonce) { _, _ in
            // 🔑 Reload 触发：强制发送新的 config（包含 reloadKey）
            coordinator.tabConfig = XPCTabConfig(
                tabId: tab.id,
                title: tab.title,
                backgroundColor: tab.backgroundColor,
                containerSize: isFocused ? viewportSize : cardSize,
                systemAppType: tab.systemTabType?.rawValue,
                unifiedScript: tab.unifiedScript,
                leftPanelScript: tab.leftPanelScript,
                leftPanelVisible: tab.leftPanelDefaultVisible,
                rightPanelVisible: tab.rightPanelDefaultVisible,
                bookmarks: bookmarks,
                recentHistory: recentHistory,
                reloadKey: tab.reloadNonce
            )
            coordinator.lastSentConfigSize = nil
            coordinator.lastSentReloadKey = nil
            sendTabConfigIfReady()
        }
        // 尺寸变化由 PersistentRendererProcessView.updateNSView 统一处理
        // 不再需要 onChange 监听，避免多源竞争导致尺寸抖动
    }

    private var currentRendererConfig: [String: Any] {
        var config: [String: Any] = [
            RuntimeBridgeContract.tabId: tab.id.uuidString,
            RuntimeBridgeContract.title: tab.title,
            "backgroundColor": tab.backgroundColor,
            RuntimeBridgeContract.containerWidth: currentSize.width,
            RuntimeBridgeContract.containerHeight: currentSize.height,
            "isOverviewMode": !isFocused,
            "leftPanelVisible": tab.leftPanelDefaultVisible,
            "rightPanelVisible": tab.rightPanelDefaultVisible,
            "bookmarks": bookmarks,
            "recentHistory": recentHistory,
            "reloadKey": tab.reloadNonce
        ]
        if let systemAppType = tab.systemTabType?.rawValue {
            config[RuntimeBridgeContract.systemAppType] = systemAppType
        }
        if let unifiedScript = tab.unifiedScript {
            config["unifiedScript"] = unifiedScript
        }
        if let leftPanelScript = tab.leftPanelScript {
            config["leftPanelScript"] = leftPanelScript
        }
        return config
    }

    /// 检查 viewportSize 是否有效（不是初始的 100x100）- 用于直接访问 view 的 viewportSize
    private var isViewportSizeValid: Bool {
        viewportSize.width > 100 && viewportSize.height > 100
    }

    /// 检查 coordinator 中的 viewportSize 是否有效 - 用于闭包中访问最新值
    /// 注意：使用宽松的阈值 (> 10)，因为在 Overview 模式下 cardSize 可能很小
    /// 但仍需要发送 tabConfig 让 RN 能渲染内容
    private var isCoordinatorViewportSizeValid: Bool {
        coordinator.currentViewportSize.width > 10 && coordinator.currentViewportSize.height > 10
    }

    private func allocateServiceIfNeeded() {
        if coordinator.allocatedServiceId != nil {
            // 已分配，确保 @State 同步
            if !serviceAllocated {
                serviceAllocated = true
            }
            // 快速路径：检查预热 Renderer 是否已连接。
            checkPrewarmStatusAndSetReady()
            return
        }

        do {
            let serviceId = try RendererProcessPool.shared.allocateRenderer(
                for: tab.id,
                fingerprint: fingerprint
            )
            coordinator.allocatedServiceId = serviceId
            // 🔑 触发 SwiftUI 重渲染
            serviceAllocated = true
            checkPrewarmStatusAndSetReady()
        } catch let error as RendererProcessPoolError {
            errorMessage = error.localizedDescription
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// 🔥 快速路径：如果 Service 已预热，直接标记 ready 并尝试发送 config
    private func checkPrewarmStatusAndSetReady() {
        guard let serviceId = coordinator.allocatedServiceId else { return }

        if RendererProcessPool.shared.isRendererReady(identifier: serviceId) {
            if !coordinator.xpcServiceReady {
                coordinator.xpcServiceReady = true
                NSLog("🔥 [Prewarm] Renderer %@ already ready, fast path enabled",
                      serviceId.components(separatedBy: ".").last ?? "")
            }
            sendTabConfigIfReady()
        }
    }

    private func setupXPCReadyListener() {
        guard coordinator.xpcReadyObserver == nil,
              let serviceId = coordinator.allocatedServiceId else {
            return
        }

        // 🔥 快速路径已在 checkPrewarmStatusAndSetReady 中处理
        // 这里仅设置监听器作为备用（处理 Service 重启等情况）

        // 监听 com.actro.xpcServiceReady（无后缀），通过 userInfo.bundleId 和 hostProcessId 过滤
        coordinator.xpcReadyObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.actro.xpcServiceReady"),
            object: nil,
            queue: .main
        ) { [serviceId] notification in
            // 只处理来自目标 service 的通知
            guard let userInfo = notification.userInfo,
                  let bundleId = userInfo["bundleId"] as? String,
                  bundleId == serviceId else { return }
            // 过滤：只处理属于本进程的通知
            if let hostPid = userInfo["hostProcessId"] as? Int32,
               hostPid != ActroHostProcessId {
                return
            }

            // 🔑 通知 IPCBridgeAdapter，同步状态到 IPCInteractionManager
            IPCBridgeAdapter.shared.markServiceReady(serviceId)
            coordinator.xpcServiceReady = true
            sendTabConfigIfReady()
        }

        // 🔑 设置 listenerReady 监听器 - 解决竞态条件
        // Service 设置好带 PID 的监听器后会发送 listenerReady 通知
        // Host 收到后重新发送 containerSizeChanged
        setupListenerReadyListener(serviceId: serviceId)
    }

    /// 设置 Service "listener ready" 通知监听器
    /// 解决竞态条件：Service 可能在 Host 发送 containerSizeChanged 之后才设置好监听器
    private func setupListenerReadyListener(serviceId: String) {
        guard coordinator.listenerReadyObserver == nil else { return }

        let listenerReadyName = "com.actro.listenerReady.\(ActroHostProcessId).\(serviceId)"
        ipcLog("🎧 Setting up listenerReady listener: \(listenerReadyName)")

        coordinator.listenerReadyObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(listenerReadyName),
            object: nil,
            queue: .main
        ) { [weak coordinator] notification in
            guard let coordinator = coordinator else { return }
            guard let userInfo = notification.userInfo,
                  let notifyingServiceId = userInfo["serviceId"] as? String,
                  notifyingServiceId == serviceId else { return }

            let tabIdShort = coordinator.tabId?.uuidString.prefix(8) ?? "nil"
            ipcLog("📥 [HOST] Received listenerReady for tab=\(tabIdShort), re-sending containerSizeChanged")

            // 🔑 重置 lastSentContainerSize 强制重新发送
            coordinator.resetLastSentContainerSize()
            coordinator.trySendInitialConfig()
        }
    }

    private func setupConnectionCallback() {
        coordinator.onConnectionEstablished = {
            coordinator.hasConnection = true
            sendTabConfigIfReady()
        }

        // 检查连接是否已建立（通过 hasRemoteViewController 判断）
        if coordinator.hostView?.hasRemoteViewController == true && !coordinator.hasConnection {
            coordinator.onConnectionEstablished?()
        }

        // 🔑 如果 hasConnection 已经是 true（由 PersistentViewBridgeView.makeNSView 设置），
        // 且 xpcServiceReady，直接发送 tabConfig
        if coordinator.hasConnection && coordinator.xpcServiceReady {
            NSLog("🔗 [setupConnectionCallback] Connection already established, sending config")
            sendTabConfigIfReady()
        }
    }

    private func sendTabConfigIfReady() {
        // 使用 coordinator 中的 viewportSize，因为这个方法可能从闭包中被调用
        let currentSize = coordinator.currentViewportSize
        let tabIdShort = String(tab.id.uuidString.prefix(8))

        // 检查 coordinator 中的 viewportSize 是否有效（不是初始的 100x100）
        guard isCoordinatorViewportSizeValid else {
            NSLog("⚠️ [sendTabConfig] tab=%@ SKIP: viewportSize invalid (%.0fx%.0f)",
                  tabIdShort, currentSize.width, currentSize.height)
            return
        }

        guard coordinator.hasConnection,
              let serviceId = coordinator.allocatedServiceId else {
            NSLog("⚠️ [sendTabConfig] tab=%@ SKIP: hasConnection=%d, xpcReady=%d, serviceId=%@",
                  tabIdShort, coordinator.hasConnection ? 1 : 0,
                  coordinator.xpcServiceReady ? 1 : 0,
                  coordinator.allocatedServiceId ?? "nil")
            return
        }

        let reloadKey = tab.reloadNonce
        // 防止重复发送相同尺寸且 reloadKey 未变化的 config
        if coordinator.lastSentConfigSize == currentSize && coordinator.lastSentReloadKey == reloadKey {
            return
        }
        coordinator.lastSentConfigSize = currentSize
        coordinator.lastSentReloadKey = reloadKey

        // 发送 tab config - 使用 coordinator 中的最新尺寸
        var tabConfig: [String: Any] = [
            RuntimeBridgeContract.tabId: tab.id.uuidString,
            RuntimeBridgeContract.title: tab.title,
            "backgroundColor": tab.backgroundColor,
            RuntimeBridgeContract.containerWidth: currentSize.width,
            RuntimeBridgeContract.containerHeight: currentSize.height,
            // 🔑 从状态管理器读取最新状态，而不是 coordinator 本地缓存
            "isOverviewMode": ViewBridgeCoordinatorManager.shared.currentOverviewMode,
            "reloadKey": reloadKey
        ]

        // Add optional scripts
        if let unifiedScript = tab.unifiedScript {
            tabConfig["unifiedScript"] = unifiedScript
        }
        if let systemTabType = tab.systemTabType {
            tabConfig[RuntimeBridgeContract.systemAppType] = systemTabType.rawValue
        }

        // Add system app data (for StartPageApp)
        if !bookmarks.isEmpty {
            tabConfig["bookmarks"] = bookmarks
        }
        if !recentHistory.isEmpty {
            tabConfig["recentHistory"] = recentHistory
        }

        NSLog("📤 [sendTabConfig] tab=%@ service=%@ size=%.0fx%.0f systemAppType=%@ bookmarks=%d history=%d",
              tabIdShort, serviceId.components(separatedBy: ".").last ?? "",
              currentSize.width, currentSize.height,
              tab.systemTabType?.rawValue ?? "nil",
              bookmarks.count, recentHistory.count)

        coordinator.rendererHostView?.apply(config: tabConfig)
    }
}

/// SwiftUI wrapper for ViewBridgeHostView
/// Remote view size 跟随 TabPanel 实际尺寸
/// 支持从进程池分配独立的 XPC service
struct ViewBridgeTabView: NSViewRepresentable {
    let tabId: UUID
    let contentSize: CGSize  // TabPanel 实际尺寸
    let coordinator: ViewBridgeCoordinator
    let serviceBundleIdentifier: String  // 从进程池分配的 bundle id

    init(tabId: UUID, contentSize: CGSize, coordinator: ViewBridgeCoordinator, serviceBundleIdentifier: String) {
        self.tabId = tabId
        self.contentSize = contentSize
        self.coordinator = coordinator
        self.serviceBundleIdentifier = serviceBundleIdentifier
    }

    func makeNSView(context: Context) -> ViewBridgeHostView {
        NSLog("🆕 ViewBridgeTabView.makeNSView: tab=%@, service=%@, size=%.0fx%.0f",
              String(tabId.uuidString.prefix(8)),
              serviceBundleIdentifier.components(separatedBy: ".").last ?? "",
              contentSize.width, contentSize.height)
        let view = ViewBridgeHostView(serviceBundleIdentifier: serviceBundleIdentifier, initialContentSize: contentSize)
        // Set coordinator reference for lifecycle management
        view.coordinator = coordinator
        coordinator.hostView = view
        // 设置连接建立回调
        // 🔑 直接在这里设置 hasConnection，因为 onAppear 中的 setupConnectionCallback
        // 可能在连接建立后才执行（时序问题）
        view.onConnectionEstablished = { [weak coordinator, tabId] in
            guard let coordinator = coordinator else { return }
            NSLog("🔗 [TabView] Connection established, setting hasConnection=true")
            coordinator.hasConnection = true
            // 🔑 通知 IPCBridgeAdapter，同步状态到 IPCInteractionManager
            IPCBridgeAdapter.shared.markTabConnected(tabId)
            coordinator.onConnectionEstablished?()
        }
        return view
    }

    func updateNSView(_ nsView: ViewBridgeHostView, context: Context) {
        ipcLog("📐 ViewBridgeTabView.updateNSView: contentSize=\(Int(contentSize.width))x\(Int(contentSize.height))")
        // Ensure coordinator reference is set
        nsView.coordinator = coordinator
        coordinator.hostView = nsView
        // 统一由 coordinator 发送 containerSizeChanged（HostView 只做本地尺寸更新）
        coordinator.updateViewportSize(contentSize)
    }

    static func dismantleNSView(_ nsView: ViewBridgeHostView, coordinator: ()) {
        // 不在这里断开连接！SwiftUI 可能会因为尺寸变化频繁调用 dismantle
        // 连接生命周期由 ViewBridgeCoordinator.deinit 管理
        NSLog("⚠️ ViewBridgeTabView.dismantleNSView called (NOT disconnecting - managed by coordinator)")
        // Note: 旧设计会调用 nsView.disconnect()，但这会在尺寸变化时意外断开连接
    }
}

