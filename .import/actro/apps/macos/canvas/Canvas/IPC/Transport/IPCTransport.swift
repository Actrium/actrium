//
//  IPCTransport.swift
//  Canvas
//
//  Transport layer protocol for IPC communication.
//  Abstracts the underlying transport mechanism (XPC, DistributedNotification, etc.)
//

import Foundation

// MARK: - Transport State

/// State of the transport layer
public enum IPCTransportState: Sendable, Equatable {
    case disconnected
    case connecting
    case connected
    case interrupted
    case failed(String)
}

// MARK: - Transport Delegate

/// Delegate protocol for receiving transport events
public protocol IPCTransportDelegate: AnyObject, Sendable {
    /// Called when transport state changes
    func transport(_ transport: any IPCTransport, didChangeState state: IPCTransportState)

    /// Called when a message is received
    func transport(_ transport: any IPCTransport, didReceiveMessage data: Data, from serviceId: String)

    /// Called when connection is interrupted
    func transportDidInterrupt(_ transport: any IPCTransport)

    /// Called when connection is invalidated
    func transportDidInvalidate(_ transport: any IPCTransport, reason: String)
}

// MARK: - Transport Protocol

/// Protocol for transport layer implementations
public protocol IPCTransport: AnyObject, Sendable {
    /// Unique identifier for this transport
    var transportId: String { get }

    /// Current state of the transport
    var state: IPCTransportState { get async }

    /// Delegate for receiving events
    var delegate: IPCTransportDelegate? { get set }

    /// Connect to a service
    func connect(to serviceId: String) async throws

    /// Disconnect from a service
    func disconnect(from serviceId: String) async

    /// Disconnect from all services
    func disconnectAll() async

    /// Send data to a service
    func send(_ data: Data, to serviceId: String) async throws

    /// Send data to a service with timeout
    func send(_ data: Data, to serviceId: String, timeout: TimeInterval) async throws

    /// Check if connected to a specific service
    func isConnected(to serviceId: String) async -> Bool

    /// Get list of connected service IDs
    func connectedServices() async -> [String]

    /// Invalidate the transport (permanent shutdown)
    func invalidate() async
}

// MARK: - Transport Factory

/// Factory for creating transport instances
public enum IPCTransportFactory {
    /// Create the default transport based on configuration
    public static func create(configuration: IPCConfiguration) -> any IPCTransport {
        return XPCConnectionTransport(configuration: configuration)
    }

    /// Create a mock transport for testing
    public static func createMock() -> MockTransport {
        MockTransport()
    }
}

// MARK: - Transport Error

/// Errors specific to transport layer
public enum IPCTransportError: Error, Sendable {
    case notConnected
    case connectionFailed(String)
    case sendFailed(String)
    case timeout
    case invalidated
}
