//
//  MockTransport.swift
//  Canvas
//
//  Mock transport for testing IPC without real XPC connections.
//

import Foundation

// MARK: - Mock Transport

/// Mock implementation of IPCTransport for testing
public final class MockTransport: IPCTransport, @unchecked Sendable {
    // MARK: - Properties

    public let transportId: String
    public weak var delegate: IPCTransportDelegate?

    private let lock = NSLock()
    private var _state: IPCTransportState = .disconnected
    private var _connectedServices: Set<String> = []
    private var _sentMessages: [(serviceId: String, data: Data, timestamp: Date)] = []

    // MARK: - Test Configuration

    /// Simulated delay for operations
    public var simulatedDelay: TimeInterval = 0

    /// Whether to fail connection attempts
    public var shouldFailConnection: Bool = false

    /// Error message for failed connections
    public var connectionFailureMessage: String = "Simulated connection failure"

    /// Whether to fail send operations
    public var shouldFailSend: Bool = false

    /// Error message for failed sends
    public var sendFailureMessage: String = "Simulated send failure"

    /// Simulate connection timeout
    public var shouldTimeout: Bool = false

    // MARK: - Initialization

    public init(transportId: String = UUID().uuidString) {
        self.transportId = transportId
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
        if simulatedDelay > 0 {
            try await Task.sleep(nanoseconds: UInt64(simulatedDelay * 1_000_000_000))
        }

        if shouldTimeout {
            throw IPCTransportError.timeout
        }

        if shouldFailConnection {
            lock.lock()
            _state = .failed(connectionFailureMessage)
            lock.unlock()
            throw IPCTransportError.connectionFailed(connectionFailureMessage)
        }

        lock.lock()
        _state = .connecting
        lock.unlock()

        delegate?.transport(self, didChangeState: .connecting)

        // Simulate connection time
        if simulatedDelay > 0 {
            try await Task.sleep(nanoseconds: UInt64(simulatedDelay * 1_000_000_000))
        }

        lock.lock()
        _connectedServices.insert(serviceId)
        _state = .connected
        lock.unlock()

        delegate?.transport(self, didChangeState: .connected)
    }

    public func disconnect(from serviceId: String) async {
        lock.lock()
        _connectedServices.remove(serviceId)
        if _connectedServices.isEmpty {
            _state = .disconnected
        }
        lock.unlock()

        if _connectedServices.isEmpty {
            delegate?.transport(self, didChangeState: .disconnected)
        }
    }

    public func disconnectAll() async {
        lock.lock()
        _connectedServices.removeAll()
        _state = .disconnected
        lock.unlock()

        delegate?.transport(self, didChangeState: .disconnected)
    }

    public func send(_ data: Data, to serviceId: String) async throws {
        try await send(data, to: serviceId, timeout: 30.0)
    }

    public func send(_ data: Data, to serviceId: String, timeout: TimeInterval) async throws {
        lock.lock()
        let isConnected = _connectedServices.contains(serviceId)
        lock.unlock()

        guard isConnected else {
            throw IPCTransportError.notConnected
        }

        if simulatedDelay > 0 {
            try await Task.sleep(nanoseconds: UInt64(simulatedDelay * 1_000_000_000))
        }

        if shouldTimeout {
            throw IPCTransportError.timeout
        }

        if shouldFailSend {
            throw IPCTransportError.sendFailed(sendFailureMessage)
        }

        lock.lock()
        _sentMessages.append((serviceId: serviceId, data: data, timestamp: Date()))
        lock.unlock()
    }

    public func isConnected(to serviceId: String) async -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return _connectedServices.contains(serviceId)
    }

    public func connectedServices() async -> [String] {
        lock.lock()
        defer { lock.unlock() }
        return Array(_connectedServices)
    }

    public func invalidate() async {
        lock.lock()
        _connectedServices.removeAll()
        _state = .disconnected
        lock.unlock()

        delegate?.transportDidInvalidate(self, reason: "Invalidated")
    }

    // MARK: - Test Helpers

    /// Get all sent messages
    public var sentMessages: [(serviceId: String, data: Data, timestamp: Date)] {
        lock.lock()
        defer { lock.unlock() }
        return _sentMessages
    }

    /// Clear sent messages history
    public func clearSentMessages() {
        lock.lock()
        _sentMessages.removeAll()
        lock.unlock()
    }

    /// Simulate receiving a message
    public func simulateReceive(data: Data, from serviceId: String) {
        delegate?.transport(self, didReceiveMessage: data, from: serviceId)
    }

    /// Simulate connection interruption
    public func simulateInterruption() {
        lock.lock()
        _state = .interrupted
        lock.unlock()

        delegate?.transport(self, didChangeState: .interrupted)
        delegate?.transportDidInterrupt(self)
    }

    /// Simulate reconnection after interruption
    public func simulateReconnection() {
        lock.lock()
        _state = .connected
        lock.unlock()

        delegate?.transport(self, didChangeState: .connected)
    }

    /// Simulate invalidation
    public func simulateInvalidation(reason: String) {
        lock.lock()
        _connectedServices.removeAll()
        _state = .failed(reason)
        lock.unlock()

        delegate?.transport(self, didChangeState: .failed(reason))
        delegate?.transportDidInvalidate(self, reason: reason)
    }

    /// Reset mock state
    public func reset() {
        lock.lock()
        _state = .disconnected
        _connectedServices.removeAll()
        _sentMessages.removeAll()
        lock.unlock()

        shouldFailConnection = false
        shouldFailSend = false
        shouldTimeout = false
        simulatedDelay = 0
    }
}
