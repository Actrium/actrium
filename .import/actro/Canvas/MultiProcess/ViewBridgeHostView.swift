//
//  ViewBridgeHostView.swift
//  Canvas
//
//  Host view that displays content from a ViewBridge XPC service.
//  Uses NSRemoteViewController to request and embed remote views.
//
//  纯 AppKit 实现：完全截断 SwiftUI 的布局干预
//  - 忽略外部 setFrameSize 调用
//  - 内部固定使用 contentSize
//  - 使用节流防止 resize storm
//

import Cocoa

// MARK: - ViewBridge Host View

/// A view that hosts content from a ViewBridge XPC service.
/// 纯 AppKit 实现，完全截断 SwiftUI 的布局干预。
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

    /// Current content size (viewport size) - 真正的视口渲染尺寸
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

    // MARK: - 纯 AppKit: 完全忽略外部 frame 设置

    public override func setFrameSize(_ newSize: NSSize) {
        // 完全忽略 SwiftUI 传来的尺寸，始终使用 contentSize
        // 这样可以截断 scaleEffect 对 NSRemoteView 的影响
        super.setFrameSize(currentContentSize)
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

        // 纯 AppKit: 直接设置 frame，不使用约束
        addSubview(remoteView)
        remoteView.frame = CGRect(origin: .zero, size: currentContentSize)

        // 通知 XPC Service 端使用正确的尺寸
        vc.preferredContentSize = currentContentSize

        NSLog("ViewBridgeHostView: Remote view embedded, size=\(currentContentSize)")
    }

    // MARK: - Content Size Management

    /// 节流间隔（毫秒）- 防止 resize storm
    private static let resizeThrottleInterval: TimeInterval = 0.1  // 100ms

    /// Update the content size (viewport size)
    /// 使用节流机制防止 resize storm
    public func updateContentSize(_ newSize: CGSize) {
        // Validate reasonable size
        guard newSize.width >= 100 && newSize.height >= 100 &&
              newSize.width <= 4096 && newSize.height <= 4096 else {
            return
        }

        // Ignore small changes (< 2px)
        let sizeDiff = abs(newSize.width - currentContentSize.width) + abs(newSize.height - currentContentSize.height)
        guard sizeDiff >= 2 else { return }

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

        NSLog("ViewBridgeHostView: Applying content size \(newSize)")
        currentContentSize = newSize

        // 强制更新自身 frame
        super.setFrameSize(newSize)

        // Update remote view if exists
        if let vc = remoteViewController {
            vc.view.frame = CGRect(origin: .zero, size: newSize)
            vc.preferredContentSize = newSize
        }
    }

    /// 纯 AppKit 模式：不使用 scaleFactor
    /// 这个方法保留但不做任何事情
    public func updateScaleFactor(_ scaleFactor: CGFloat) {
        // 纯 AppKit 模式：忽略 scaleFactor，不做任何 transform
        // ViewBridge 内容始终以 contentSize 渲染，不缩放
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
        guard let layer = layer else { return nil }

        let size = bounds.size
        guard size.width > 0 && size.height > 0 else { return nil }

        let image = NSImage(size: size)
        image.lockFocus()
        if let context = NSGraphicsContext.current?.cgContext {
            layer.render(in: context)
        }
        image.unlockFocus()

        NSLog("ViewBridgeHostView: Captured snapshot \(size)")
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

/// Coordinator that holds reference to ViewBridgeHostView
class ViewBridgeCoordinator {
    weak var hostView: ViewBridgeHostView?

    func captureSnapshot() -> NSImage? {
        return hostView?.captureSnapshot()
    }
}

/// SwiftUI wrapper for ViewBridgeHostView
/// 纯 AppKit 包装：不传递 frame 变化给 NSView
struct ViewBridgeTabView: NSViewRepresentable {
    let tabId: UUID
    let contentSize: CGSize  // Viewport size for rendering (首次设置后锁定)
    let coordinator: ViewBridgeCoordinator

    func makeNSView(context: Context) -> ViewBridgeHostView {
        NSLog("ViewBridgeTabView: Creating ViewBridgeHostView for tab \(tabId), size: \(contentSize)")
        let view = ViewBridgeHostView(initialContentSize: contentSize)
        coordinator.hostView = view
        return view
    }

    func updateNSView(_ nsView: ViewBridgeHostView, context: Context) {
        coordinator.hostView = nsView
        // 纯 AppKit 模式：首次设置后锁定，不再更新尺寸
        // 尺寸由 ViewBridgeHostView 内部管理，不受 SwiftUI 影响
        if contentSize.width > 0 && contentSize.height > 0 {
            nsView.updateContentSize(contentSize)
        }
    }

    static func dismantleNSView(_ nsView: ViewBridgeHostView, coordinator: ()) {
        NSLog("ViewBridgeTabView: Disconnecting ViewBridgeHostView")
        nsView.disconnect()
    }
}

// MARK: - ViewBridge Multi-Process Tab View

/// A view that displays tab content via ViewBridge XPC service
/// 纯 AppKit 模式：ViewBridge 以 containerSize 渲染，不受 scaleEffect 影响
struct ViewBridgeMultiProcessTabView: View {
    let tab: Tab
    let containerSize: CGSize  // 渲染尺寸（首次设置后锁定）
    let scaleFactor: CGFloat   // 保留参数但不使用

    @State private var isLoading = true
    @State private var errorMessage: String?
    @State private var coordinator = ViewBridgeCoordinator()

    var body: some View {
        ZStack {
            if ViewBridgeHostView.isAvailable {
                // 纯 AppKit: ViewBridge 以 containerSize 渲染
                // frame 固定为 containerSize，不随 scaleEffect 变化
                ViewBridgeTabView(
                    tabId: tab.id,
                    contentSize: containerSize,
                    coordinator: coordinator
                )
                // 关键：frame 固定为 containerSize，忽略外部的 scaleEffect
                .frame(width: containerSize.width, height: containerSize.height)
                .onAppear {
                    isLoading = false
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
