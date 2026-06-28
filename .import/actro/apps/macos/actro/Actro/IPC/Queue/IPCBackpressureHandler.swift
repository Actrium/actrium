//
//  IPCBackpressureHandler.swift
//  Actro
//
//  Handler for backpressure in IPC operations.
//  Manages flow control when the service is overloaded.
//

import Foundation

// MARK: - Backpressure State

/// Current state of backpressure handling
public enum IPCBackpressureState: Sendable, Equatable {
    /// Normal operation, accepting all requests
    case normal

    /// Under pressure, may delay or reject requests
    case pressured(level: Double)

    /// Overloaded, rejecting non-critical requests
    case overloaded

    /// Completely blocked, rejecting all requests
    case blocked

    public var description: String {
        switch self {
        case .normal:
            return "normal"
        case .pressured(let level):
            return "pressured(\(Int(level * 100))%)"
        case .overloaded:
            return "overloaded"
        case .blocked:
            return "blocked"
        }
    }

    /// Whether requests should be delayed
    public var shouldDelay: Bool {
        switch self {
        case .normal:
            return false
        case .pressured, .overloaded, .blocked:
            return true
        }
    }

    /// Whether non-critical requests should be rejected
    public var shouldRejectNonCritical: Bool {
        switch self {
        case .normal, .pressured:
            return false
        case .overloaded, .blocked:
            return true
        }
    }

    /// Whether all requests should be rejected
    public var shouldRejectAll: Bool {
        self == .blocked
    }
}

// MARK: - Backpressure Handler

/// Handler for managing backpressure
public actor IPCBackpressureHandler {
    // MARK: - Properties

    /// Current backpressure state
    private var _state: IPCBackpressureState = .normal

    /// Configuration
    private let config: IPCBackpressureConfig

    /// Queue size for calculating pressure
    private var currentQueueSize: Int = 0

    /// In-flight request count
    private var currentInFlight: Int = 0

    /// Recent response times for calculating latency
    private var recentLatencies: [TimeInterval] = []

    /// Maximum latencies to track
    private let maxLatencyHistory = 50

    /// State change callbacks
    private var stateChangeHandlers: [(IPCBackpressureState) -> Void] = []

    // MARK: - Initialization

    public init(config: IPCBackpressureConfig = .default) {
        self.config = config
    }

    // MARK: - State Access

    /// Current backpressure state
    public var state: IPCBackpressureState {
        _state
    }

    /// Check if a request can proceed
    public func canAccept(priority: IPCPriority) -> Bool {
        switch _state {
        case .normal:
            return true
        case .pressured:
            return true // Accept but may delay
        case .overloaded:
            return priority >= .high
        case .blocked:
            return priority == .critical
        }
    }

    /// Get suggested delay for a request
    public func suggestedDelay(priority: IPCPriority) -> TimeInterval {
        switch _state {
        case .normal:
            return 0
        case .pressured(let level):
            // Scale delay based on pressure level and priority
            let baseDelay = config.baseDelay * level
            let priorityFactor = 1.0 - (Double(priority.rawValue) / 200.0)
            return baseDelay * priorityFactor
        case .overloaded:
            return config.overloadedDelay
        case .blocked:
            return config.blockedDelay
        }
    }

    // MARK: - Metrics Updates

    /// Update queue size
    public func updateQueueSize(_ size: Int) {
        currentQueueSize = size
        recalculateState()
    }

    /// Update in-flight count
    public func updateInFlight(_ count: Int) {
        currentInFlight = count
        recalculateState()
    }

    /// Record a response latency
    public func recordLatency(_ latency: TimeInterval) {
        recentLatencies.append(latency)
        if recentLatencies.count > maxLatencyHistory {
            recentLatencies.removeFirst()
        }
        recalculateState()
    }

    /// Manually set the state (for testing or external signals)
    public func setState(_ state: IPCBackpressureState) {
        guard _state != state else { return }
        let oldState = _state
        _state = state
        notifyStateChange(from: oldState, to: state)
    }

    // MARK: - State Calculation

    private func recalculateState() {
        let newState = calculateState()
        guard newState != _state else { return }

        let oldState = _state
        _state = newState
        notifyStateChange(from: oldState, to: newState)
    }

    private func calculateState() -> IPCBackpressureState {
        // Calculate pressure from multiple factors

        // Queue pressure (0.0 - 1.0)
        let queuePressure = Double(currentQueueSize) / Double(config.maxQueueSize)

        // In-flight pressure (0.0 - 1.0)
        let inFlightPressure = Double(currentInFlight) / Double(config.maxInFlight)

        // Latency pressure (0.0 - 1.0)
        let averageLatency = recentLatencies.isEmpty ? 0 : recentLatencies.reduce(0, +) / Double(recentLatencies.count)
        let latencyPressure = min(1.0, averageLatency / config.latencyThreshold)

        // Combined pressure (weighted average)
        let combinedPressure = (queuePressure * 0.4) + (inFlightPressure * 0.4) + (latencyPressure * 0.2)

        // Determine state based on combined pressure
        if combinedPressure >= config.blockedThreshold {
            return .blocked
        } else if combinedPressure >= config.overloadedThreshold {
            return .overloaded
        } else if combinedPressure >= config.pressuredThreshold {
            return .pressured(level: combinedPressure)
        } else {
            return .normal
        }
    }

    // MARK: - Callbacks

    /// Register a handler for state changes
    public func onStateChange(_ handler: @escaping (IPCBackpressureState) -> Void) {
        stateChangeHandlers.append(handler)
    }

    private func notifyStateChange(from oldState: IPCBackpressureState, to newState: IPCBackpressureState) {
        for handler in stateChangeHandlers {
            handler(newState)
        }
    }

    // MARK: - Reset

    /// Reset all metrics
    public func reset() {
        currentQueueSize = 0
        currentInFlight = 0
        recentLatencies.removeAll()
        _state = .normal
    }
}

// MARK: - Configuration

/// Configuration for backpressure handling
public struct IPCBackpressureConfig: Sendable {
    /// Maximum queue size before blocking
    public let maxQueueSize: Int

    /// Maximum in-flight requests before blocking
    public let maxInFlight: Int

    /// Latency threshold for pressure calculation (seconds)
    public let latencyThreshold: TimeInterval

    /// Pressure level at which to start delaying
    public let pressuredThreshold: Double

    /// Pressure level at which to reject non-critical
    public let overloadedThreshold: Double

    /// Pressure level at which to block all
    public let blockedThreshold: Double

    /// Base delay when pressured (seconds)
    public let baseDelay: TimeInterval

    /// Delay when overloaded (seconds)
    public let overloadedDelay: TimeInterval

    /// Delay when blocked (seconds)
    public let blockedDelay: TimeInterval

    public init(
        maxQueueSize: Int = 100,
        maxInFlight: Int = 10,
        latencyThreshold: TimeInterval = 1.0,
        pressuredThreshold: Double = 0.5,
        overloadedThreshold: Double = 0.8,
        blockedThreshold: Double = 0.95,
        baseDelay: TimeInterval = 0.1,
        overloadedDelay: TimeInterval = 0.5,
        blockedDelay: TimeInterval = 2.0
    ) {
        self.maxQueueSize = maxQueueSize
        self.maxInFlight = maxInFlight
        self.latencyThreshold = latencyThreshold
        self.pressuredThreshold = pressuredThreshold
        self.overloadedThreshold = overloadedThreshold
        self.blockedThreshold = blockedThreshold
        self.baseDelay = baseDelay
        self.overloadedDelay = overloadedDelay
        self.blockedDelay = blockedDelay
    }

    public static let `default` = IPCBackpressureConfig()

    public static let aggressive = IPCBackpressureConfig(
        maxQueueSize: 50,
        maxInFlight: 5,
        latencyThreshold: 0.5,
        pressuredThreshold: 0.3,
        overloadedThreshold: 0.6,
        blockedThreshold: 0.9
    )

    public static let relaxed = IPCBackpressureConfig(
        maxQueueSize: 500,
        maxInFlight: 50,
        latencyThreshold: 5.0,
        pressuredThreshold: 0.7,
        overloadedThreshold: 0.9,
        blockedThreshold: 0.99
    )
}
