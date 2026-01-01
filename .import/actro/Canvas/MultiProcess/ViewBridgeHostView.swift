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
        remoteView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(remoteView)

        NSLayoutConstraint.activate([
            remoteView.leadingAnchor.constraint(equalTo: leadingAnchor),
            remoteView.trailingAnchor.constraint(equalTo: trailingAnchor),
            remoteView.topAnchor.constraint(equalTo: topAnchor),
            remoteView.bottomAnchor.constraint(equalTo: bottomAnchor)
        ])

        NSLog("ViewBridgeHostView: Remote view embedded successfully")
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

/// SwiftUI wrapper for ViewBridgeHostView
struct ViewBridgeTabView: NSViewRepresentable {
    let tabId: UUID

    func makeNSView(context: Context) -> ViewBridgeHostView {
        NSLog("ViewBridgeTabView: Creating ViewBridgeHostView for tab \(tabId)")
        let view = ViewBridgeHostView()
        return view
    }

    func updateNSView(_ nsView: ViewBridgeHostView, context: Context) {
        // ViewBridge handles updates automatically
    }

    static func dismantleNSView(_ nsView: ViewBridgeHostView, coordinator: ()) {
        NSLog("ViewBridgeTabView: Disconnecting ViewBridgeHostView")
        nsView.disconnect()
    }
}

// MARK: - ViewBridge Multi-Process Tab View

/// A view that displays tab content via ViewBridge XPC service
struct ViewBridgeMultiProcessTabView: View {
    let tab: Tab
    let containerSize: CGSize

    @State private var isLoading = true
    @State private var errorMessage: String?

    var body: some View {
        ZStack {
            if ViewBridgeHostView.isAvailable {
                ViewBridgeTabView(tabId: tab.id)
                    .frame(width: containerSize.width, height: containerSize.height)
                    .onAppear {
                        NSLog("ViewBridgeMultiProcessTabView: ViewBridge view appeared for tab \(tab.id)")
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
