//
//  ViewBridgeHostView.swift
//  Canvas
//
//  Host view that displays content from a ViewBridge XPC service.
//  Uses NSRemoteViewController to request and embed remote views.
//

import Cocoa

// MARK: - ViewBridge Host View

/// A view that hosts content from a ViewBridge XPC service.
/// This replaces IOSurfaceHostView for the ViewBridge experiment.
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

    /// Content size constraints - stored for dynamic updates
    private var widthConstraint: NSLayoutConstraint?
    private var heightConstraint: NSLayoutConstraint?

    /// Current content size (viewport size in Focus mode)
    private var currentContentSize: CGSize = CGSize(width: 1280, height: 720)

    /// Current scale factor for CALayer transform (visual scaling without resize)
    private var currentScaleFactor: CGFloat = 1.0

    // MARK: - Initialization

    /// Initialize with service configuration
    /// - Parameters:
    ///   - bundleIdentifier: The XPC service bundle identifier (e.g., "com.example.CanvasViewService")
    ///   - viewControllerClassName: The class name in the service (e.g., "CanvasViewService.CanvasServiceViewController")
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
            viewControllerClassName: "CanvasServiceViewController"  // No module prefix - dynamically registered
        )
    }

    /// Convenience initializer with initial content size and scale factor
    public convenience init(initialContentSize: CGSize, scaleFactor: CGFloat = 1.0) {
        self.init()
        self.currentContentSize = initialContentSize
        self.currentScaleFactor = scaleFactor
        NSLog("ViewBridgeHostView: init with initialContentSize=%.1fx%.1f, scaleFactor=%.3f", initialContentSize.width, initialContentSize.height, scaleFactor)
        // Write to file for debugging - use app support directory
        let logMsg = "[ViewBridgeHostView] init with initialContentSize=\(initialContentSize), scaleFactor=\(scaleFactor)\n"
        if let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
            let logPath = appSupport.appendingPathComponent("Canvas/viewbridge_host.log")
            try? FileManager.default.createDirectory(at: appSupport.appendingPathComponent("Canvas"), withIntermediateDirectories: true)
            try? logMsg.data(using: .utf8)?.write(to: logPath)
        }
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

        // 禁用自动调整大小，防止 scaleEffect 影响 frame
        translatesAutoresizingMaskIntoConstraints = true
        autoresizingMask = []

        // Set anchor point to top-left for proper scaling
        layer?.anchorPoint = CGPoint(x: 0, y: 0)

        // Apply initial scale factor if not 1.0
        if currentScaleFactor != 1.0 {
            layer?.transform = CATransform3DMakeScale(currentScaleFactor, currentScaleFactor, 1.0)
        }

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

    // MARK: - Frame Change Tracking

    private var lastFrameSize: CGSize = .zero
    private var hasLoggedCallStack = false

    public override func setFrameSize(_ newSize: NSSize) {
        // 只接受与 currentContentSize 匹配的尺寸，忽略其他尺寸变化
        // 这样可以防止 scaleEffect 动画导致的 resize
        let targetSize = currentContentSize
        let sizeDiff = abs(newSize.width - targetSize.width) + abs(newSize.height - targetSize.height)

        // 写日志到文件
        let logMsg = "[setFrameSize] newSize=\(newSize), targetSize=\(targetSize), sizeDiff=\(sizeDiff)\n"
        if let data = logMsg.data(using: .utf8),
           let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
            let logPath = appSupport.appendingPathComponent("Canvas/viewbridge_host.log").path
            if let handle = FileHandle(forWritingAtPath: logPath) {
                handle.seekToEndOfFile()
                handle.write(data)
                handle.closeFile()
            }
        }

        if sizeDiff > 2 {
            // 强制使用目标尺寸
            super.setFrameSize(targetSize)
            return
        }

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

        // 🔒 锁定 NSRemoteView 的尺寸，防止 scaleEffect 动画导致的 resize storms
        // NSRemoteView 默认 translatesAutoresizingMaskIntoConstraints = YES
        // 我们需要关闭它并添加固定尺寸约束
        remoteView.translatesAutoresizingMaskIntoConstraints = false

        // 移除 NSRemoteView 的现有约束（可能包含默认的 1280 宽度约束）
        for constraint in remoteView.constraints {
            if constraint.firstAttribute == .width || constraint.firstAttribute == .height {
                NSLog("ViewBridgeHostView: Removing existing constraint: \(constraint)")
                remoteView.removeConstraint(constraint)
            }
        }

        addSubview(remoteView)

        // 直接设置 frame（绕过约束系统）
        remoteView.frame = CGRect(origin: .zero, size: currentContentSize)
        NSLog("ViewBridgeHostView: Set remoteView.frame to \(remoteView.frame)")

        // 使用固定尺寸约束 - 保存引用以便动态更新
        // 设置最高优先级，防止被其他约束覆盖
        let wc = remoteView.widthAnchor.constraint(equalToConstant: currentContentSize.width)
        let hc = remoteView.heightAnchor.constraint(equalToConstant: currentContentSize.height)
        wc.priority = .required
        hc.priority = .required
        widthConstraint = wc
        heightConstraint = hc

        let leading = remoteView.leadingAnchor.constraint(equalTo: leadingAnchor)
        let top = remoteView.topAnchor.constraint(equalTo: topAnchor)
        leading.priority = .required
        top.priority = .required

        NSLayoutConstraint.activate([wc, hc, leading, top])

        NSLog("ViewBridgeHostView: Set NSRemoteView size to \(currentContentSize), self.frame=\(frame)")
        // Write to file for debugging
        let logMsg = "[handleConnectionResult] Set NSRemoteView size to \(currentContentSize), self.frame=\(frame)\n"
        if let data = logMsg.data(using: .utf8),
           let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
            let logPath = appSupport.appendingPathComponent("Canvas/viewbridge_host.log").path
            if let handle = FileHandle(forWritingAtPath: logPath) {
                handle.seekToEndOfFile()
                handle.write(data)
                handle.closeFile()
            }
        }

        // 通知 XPC Service 端使用正确的尺寸
        // 调用 requestResize:animation:completion: 方法
        let requestResizeSelector = NSSelectorFromString("requestResize:animation:completion:")
        if vc.responds(to: requestResizeSelector) {
            // Use performSelector with NSValue-wrapped size
            let targetSize = currentContentSize
            let sizeValue = NSValue(size: targetSize)
            // requestResize:animation:completion: requires more complex invocation
            // Fall back to preferredContentSize
            vc.preferredContentSize = currentContentSize
            NSLog("ViewBridgeHostView: Set preferredContentSize to \(currentContentSize)")
        } else {
            vc.preferredContentSize = currentContentSize
            NSLog("ViewBridgeHostView: Set preferredContentSize to \(currentContentSize) (requestResize not available)")
        }

        // 强制更新 remote view 的 frame
        let vcView = vc.view.superview ?? vc.view
        let oldFrame = vcView.frame
        vcView.setFrameSize(currentContentSize)
        NSLog("ViewBridgeHostView: Set vcView frame from \(oldFrame) to \(vcView.frame)")

        // 应用初始 scale factor
        if currentScaleFactor != 1.0 {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            layer?.transform = CATransform3DMakeScale(currentScaleFactor, currentScaleFactor, 1.0)
            CATransaction.commit()
            NSLog("ViewBridgeHostView: Applied initial scaleFactor %.3f", currentScaleFactor)
        }

        NSLog("ViewBridgeHostView: Remote view embedded successfully")
    }

    // MARK: - Content Size Management

    /// Update the content size (viewport size)
    /// This updates the fixed constraints and notifies the XPC service
    public func updateContentSize(_ newSize: CGSize) {
        // Ignore if size hasn't changed significantly (< 1px)
        let sizeDiff = abs(newSize.width - currentContentSize.width) + abs(newSize.height - currentContentSize.height)
        guard sizeDiff >= 1 else { return }

        // Validate reasonable size
        guard newSize.width >= 100 && newSize.height >= 100 &&
              newSize.width <= 4096 && newSize.height <= 4096 else {
            NSLog("ViewBridgeHostView: Ignoring unreasonable size \(newSize)")
            return
        }

        NSLog("ViewBridgeHostView: Updating content size from \(currentContentSize) to \(newSize)")
        currentContentSize = newSize

        // Update constraints
        widthConstraint?.constant = newSize.width
        heightConstraint?.constant = newSize.height

        // Notify XPC service via requestResize if available
        if let vc = remoteViewController {
            let selector = NSSelectorFromString("requestResize:animation:completion:")
            if vc.responds(to: selector) {
                // Use NSInvocation alternative - set preferred content size
                vc.preferredContentSize = newSize
                NSLog("ViewBridgeHostView: Requested resize to \(newSize)")
            }
        }
    }

    /// Update the scale factor for visual scaling via CALayer transform
    /// This does NOT trigger resize - only visual scaling
    public func updateScaleFactor(_ scaleFactor: CGFloat) {
        // Ignore if scale hasn't changed significantly
        let scaleDiff = abs(scaleFactor - currentScaleFactor)
        guard scaleDiff >= 0.001 else { return }

        currentScaleFactor = scaleFactor

        // Apply CALayer transform for visual scaling
        // This scales the view visually without changing its frame
        // so it won't trigger resize storms to the XPC service
        CATransaction.begin()
        CATransaction.setDisableActions(true)  // Disable implicit animations
        layer?.transform = CATransform3DMakeScale(scaleFactor, scaleFactor, 1.0)
        layer?.anchorPoint = CGPoint(x: 0, y: 0)  // Anchor at top-left
        CATransaction.commit()

        NSLog("ViewBridgeHostView: Updated scaleFactor to %.3f", scaleFactor)
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

/// Coordinator that holds reference to ViewBridgeHostView for snapshot access
class ViewBridgeCoordinator {
    weak var hostView: ViewBridgeHostView?

    func captureSnapshot() -> NSImage? {
        return hostView?.captureSnapshot()
    }

    func updateContentSize(_ size: CGSize) {
        hostView?.updateContentSize(size)
    }
}

/// SwiftUI wrapper for ViewBridgeHostView
struct ViewBridgeTabView: NSViewRepresentable {
    let tabId: UUID
    let contentSize: CGSize  // Viewport size for rendering
    let scaleFactor: CGFloat  // 缩放比例（CALayer transform 实现）
    let coordinator: ViewBridgeCoordinator

    func makeNSView(context: Context) -> ViewBridgeHostView {
        NSLog("ViewBridgeTabView: Creating ViewBridgeHostView for tab \(tabId), size: \(contentSize), scale: \(scaleFactor)")
        let view = ViewBridgeHostView(initialContentSize: contentSize, scaleFactor: scaleFactor)
        coordinator.hostView = view
        return view
    }

    func updateNSView(_ nsView: ViewBridgeHostView, context: Context) {
        coordinator.hostView = nsView
        // Only update if size is valid (> 0)
        if contentSize.width > 0 && contentSize.height > 0 {
            nsView.updateContentSize(contentSize)
        }
        // Update scale factor for CALayer transform
        nsView.updateScaleFactor(scaleFactor)
    }

    static func dismantleNSView(_ nsView: ViewBridgeHostView, coordinator: ()) {
        NSLog("ViewBridgeTabView: Disconnecting ViewBridgeHostView")
        nsView.disconnect()
    }
}

// MARK: - ViewBridge Multi-Process Tab View

/// A view that displays tab content via ViewBridge XPC service
/// ViewBridge renders at fixed containerSize, uses CALayer transform for visual scaling
struct ViewBridgeMultiProcessTabView: View {
    let tab: Tab
    let containerSize: CGSize
    let scaleFactor: CGFloat  // 缩放比例（CALayer transform 实现，不触发 resize）

    @State private var isLoading = true
    @State private var errorMessage: String?
    @State private var coordinator = ViewBridgeCoordinator()

    var body: some View {
        ZStack {
            if ViewBridgeHostView.isAvailable {
                // ViewBridge renders at containerSize (viewport size)
                // Uses CALayer transform for visual scaling without triggering resize
                ViewBridgeTabView(
                    tabId: tab.id,
                    contentSize: containerSize,
                    scaleFactor: scaleFactor,
                    coordinator: coordinator
                )
                .frame(width: containerSize.width * scaleFactor, height: containerSize.height * scaleFactor)
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
