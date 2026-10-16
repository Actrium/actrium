//
//  IPCConnectionStateMachine.swift
//  Actrium
//
//  State machine for managing IPC connection lifecycle.
//
//  States:
//    idle → connecting → ready ⟷ interrupted → invalidated
//                   ↓                              ↑
//                disabled ←────────────────────────┘
//

import Foundation
import Combine

// MARK: - Connection State

/// Possible states for an IPC connection
public enum IPCConnectionState: Sendable, Equatable, CustomStringConvertible {
    /// Initial state, no connection attempt made
    case idle

    /// Connection is being established
    case connecting

    /// Connection is established and ready for use
    case ready

    /// Connection was interrupted (temporary, may recover)
    case interrupted

    /// Connection was invalidated (permanent, needs reset)
    case invalidated(reason: String)

    /// Connection is disabled (user/system requested)
    case disabled

    public var description: String {
        switch self {
        case .idle: return "idle"
        case .connecting: return "connecting"
        case .ready: return "ready"
        case .interrupted: return "interrupted"
        case .invalidated(let reason): return "invalidated(\(reason))"
        case .disabled: return "disabled"
        }
    }

    /// Whether the connection can accept requests
    public var canAcceptRequests: Bool {
        self == .ready
    }

    /// Whether the connection should queue requests
    public var shouldQueueRequests: Bool {
        switch self {
        case .connecting, .interrupted:
            return true
        default:
            return false
        }
    }

    /// Whether the connection is in a terminal state
    public var isTerminal: Bool {
        switch self {
        case .invalidated, .disabled:
            return true
        default:
            return false
        }
    }
}

// MARK: - Connection Event

/// Events that can trigger state transitions
public enum IPCConnectionEvent: Sendable, CustomStringConvertible {
    /// Request to connect
    case connect

    /// Connection was successfully established
    case connectionEstablished

    /// Connection was interrupted (may recover)
    case connectionInterruption

    /// Connection was invalidated (permanent failure)
    case connectionInvalidation(reason: String)

    /// Service is not available
    case serviceNotAvailable

    /// Request to disable the connection
    case disable

    /// Request to enable the connection
    case enable

    /// Request to reset (from invalidated state)
    case reset

    public var description: String {
        switch self {
        case .connect: return "connect"
        case .connectionEstablished: return "connectionEstablished"
        case .connectionInterruption: return "connectionInterruption"
        case .connectionInvalidation(let reason): return "connectionInvalidation(\(reason))"
        case .serviceNotAvailable: return "serviceNotAvailable"
        case .disable: return "disable"
        case .enable: return "enable"
        case .reset: return "reset"
        }
    }
}

// MARK: - Transition Result

/// Result of a state transition
public struct IPCStateTransition: Sendable {
    public let from: IPCConnectionState
    public let to: IPCConnectionState
    public let event: IPCConnectionEvent
    public let timestamp: Date

    public init(from: IPCConnectionState, to: IPCConnectionState, event: IPCConnectionEvent) {
        self.from = from
        self.to = to
        self.event = event
        self.timestamp = Date()
    }

    public var wasSuccessful: Bool {
        from != to
    }
}

// MARK: - State Machine

/// Thread-safe state machine for IPC connection management
public actor IPCConnectionStateMachine {
    // MARK: - Properties

    private var _state: IPCConnectionState = .idle
    private let serviceId: String
    private var transitionHistory: [IPCStateTransition] = []
    private let maxHistorySize: Int
    private var stateChangeHandlers: [(IPCStateTransition) -> Void] = []

    /// Current connection state
    public var state: IPCConnectionState {
        _state
    }

    // MARK: - Initialization

    public init(serviceId: String, maxHistorySize: Int = 50) {
        self.serviceId = serviceId
        self.maxHistorySize = maxHistorySize
    }

    // MARK: - State Queries

    /// Whether the connection can accept requests
    public var canAcceptRequests: Bool {
        _state.canAcceptRequests
    }

    /// Whether requests should be queued
    public var shouldQueueRequests: Bool {
        _state.shouldQueueRequests
    }

    /// Whether the connection is in a terminal state
    public var isTerminal: Bool {
        _state.isTerminal
    }

    // MARK: - State Transitions

    /// Process an event and transition to appropriate state
    @discardableResult
    public func process(_ event: IPCConnectionEvent) -> IPCStateTransition {
        let previousState = _state
        let newState = nextState(for: event)

        if newState != previousState {
            _state = newState

            let transition = IPCStateTransition(from: previousState, to: newState, event: event)
            recordTransition(transition)
            notifyHandlers(transition)

            return transition
        }

        return IPCStateTransition(from: previousState, to: previousState, event: event)
    }

    /// Calculate next state based on current state and event
    private func nextState(for event: IPCConnectionEvent) -> IPCConnectionState {
        switch (_state, event) {
        // From idle
        case (.idle, .connect):
            return .connecting
        case (.idle, .disable):
            return .disabled

        // From connecting
        case (.connecting, .connectionEstablished):
            return .ready
        case (.connecting, .connectionInterruption):
            return .interrupted
        case (.connecting, .connectionInvalidation(let reason)):
            return .invalidated(reason: reason)
        case (.connecting, .serviceNotAvailable):
            return .invalidated(reason: "Service not available")
        case (.connecting, .disable):
            return .disabled

        // From ready
        case (.ready, .connectionInterruption):
            return .interrupted
        case (.ready, .connectionInvalidation(let reason)):
            return .invalidated(reason: reason)
        case (.ready, .disable):
            return .disabled

        // From interrupted
        case (.interrupted, .connectionEstablished):
            return .ready
        case (.interrupted, .connectionInvalidation(let reason)):
            return .invalidated(reason: reason)
        case (.interrupted, .connect):
            return .connecting
        case (.interrupted, .disable):
            return .disabled

        // From invalidated
        case (.invalidated, .reset):
            return .idle
        case (.invalidated, .disable):
            return .disabled

        // From disabled
        case (.disabled, .enable):
            return .idle
        case (.disabled, .reset):
            return .idle

        // Invalid transitions (no-op)
        default:
            return _state
        }
    }

    // MARK: - History

    private func recordTransition(_ transition: IPCStateTransition) {
        transitionHistory.append(transition)
        if transitionHistory.count > maxHistorySize {
            transitionHistory.removeFirst()
        }
    }

    /// Get recent transition history
    public func getHistory(limit: Int = 10) -> [IPCStateTransition] {
        Array(transitionHistory.suffix(limit))
    }

    /// Clear transition history
    public func clearHistory() {
        transitionHistory.removeAll()
    }

    // MARK: - Handlers

    /// Register a handler to be called on state changes
    public func onStateChange(_ handler: @escaping (IPCStateTransition) -> Void) {
        stateChangeHandlers.append(handler)
    }

    private func notifyHandlers(_ transition: IPCStateTransition) {
        for handler in stateChangeHandlers {
            handler(transition)
        }
    }

    // MARK: - Convenience Methods

    /// Attempt to connect (from idle or interrupted)
    public func connect() -> IPCStateTransition {
        process(.connect)
    }

    /// Mark connection as established
    public func markReady() -> IPCStateTransition {
        process(.connectionEstablished)
    }

    /// Mark connection as interrupted
    public func markInterrupted() -> IPCStateTransition {
        process(.connectionInterruption)
    }

    /// Mark connection as invalidated
    public func markInvalidated(reason: String) -> IPCStateTransition {
        process(.connectionInvalidation(reason: reason))
    }

    /// Disable the connection
    public func disable() -> IPCStateTransition {
        process(.disable)
    }

    /// Enable the connection
    public func enable() -> IPCStateTransition {
        process(.enable)
    }

    /// Reset the connection (from invalidated)
    public func reset() -> IPCStateTransition {
        process(.reset)
    }
}

// MARK: - Combine Support

/// Publisher for connection state changes
public class IPCConnectionStatePublisher: ObservableObject {
    @Published public private(set) var state: IPCConnectionState = .idle

    private let stateMachine: IPCConnectionStateMachine

    public init(stateMachine: IPCConnectionStateMachine) {
        self.stateMachine = stateMachine

        Task {
            await stateMachine.onStateChange { [weak self] transition in
                Task { @MainActor in
                    self?.state = transition.to
                }
            }
        }
    }

    /// Get current state synchronously
    public func getCurrentState() async -> IPCConnectionState {
        await stateMachine.state
    }
}
