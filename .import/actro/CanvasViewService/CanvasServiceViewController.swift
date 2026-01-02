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
    let timestamp = ISO8601DateFormatter().string(from: Date())
    let logMessage = "[\(timestamp)] \(message)\n"

    // Write to stderr (visible in console)
    fputs("[CanvasViewService] \(message)\n", stderr)
    fflush(stderr)

    // Also append to log file in container
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

    override func setFrameSize(_ newSize: NSSize) {
        // 直接使用 ViewBridge 传来的尺寸，无需任何过滤或锁定
        debugLog("Container setFrameSize: \(newSize)")
        super.setFrameSize(newSize)

        // 更新 RN view 尺寸
        if let rnView = rnView {
            rnView.frame = bounds
        }
    }

    func setRNView(_ view: NSView) {
        self.rnView = view
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

private class ServiceReactNativeHost {
    static let shared = ServiceReactNativeHost()

    private let factoryDelegate: ServiceReactNativeFactoryDelegate
    private let factory: RCTReactNativeFactory
    private var rootView: RCTRootView?

    private init() {
        debugLog("Initializing React Native host")

        #if DEBUG
        RCTRedBoxSetEnabled(false)
        #endif

        factoryDelegate = ServiceReactNativeFactoryDelegate()
        factory = RCTReactNativeFactory(delegate: factoryDelegate)
        debugLog("React Native factory created")
    }

    func createRootView(initialProperties: [String: Any]) -> NSView {
        debugLog("Creating RCTRootView with module: ActrApp")

        let view = factory.rootViewFactory.view(
            withModuleName: "ActrApp",
            initialProperties: initialProperties,
            launchOptions: nil
        )

        if let rctRootView = view as? RCTRootView {
            self.rootView = rctRootView
            rctRootView.sizeFlexibility = .none
            rctRootView.backgroundColor = NSColor.clear
            debugLog("RCTRootView created successfully")
        } else {
            debugLog("WARNING: View is not RCTRootView, type: \(type(of: view))")
        }

        return view
    }

    func updateProperties(_ properties: [String: Any]) {
        rootView?.appProperties = properties
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
    let loadViewBlock: @convention(block) (AnyObject) -> Void = { (self_) in
        debugLog("loadView called")

        // Use a reasonable default size instead of screen size
        // This prevents oversized initial frames
        let initialWidth: CGFloat = 1280
        let initialHeight: CGFloat = 720
        debugLog("Using default initial size: \(initialWidth)x\(initialHeight)")

        let containerView = ResizableContainerView(frame: NSRect(x: 0, y: 0, width: initialWidth, height: initialHeight))
        containerView.wantsLayer = true
        containerView.layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
        // Disable auto-resizing - we control size via remoteViewSizeDidChange
        containerView.autoresizingMask = []

        // Set as view using KVC
        self_.setValue(containerView, forKey: "view")

        debugLog("Container view set, frame: \(containerView.frame)")

        // Try to request a fixed size from host using requestResize:animation:completion:
        let requestResizeSelector = NSSelectorFromString("requestResize:animation:completion:")
        debugLog("Step 1: Calling requestResize to lock size at \(initialWidth)x\(initialHeight)")

        debugLog("Step 2: Creating targetSize")
        let targetSize = NSSize(width: initialWidth, height: initialHeight)
        debugLog("Step 3: targetSize created")

        // Simpler approach: use setValue for preferredContentSize
        debugLog("Step 4: Checking preferredContentSize")
        let preferredSizeSelector = NSSelectorFromString("setPreferredContentSize:")
        if self_.responds(to: preferredSizeSelector) {
            debugLog("Step 5: Setting preferredContentSize to \(targetSize)")
            self_.setValue(NSValue(size: targetSize), forKey: "preferredContentSize")
            debugLog("Step 6: preferredContentSize set")
        } else {
            debugLog("Step 5b: preferredContentSize not supported")
        }

        debugLog("Step 7: About to call setupReactNativeContent...")
        setupReactNativeContent(in: containerView)
        debugLog("Step 8: setupReactNativeContent returned")
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

private func setupReactNativeContent(in containerView: NSView) {
    debugLog("Setting up React Native content")

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

    // Create React Native view with test properties
    // TODO: Get actual tab config from host via IPC
    let testTabId = UUID().uuidString  // Use same ID for tab and currentTabId
    let testProperties: [String: Any] = [
        "tabs": [
            [
                "id": testTabId,
                "title": "ViewBridge Test",
                "backgroundColor": "#1a4a3c",
                "unifiedScript": """
                // Simple test app rendered in XPC service
                const { View, Text } = require('react-native');
                module.exports = function TestApp() {
                    return React.createElement(View, {
                        style: { flex: 1, justifyContent: 'center', alignItems: 'center', backgroundColor: '#1a4a3c' }
                    }, [
                        React.createElement(Text, {
                            key: 'title',
                            style: { fontSize: 24, fontWeight: 'bold', color: 'white' }
                        }, 'React Native in XPC Service!'),
                        React.createElement(Text, {
                            key: 'subtitle',
                            style: { fontSize: 14, color: 'rgba(255,255,255,0.7)', marginTop: 10 }
                        }, 'Running in CanvasViewService process')
                    ]);
                };
                """,
                "rightPanelDefaultVisible": true
            ]
        ],
        "currentTabId": testTabId,  // Same as tab id
        "isLeftPanelVisible": true,
        "isRightPanelVisible": true
    ]

    // Create RN view in background to not block
    DispatchQueue.global(qos: .userInitiated).async {
        // Small delay to let ViewBridge connection stabilize
        Thread.sleep(forTimeInterval: 0.5)

        DispatchQueue.main.async {
            debugLog("Creating React Native root view")

            // Remove loading indicator
            loadingLabel.removeFromSuperview()

            // Create and add React Native view
            let rnView = ServiceReactNativeHost.shared.createRootView(initialProperties: testProperties)

            // Use frame-based layout instead of Auto Layout (RCTRootView may not work well with Auto Layout)
            rnView.frame = containerView.bounds
            rnView.autoresizingMask = [.width, .height]
            containerView.addSubview(rnView)

            // Register RN view with ResizableContainerView for resize handling
            if let resizableContainer = containerView as? ResizableContainerView {
                resizableContainer.setRNView(rnView)
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
