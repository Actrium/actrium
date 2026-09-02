//
//  XPCConnectionTransport.swift
//  Actrium
//
//  Transport implementation using NSXPCConnection.
//  This is the recommended transport for production use.
//

import Foundation

// MARK: - XPC Connection Transport

/// Transport layer implementation using NSXPCConnection
public final class XPCConnectionTransport: NSObject, IPCTransport, @unchecked Sendable {
    // MARK: - Properties

    public let transportId: String
    public weak var delegate: IPCTransportDelegate?

    private let lock = NSLock()
    private var _state: IPCTransportState = .disconnected
    private var connections: [String: NSXPCConnection] = [:]
    private var connectionQueue = DispatchQueue(label: "io.actrium.app.xpc.connection")

    /// Stored endpoints for anonymous listener connections
    private var endpoints: [String: NSXPCListenerEndpoint] = [:]

    /// Host process ID for isolation
    private let hostProcessId: Int32

    /// Configuration
    private let configuration: IPCConfiguration

    // MARK: - Host Callback Handler

    private var hostCallbackHandler: XPCHostCallbackHandler?

    // MARK: - Initialization

    public init(
        transportId: String = UUID().uuidString,
        configuration: IPCConfiguration = .default
    ) {
        self.transportId = transportId
        self.configuration = configuration
        self.hostProcessId = configuration.hostProcessId
        super.init()

        self.hostCallbackHandler = XPCHostCallbackHandler(transport: self)
        setupEndpointListener()
    }

    // MARK: - Endpoint Management

    /// Store an endpoint for a service (used for anonymous listener pattern)
    public func storeEndpoint(_ endpoint: NSXPCListenerEndpoint, for serviceId: String) {
        lock.lock()
        endpoints[serviceId] = endpoint
        lock.unlock()

        IPCLogger.shared.info(
            "Stored endpoint for service",
            category: .connection,
            serviceId: serviceId
        )
    }

    /// Setup listener for endpoint notifications from Service
    private func setupEndpointListener() {
        let notificationName = "io.actrium.app.xpcEndpoint.\(hostProcessId)"

        DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(notificationName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleEndpointNotification(notification)
        }

        // Also listen for generic endpoint notifications (before host PID is known)
        DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("io.actrium.app.xpcEndpoint"),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            self?.handleEndpointNotification(notification)
        }

        NSLog("📡 [XPCTransport] Listening for endpoints (pid=%d)", hostProcessId)
    }

    private func handleEndpointNotification(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let bundleId = userInfo["bundleId"] as? String else {
            return
        }

        // Verify this is for our process if hostProcessId is included
        if let notificationHostPid = userInfo["hostProcessId"] as? Int32,
           notificationHostPid != hostProcessId {
            return
        }

        // Check if this is a "listenerReady" notification (no endpoint)
        if userInfo["listenerReady"] as? Bool == true {
            NSLog("📡 [XPCTransport] Service listener ready: %@ (using serviceName fallback)", bundleId.components(separatedBy: ".").last ?? "")
            // Mark service as ready for serviceName connection
            // Connection will use NSXPCConnection(serviceName:) fallback
            return
        }

        // Try to decode endpoint if provided
        guard let endpointBase64 = userInfo["endpoint"] as? String,
              let endpointData = Data(base64Encoded: endpointBase64) else {
            NSLog("⚠️ [XPCTransport] No endpoint in notification for %@", bundleId)
            return
        }

        // Decode endpoint
        guard let endpoint = try? NSKeyedUnarchiver.unarchivedObject(
            ofClass: NSXPCListenerEndpoint.self,
            from: endpointData
        ) else {
            NSLog("❌ [XPCTransport] Failed to decode endpoint for %@", bundleId)
            return
        }

        NSLog("📡 [XPCTransport] Received endpoint for %@", bundleId.components(separatedBy: ".").last ?? "")
        storeEndpoint(endpoint, for: bundleId)
    }

    // MARK: - IPCTransport

    public var state: IPCTransportState {
        get async {
            lock.lock()
            defer { lock.unlock() }
            return _state
        }
    }

    public func connect(to serviceId: String) async throws {
        lock.lock()
        if connections[serviceId] != nil {
            lock.unlock()
            return // Already connected
        }

        // Check for stored endpoint
        let endpoint = endpoints[serviceId]
        lock.unlock()

        // Wait for endpoint if not available (ViewBridge services use anonymous listeners)
        let resolvedEndpoint: NSXPCListenerEndpoint?
        if endpoint == nil {
            resolvedEndpoint = await waitForEndpoint(serviceId: serviceId, timeout: 5.0)
        } else {
            resolvedEndpoint = endpoint
        }

        // Create connection on dedicated queue
        let connection = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<NSXPCConnection, Error>) in
            connectionQueue.async { [weak self] in
                guard let self = self else {
                    continuation.resume(throwing: IPCTransportError.invalidated)
                    return
                }

                let conn: NSXPCConnection
                if let ep = resolvedEndpoint {
                    // Use endpoint for anonymous listener connection
                    conn = NSXPCConnection(listenerEndpoint: ep)
                    NSLog("📡 [XPCTransport] Connecting via endpoint to %@", serviceId.components(separatedBy: ".").last ?? "")
                } else {
                    // Fallback to service name (standard XPC Service)
                    conn = NSXPCConnection(serviceName: serviceId)
                    NSLog("📡 [XPCTransport] Connecting via serviceName to %@", serviceId.components(separatedBy: ".").last ?? "")
                }

                // Configure interfaces
                conn.remoteObjectInterface = XPCInterfaceFactory.hostInterface()
                conn.exportedInterface = XPCInterfaceFactory.serviceInterface()
                conn.exportedObject = self.hostCallbackHandler

                // Configure handlers
                conn.interruptionHandler = { [weak self] in
                    self?.handleInterruption(serviceId: serviceId)
                }

                conn.invalidationHandler = { [weak self] in
                    self?.handleInvalidation(serviceId: serviceId)
                }

                // Resume (activate) the connection
                conn.resume()

                continuation.resume(returning: conn)
            }
        }

        // Store connection
        lock.lock()
        connections[serviceId] = connection
        _state = .connected
        lock.unlock()

        delegate?.transport(self, didChangeState: .connected)

        IPCLogger.shared.info(
            "XPC connection established",
            category: .connection,
            serviceId: serviceId
        )
    }

    /// Wait for endpoint to be received
    private func waitForEndpoint(serviceId: String, timeout: TimeInterval) async -> NSXPCListenerEndpoint? {
        let startTime = Date()
        let checkInterval: TimeInterval = 0.1

        while Date().timeIntervalSince(startTime) < timeout {
            lock.lock()
            if let endpoint = endpoints[serviceId] {
                lock.unlock()
                return endpoint
            }
            lock.unlock()

            try? await Task.sleep(nanoseconds: UInt64(checkInterval * 1_000_000_000))
        }

        NSLog("⚠️ [XPCTransport] Timeout waiting for endpoint: %@", serviceId.components(separatedBy: ".").last ?? "")
        return nil
    }

    public func disconnect(from serviceId: String) async {
        lock.lock()
        guard let connection = connections.removeValue(forKey: serviceId) else {
            lock.unlock()
            return
        }

        if connections.isEmpty {
            _state = .disconnected
        }
        lock.unlock()

        connectionQueue.async {
            connection.invalidate()
        }

        if connections.isEmpty {
            delegate?.transport(self, didChangeState: .disconnected)
        }

        IPCLogger.shared.info(
            "XPC connection disconnected",
            category: .connection,
            serviceId: serviceId
        )
    }

    public func disconnectAll() async {
        lock.lock()
        let allConnections = connections
        connections.removeAll()
        _state = .disconnected
        lock.unlock()

        connectionQueue.async {
            for (_, connection) in allConnections {
                connection.invalidate()
            }
        }

        delegate?.transport(self, didChangeState: .disconnected)
    }

    public func send(_ data: Data, to serviceId: String) async throws {
        try await send(data, to: serviceId, timeout: configuration.defaultTimeout)
    }

    public func send(_ data: Data, to serviceId: String, timeout: TimeInterval) async throws {
        lock.lock()
        guard let connection = connections[serviceId] else {
            lock.unlock()
            throw IPCTransportError.notConnected
        }
        lock.unlock()

        // Get proxy with error handler
        let proxy = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<ActriumHostXPCProtocol, Error>) in
            let proxyOrNil = connection.remoteObjectProxyWithErrorHandler { error in
                continuation.resume(throwing: IPCTransportError.sendFailed(error.localizedDescription))
            } as? ActriumHostXPCProtocol

            if let proxy = proxyOrNil {
                continuation.resume(returning: proxy)
            } else {
                continuation.resume(throwing: IPCTransportError.sendFailed("Failed to get proxy"))
            }
        }

        // Decode the request type from data to call appropriate method
        // For now, we'll use a simple type indicator at the start

        // This is a simplified implementation
        // In production, you'd have a proper message envelope

        // Call ping as a test
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            proxy.ping { success in
                if success {
                    continuation.resume()
                } else {
                    continuation.resume(throwing: IPCTransportError.sendFailed("Ping failed"))
                }
            }
        }
    }

    public func isConnected(to serviceId: String) async -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return connections[serviceId] != nil
    }

    public func connectedServices() async -> [String] {
        lock.lock()
        defer { lock.unlock() }
        return Array(connections.keys)
    }

    public func invalidate() async {
        await disconnectAll()
        delegate?.transportDidInvalidate(self, reason: "Transport invalidated")
    }

    // MARK: - XPC Operations

    /// Send tab configuration via XPC
    public func sendTabConfig(_ config: IPCXPCTabConfig, to serviceId: String) async throws {
        lock.lock()
        guard let connection = connections[serviceId] else {
            lock.unlock()
            throw IPCTransportError.notConnected
        }
        lock.unlock()

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let proxy = connection.remoteObjectProxyWithErrorHandler { error in
                continuation.resume(throwing: IPCTransportError.sendFailed(error.localizedDescription))
            } as! ActriumHostXPCProtocol

            proxy.sendTabConfig(config) { success, errorMessage in
                if success {
                    continuation.resume()
                } else {
                    continuation.resume(throwing: IPCTransportError.sendFailed(errorMessage ?? "Unknown error"))
                }
            }
        }
    }

    /// Update container size via XPC
    public func updateContainerSize(_ size: IPCXPCContainerSize, to serviceId: String) async throws {
        lock.lock()
        guard let connection = connections[serviceId] else {
            lock.unlock()
            throw IPCTransportError.notConnected
        }
        lock.unlock()

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let proxy = connection.remoteObjectProxyWithErrorHandler { error in
                continuation.resume(throwing: IPCTransportError.sendFailed(error.localizedDescription))
            } as! ActriumHostXPCProtocol

            proxy.updateContainerSize(size) { success, errorMessage in
                if success {
                    continuation.resume()
                } else {
                    continuation.resume(throwing: IPCTransportError.sendFailed(errorMessage ?? "Unknown error"))
                }
            }
        }
    }

    /// Ping a service
    public func ping(serviceId: String) async -> Bool {
        lock.lock()
        guard let connection = connections[serviceId] else {
            lock.unlock()
            return false
        }
        lock.unlock()

        return await withCheckedContinuation { continuation in
            let proxy = connection.remoteObjectProxyWithErrorHandler { _ in
                continuation.resume(returning: false)
            } as! ActriumHostXPCProtocol

            proxy.ping { success in
                continuation.resume(returning: success)
            }
        }
    }

    // MARK: - Connection Event Handlers

    private func handleInterruption(serviceId: String) {
        lock.lock()
        _state = .interrupted
        lock.unlock()

        delegate?.transport(self, didChangeState: .interrupted)
        delegate?.transportDidInterrupt(self)

        IPCLogger.shared.warning(
            "XPC connection interrupted",
            category: .connection,
            serviceId: serviceId
        )

        Task {
            await IPCMetrics.shared.increment(.connectionInterrupted, serviceId: serviceId)
        }
    }

    private func handleInvalidation(serviceId: String) {
        lock.lock()
        connections.removeValue(forKey: serviceId)
        if connections.isEmpty {
            _state = .disconnected
        }
        lock.unlock()

        delegate?.transport(self, didChangeState: .failed("Connection invalidated"))
        delegate?.transportDidInvalidate(self, reason: "Connection invalidated")

        IPCLogger.shared.error(
            "XPC connection invalidated",
            category: .connection,
            serviceId: serviceId
        )

        Task {
            await IPCMetrics.shared.increment(.connectionInvalidated, serviceId: serviceId)
        }
    }

    // MARK: - Callback Forwarding

    /// Called by XPCHostCallbackHandler when service sends data
    func handleServiceCallback(data: Data, from serviceId: String) {
        delegate?.transport(self, didReceiveMessage: data, from: serviceId)
    }
}

// MARK: - Host Callback Handler

/// Handler for callbacks from XPC service to host
final class XPCHostCallbackHandler: NSObject, ActriumServiceXPCProtocol {
    weak var transport: XPCConnectionTransport?

    init(transport: XPCConnectionTransport) {
        self.transport = transport
        super.init()
    }

    func serviceDidBecomeReady(bundleId: String) {
        IPCLogger.shared.info(
            "Service reported ready",
            category: .connection,
            serviceId: bundleId
        )

        // Notify via IPCInteractionManager
        Task { @MainActor in
            IPCInteractionManager.shared.markServiceReady(bundleId)
        }
    }

    func sizeDidApply(tabId: String, width: Double, height: Double, requestId: Int) {
        IPCLogger.shared.debug(
            "Size applied: \(width)x\(height) requestId=\(requestId)",
            category: .response,
            metadata: ["tabId": tabId]
        )
    }

    func requestOpenURL(_ url: String, fromTab tabId: String) {
        IPCLogger.shared.info(
            "Open URL request: \(url)",
            category: .request,
            metadata: ["tabId": tabId, "url": url]
        )

        // Handle URL opening via app's URL handler
    }

    func reportError(_ error: IPCXPCServiceError) {
        IPCLogger.shared.logError(error.toIPCError(), context: "Service error")
    }

    func tabDidLoad(tabId: String, title: String?) {
        IPCLogger.shared.debug(
            "Tab loaded: \(title ?? "untitled")",
            category: .lifecycle,
            metadata: ["tabId": tabId]
        )
    }

    func tabRequestsFocus(tabId: String) {
        IPCLogger.shared.debug(
            "Tab requests focus",
            category: .request,
            metadata: ["tabId": tabId]
        )
    }

    func consoleLog(level: String, message: String, tabId: String?) {
        let logLevel: IPCLogLevel
        switch level.lowercased() {
        case "debug": logLevel = .debug
        case "info": logLevel = .info
        case "warn", "warning": logLevel = .warning
        case "error": logLevel = .error
        default: logLevel = .info
        }

        IPCLogger.shared.debug(
            "[Service Console] \(message)",
            category: .request,
            metadata: tabId.map { ["tabId": $0] } ?? [:]
        )
    }
}
