//
//  ServiceXPCListener.swift
//  CanvasViewService
//
//  XPC listener for receiving commands from Host.
//  Uses anonymous listener with endpoint exported via DistributedNotification.
//

import Foundation
import AppKit

// MARK: - Service XPC Listener

/// XPC listener for the Service side
/// Creates an anonymous listener and exports endpoint to Host via notification
public final class ServiceXPCListener: NSObject {
    // MARK: - Singleton

    public static let shared = ServiceXPCListener()

    // MARK: - Properties

    /// The anonymous XPC listener
    private var listener: NSXPCListener?

    /// Connection to Host for callbacks
    private var hostConnection: NSXPCConnection?

    /// Request handler for processing incoming requests
    /// Exposed for delegate assignment
    let requestHandler: ServiceRequestHandler

    /// Service bundle identifier
    private let serviceBundleId: String

    /// Host process ID for notification routing
    private var hostProcessId: Int32?

    /// Whether the listener has been started
    private(set) var isListening = false

    /// Whether XPC mode is enabled (vs DistributedNotification fallback)
    private(set) var isXPCEnabled = false

    // MARK: - Initialization

    private override init() {
        // Get service bundle ID
        let executablePath = ProcessInfo.processInfo.arguments[0]
        var bundlePath = (executablePath as NSString).deletingLastPathComponent
        bundlePath = (bundlePath as NSString).deletingLastPathComponent
        bundlePath = (bundlePath as NSString).deletingLastPathComponent
        if let serviceBundle = Bundle(path: bundlePath),
           let bundleId = serviceBundle.bundleIdentifier {
            serviceBundleId = bundleId
        } else {
            serviceBundleId = Bundle.main.bundleIdentifier ?? "unknown"
        }

        requestHandler = ServiceRequestHandler()
        super.init()
    }

    // MARK: - Lifecycle

    /// Start the XPC listener
    /// Creates an anonymous listener and exports its endpoint
    public func start() {
        guard !isListening else {
            NSLog("[ServiceXPC] Already listening")
            return
        }

        NSLog("[ServiceXPC] Starting XPC listener for service: %@", serviceBundleId)

        // Create anonymous listener
        listener = NSXPCListener.anonymous()
        listener?.delegate = self
        listener?.resume()

        isListening = true
        isXPCEnabled = true

        // Export endpoint via notification
        exportEndpoint()

        NSLog("[ServiceXPC] XPC listener started")
    }

    /// Stop the XPC listener
    public func stop() {
        guard isListening else { return }

        NSLog("[ServiceXPC] Stopping XPC listener")

        listener?.invalidate()
        listener = nil
        hostConnection?.invalidate()
        hostConnection = nil

        isListening = false
        isXPCEnabled = false

        NSLog("[ServiceXPC] XPC listener stopped")
    }

    // MARK: - Endpoint Export

    /// Export the listener endpoint via DistributedNotification
    /// Host will receive this and connect using the endpoint
    private func exportEndpoint() {
        guard let listener = listener else {
            NSLog("[ServiceXPC] Cannot export endpoint - no listener")
            return
        }

        let endpoint = listener.endpoint

        // Archive endpoint for transfer
        do {
            let data = try NSKeyedArchiver.archivedData(
                withRootObject: endpoint,
                requiringSecureCoding: true
            )
            let base64 = data.base64EncodedString()

            // Post notification with endpoint
            var userInfo: [String: Any] = [
                "bundleId": serviceBundleId,
                "endpoint": base64,
                "processId": ProcessInfo.processInfo.processIdentifier
            ]

            if let hostPid = hostProcessId {
                userInfo["hostProcessId"] = hostPid
            }

            DistributedNotificationCenter.default().postNotificationName(
                NSNotification.Name("com.canvas.xpcEndpoint"),
                object: nil,
                userInfo: userInfo,
                deliverImmediately: true
            )

            NSLog("[ServiceXPC] Endpoint exported via notification")
        } catch {
            NSLog("[ServiceXPC] Failed to archive endpoint: %@", error.localizedDescription)
        }
    }

    /// Re-export endpoint with host process ID
    public func setHostProcessId(_ pid: Int32) {
        self.hostProcessId = pid
        requestHandler.setHostProcessId(pid)

        // Re-export endpoint with host PID
        if isListening {
            exportEndpoint()
        }
    }

    // MARK: - Host Callbacks

    /// Send a callback to Host
    private func sendToHost(_ block: @escaping (CanvasServiceXPCProtocol) -> Void) {
        guard let connection = hostConnection else {
            NSLog("[ServiceXPC] No host connection for callback")
            return
        }

        let proxy = connection.remoteObjectProxyWithErrorHandler { error in
            NSLog("[ServiceXPC] Host callback error: %@", error.localizedDescription)
        }

        guard let hostProxy = proxy as? CanvasServiceXPCProtocol else {
            NSLog("[ServiceXPC] Invalid host proxy type")
            return
        }

        block(hostProxy)
    }

    /// Notify Host that size has been applied
    public func notifySizeApplied(tabId: String, size: CGSize, requestId: Int) {
        sendToHost { proxy in
            proxy.sizeDidApply(tabId: tabId, width: size.width, height: size.height, requestId: requestId)
        }
    }

    /// Request Host to open a URL
    public func requestOpenURL(_ url: String, fromTab tabId: String) {
        sendToHost { proxy in
            proxy.requestOpenURL(url, fromTab: tabId)
        }
    }

    /// Report an error to Host
    public func reportError(code: Int, message: String, tabId: String? = nil) {
        sendToHost { proxy in
            let error = ServiceXPCServiceError(code: code, message: message, tabId: tabId)
            proxy.reportError(error)
        }
    }

    // MARK: - Connection Validation

    /// Validate the connecting client
    /// For sandboxed apps, this checks code signing
    private func validateClient(_ connection: NSXPCConnection) -> Bool {
        // In production, verify the client's code signature matches our expectations
        // For now, we trust all connections from the same app bundle

        let pid = connection.processIdentifier
        NSLog("[ServiceXPC] Validating client with PID: %d", pid)

        // TODO: Implement proper code signature validation
        // let code = SecCodeCreateWithPID(pid, ...)
        // SecCodeCheckValidity(code, kSecCSDefaultFlags, requirement)

        return true
    }
}

// MARK: - NSXPCListenerDelegate

extension ServiceXPCListener: NSXPCListenerDelegate {
    public func listener(
        _ listener: NSXPCListener,
        shouldAcceptNewConnection newConnection: NSXPCConnection
    ) -> Bool {
        NSLog("[ServiceXPC] New connection request from PID: %d", newConnection.processIdentifier)

        // Validate the client
        guard validateClient(newConnection) else {
            NSLog("[ServiceXPC] Client validation failed")
            return false
        }

        // Configure the connection
        // Host → Service: we export this interface
        newConnection.exportedInterface = ServiceXPCInterfaceFactory.createHostToServiceInterface()
        newConnection.exportedObject = requestHandler

        // Service → Host: we call this interface
        newConnection.remoteObjectInterface = ServiceXPCInterfaceFactory.createServiceToHostInterface()

        // Handle connection lifecycle
        newConnection.interruptionHandler = { [weak self] in
            NSLog("[ServiceXPC] Connection interrupted")
            self?.handleConnectionInterruption()
        }

        newConnection.invalidationHandler = { [weak self] in
            NSLog("[ServiceXPC] Connection invalidated")
            self?.handleConnectionInvalidation()
        }

        // Store host connection for callbacks
        hostConnection = newConnection

        // Activate the connection
        newConnection.resume()

        NSLog("[ServiceXPC] Connection accepted from PID: %d", newConnection.processIdentifier)

        // Notify Host that we're ready
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
            self?.notifyServiceReady()
        }

        return true
    }

    private func handleConnectionInterruption() {
        // Connection was interrupted but may reconnect
        NSLog("[ServiceXPC] Handling connection interruption")

        // Notify any waiters that the connection is temporarily unavailable
        // The connection will auto-reconnect when Host makes a new call
    }

    private func handleConnectionInvalidation() {
        // Connection is permanently closed
        NSLog("[ServiceXPC] Handling connection invalidation")

        hostConnection = nil

        // Re-export endpoint in case Host wants to reconnect
        if isListening {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.exportEndpoint()
            }
        }
    }

    private func notifyServiceReady() {
        sendToHost { [weak self] proxy in
            guard let self = self else { return }
            let ready = ServiceXPCServiceReady(
                bundleId: self.serviceBundleId,
                processId: ProcessInfo.processInfo.processIdentifier
            )
            proxy.serviceDidBecomeReady(ready)
        }
    }
}
