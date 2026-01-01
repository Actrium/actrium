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

        // Create container view with flexible sizing
        let containerView = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        containerView.wantsLayer = true
        containerView.layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
        // Enable auto-resizing to fill parent view
        containerView.autoresizingMask = [.width, .height]

        // Set as view using KVC
        self_.setValue(containerView, forKey: "view")

        debugLog("Container view set, frame: \(containerView.frame)")

        // Wait for the view to be properly sized by ViewBridge before adding RN content
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
            debugLog("Delayed setup, container frame: \(containerView.frame), bounds: \(containerView.bounds)")
            setupReactNativeContent(in: containerView)
        }
    }
    let loadViewImp = imp_implementationWithBlock(loadViewBlock)
    class_addMethod(newClass, #selector(NSViewController.loadView), loadViewImp, "v@:")

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
    let testProperties: [String: Any] = [
        "tabs": [
            [
                "id": UUID().uuidString,
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
                "rightPanelDefaultVisible": false
            ]
        ],
        "currentTabId": UUID().uuidString,
        "isLeftPanelVisible": false,
        "isRightPanelVisible": false
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
