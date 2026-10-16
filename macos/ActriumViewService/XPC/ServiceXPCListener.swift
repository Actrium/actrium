//
//  ServiceXPCListener.swift
//  ActriumViewService
//
//  XPC listener for receiving commands from Host.
//  Uses anonymous listener since Service runs via ViewBridge (not standard XPC Service entry).
//

import Foundation
import AppKit

// MARK: - Service XPC Logging

/// Log path for Service XPC debugging (shared temp directory)
private let xpcLogPath: String = {
    let tempDir = NSTemporaryDirectory()
    return (tempDir as NSString).appendingPathComponent("actrium-xpc-service.log")
}()

/// Write to the shared Service log file
private func xpcLog(_ message: String) {
    NSLog("[ServiceXPC] %@", message)
    fputs("[ServiceXPC] \(message)\n", stderr)
    fflush(stderr)

    // Also write to log file
    let timestamp = ISO8601DateFormatter().string(from: Date())
    let logMessage = "[\(timestamp)] [ServiceXPC] \(message)\n"
    if let data = logMessage.data(using: .utf8) {
        if FileManager.default.fileExists(atPath: xpcLogPath) {
            if let handle = FileHandle(forWritingAtPath: xpcLogPath) {
                handle.seekToEndOfFile()
                handle.write(data)
                handle.closeFile()
            }
        } else {
            try? data.write(to: URL(fileURLWithPath: xpcLogPath))
        }
    }
}

// MARK: - Service XPC Listener

/// XPC listener for the Service side
/// Uses anonymous listener pattern for ViewBridge-based services
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
    /// Uses anonymous listener since ViewBridge services can't use NSXPCListener.service()
    public func start() {
        xpcLog("start() called for service: \(serviceBundleId)")

        guard !isListening else {
            xpcLog("Already listening, skipping")
            return
        }

        xpcLog("Starting XPC listener...")

        // Use anonymous listener (ViewBridge services aren't standard XPC Services)
        listener = NSXPCListener.anonymous()
        listener?.delegate = self
        listener?.resume()

        xpcLog("Anonymous listener created and resumed")

        isListening = true

        // Export endpoint via notification so Host can connect
        exportEndpoint()

        xpcLog("XPC listener started successfully")
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

        NSLog("[ServiceXPC] XPC listener stopped")
    }

    // MARK: - Endpoint Export

    /// Notify Host that this Service's XPC listener is ready
    /// NOTE: NSXPCListenerEndpoint cannot be serialized for DN transmission.
    /// Instead, we send a "ready" notification and use DN for message passing.
    /// Future: Pass endpoint through ViewBridge's XPC connection.
    private func exportEndpoint() {
        xpcLog("exportEndpoint called for \(serviceBundleId)")

        guard listener != nil else {
            xpcLog("exportEndpoint: listener is nil!")
            return
        }

        xpcLog("exportEndpoint: sending ready notification (endpoint stays local)")

        // Send a simple "ready" notification without the endpoint
        // Host will continue using DistributedNotificationCenter for communication
        var userInfo: [String: Any] = [
            "bundleId": serviceBundleId,
            "processId": ProcessInfo.processInfo.processIdentifier,
            "listenerReady": true
        ]

        if let hostPid = hostProcessId {
            userInfo["hostProcessId"] = hostPid
        }

        // Use host PID for process isolation if known
        let notificationName = hostProcessId.map { "io.actrium.xpcEndpoint.\($0)" }
            ?? "io.actrium.xpcEndpoint"

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: userInfo,
            deliverImmediately: true
        )

        xpcLog("Ready notification sent via DN: \(notificationName)")
    }

    /// Set host process ID and re-export endpoint
    /// Also triggers state reporting since we now know where to send it
    public func setHostProcessId(_ pid: Int32) {
        let isFirstSet = (hostProcessId == nil)
        self.hostProcessId = pid
        requestHandler.setHostProcessId(pid)
        xpcLog("setHostProcessId: \(pid) (firstSet=\(isFirstSet))")

        if isListening {
            exportEndpoint()

            // If this is the first time setting hostProcessId and service is already ready,
            // report state now (since earlier reportState calls were skipped due to nil hostProcessId)
            if isFirstSet {
                reportState(.ready)
            }
        }
    }

    // MARK: - State Reporting

    /// Service state for reporting to Host
    public enum ServiceState: String, Sendable {
        case ready = "ready"
        case busy = "busy"
        case error = "error"
        case terminating = "terminating"
    }

    /// Current service state
    private var currentState: ServiceState = .ready

    /// Report current state to Host via DistributedNotification
    /// Host's IPCInteractionManager listens for this and updates its state machine
    public func reportState(_ state: ServiceState, details: [String: Any]? = nil) {
        currentState = state

        guard let hostPid = hostProcessId else {
            xpcLog("reportState: no hostProcessId, skipping state report")
            return
        }

        var userInfo: [String: Any] = [
            "state": state.rawValue,
            "serviceId": serviceBundleId,
            "processId": ProcessInfo.processInfo.processIdentifier,
            "timestamp": ISO8601DateFormatter().string(from: Date())
        ]

        if let details = details {
            userInfo["details"] = details
        }

        // Use host PID for process isolation
        let notificationName = "io.actrium.serviceState.\(hostPid)"

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: userInfo,
            deliverImmediately: true
        )

        xpcLog("reportState: \(state.rawValue) sent to Host (pid=\(hostPid))")
    }

    // MARK: - Host Callbacks

    /// Send a callback to Host
    private func sendToHost(_ block: @escaping (ActriumServiceXPCProtocol) -> Void) {
        guard let connection = hostConnection else {
            NSLog("[ServiceXPC] No host connection for callback")
            return
        }

        let proxy = connection.remoteObjectProxyWithErrorHandler { error in
            NSLog("[ServiceXPC] Host callback error: %@", error.localizedDescription)
        }

        guard let hostProxy = proxy as? ActriumServiceXPCProtocol else {
            NSLog("[ServiceXPC] Invalid host proxy type")
            return
        }

        block(hostProxy)
    }

    /// Notify Host that size has been applied
    public func notifySizeApplied(tabId: String, size: CGSize, requestId: Int) {
        sendToHost { (proxy: ActriumServiceXPCProtocol) in
            proxy.sizeDidApply(tabId: tabId, width: Double(size.width), height: Double(size.height), requestId: requestId)
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

        // Report interrupted state to Host
        reportState(.error, details: ["reason": "connection_interrupted"])

        // Notify any waiters that the connection is temporarily unavailable
        // The connection will auto-reconnect when Host makes a new call
    }

    private func handleConnectionInvalidation() {
        // Connection is permanently closed
        NSLog("[ServiceXPC] Handling connection invalidation")

        // Report terminating state to Host
        reportState(.terminating, details: ["reason": "connection_invalidated"])

        hostConnection = nil

        // Re-export endpoint so Host can reconnect
        if isListening {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.exportEndpoint()
            }
        }
    }

    private func notifyServiceReady() {
        // Report state to Host via DistributedNotification (state machine driven)
        reportState(.ready)

        // Also send via XPC callback for backward compatibility
        sendToHost { [weak self] (proxy: ActriumServiceXPCProtocol) in
            guard let self = self else { return }
            proxy.serviceDidBecomeReady(bundleId: self.serviceBundleId)
        }
    }
}
