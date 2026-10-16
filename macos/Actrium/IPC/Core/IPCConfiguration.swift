//
//  IPCConfiguration.swift
//  Actrium
//
//  Configuration options for IPC subsystem.
//

import Foundation

// MARK: - Backpressure Mode

/// Mode for handling backpressure when queue is full
public enum IPCBackpressureMode: Sendable {
    /// Queue requests indefinitely (may cause memory issues)
    case queue

    /// Fail immediately when queue is full
    case failFast

    /// Queue up to a limit, then fail
    case queueWithLimit(Int)

    /// Drop oldest requests when queue is full
    case dropOldest(maxSize: Int)
}

// MARK: - Connection Settings

/// Configuration for XPC connections
public struct IPCConnectionSettings: Sendable {
    /// Connection timeout duration
    public let connectionTimeout: TimeInterval

    /// Heartbeat interval for connection health checks
    public let heartbeatInterval: TimeInterval

    /// Number of missed heartbeats before considering connection dead
    public let missedHeartbeatsThreshold: Int

    /// Whether to automatically reconnect on interruption
    public let autoReconnect: Bool

    /// Delay before attempting reconnection
    public let reconnectDelay: TimeInterval

    /// Maximum reconnection attempts
    public let maxReconnectAttempts: Int

    public init(
        connectionTimeout: TimeInterval = 10.0,
        heartbeatInterval: TimeInterval = 5.0,
        missedHeartbeatsThreshold: Int = 3,
        autoReconnect: Bool = true,
        reconnectDelay: TimeInterval = 0.5,
        maxReconnectAttempts: Int = 5
    ) {
        self.connectionTimeout = connectionTimeout
        self.heartbeatInterval = heartbeatInterval
        self.missedHeartbeatsThreshold = missedHeartbeatsThreshold
        self.autoReconnect = autoReconnect
        self.reconnectDelay = reconnectDelay
        self.maxReconnectAttempts = maxReconnectAttempts
    }

    public static let `default` = IPCConnectionSettings()

    public static let aggressive = IPCConnectionSettings(
        connectionTimeout: 5.0,
        heartbeatInterval: 2.0,
        missedHeartbeatsThreshold: 2,
        reconnectDelay: 0.2,
        maxReconnectAttempts: 10
    )

    public static let relaxed = IPCConnectionSettings(
        connectionTimeout: 30.0,
        heartbeatInterval: 15.0,
        missedHeartbeatsThreshold: 5,
        reconnectDelay: 2.0,
        maxReconnectAttempts: 3
    )
}

// MARK: - Queue Settings

/// Configuration for request queue
public struct IPCQueueSettings: Sendable {
    /// Maximum queue size
    public let maxQueueSize: Int

    /// Backpressure handling mode
    public let backpressureMode: IPCBackpressureMode

    /// Default coalesce window for grouping requests
    public let coalesceWindow: TimeInterval

    /// Whether to process high-priority requests first
    public let priorityQueueEnabled: Bool

    public init(
        maxQueueSize: Int = 100,
        backpressureMode: IPCBackpressureMode = .queueWithLimit(100),
        coalesceWindow: TimeInterval = 0.025, // 25ms default
        priorityQueueEnabled: Bool = true
    ) {
        self.maxQueueSize = maxQueueSize
        self.backpressureMode = backpressureMode
        self.coalesceWindow = coalesceWindow
        self.priorityQueueEnabled = priorityQueueEnabled
    }

    public static let `default` = IPCQueueSettings()

    public static let highThroughput = IPCQueueSettings(
        maxQueueSize: 500,
        backpressureMode: .dropOldest(maxSize: 500),
        coalesceWindow: 0.01,
        priorityQueueEnabled: true
    )

    public static let lowLatency = IPCQueueSettings(
        maxQueueSize: 20,
        backpressureMode: .failFast,
        coalesceWindow: 0.005,
        priorityQueueEnabled: true
    )
}

// MARK: - Logging Settings

/// Configuration for IPC logging
public struct IPCLoggingSettings: Sendable {
    /// Log level for IPC operations
    public let logLevel: IPCLogLevel

    /// Whether to include payload in logs (may contain sensitive data)
    public let includePayload: Bool

    /// Maximum payload size to log (truncated if larger)
    public let maxPayloadLogSize: Int

    /// Whether to log timing information
    public let logTiming: Bool

    /// Whether to log state transitions
    public let logStateTransitions: Bool

    public init(
        logLevel: IPCLogLevel = .info,
        includePayload: Bool = false,
        maxPayloadLogSize: Int = 256,
        logTiming: Bool = true,
        logStateTransitions: Bool = true
    ) {
        self.logLevel = logLevel
        self.includePayload = includePayload
        self.maxPayloadLogSize = maxPayloadLogSize
        self.logTiming = logTiming
        self.logStateTransitions = logStateTransitions
    }

    public static let `default` = IPCLoggingSettings()

    public static let verbose = IPCLoggingSettings(
        logLevel: .debug,
        includePayload: true,
        maxPayloadLogSize: 1024,
        logTiming: true,
        logStateTransitions: true
    )

    public static let minimal = IPCLoggingSettings(
        logLevel: .warning,
        includePayload: false,
        logTiming: false,
        logStateTransitions: false
    )
}

/// Log levels for IPC operations
public enum IPCLogLevel: Int, Sendable, Comparable {
    case debug = 0
    case info = 1
    case warning = 2
    case error = 3
    case critical = 4
    case off = 5

    public static func < (lhs: IPCLogLevel, rhs: IPCLogLevel) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

// MARK: - IPCConfiguration

/// Main configuration for the IPC subsystem
public struct IPCConfiguration: Sendable {
    /// Connection settings
    public let connection: IPCConnectionSettings

    /// Queue settings
    public let queue: IPCQueueSettings

    /// Logging settings
    public let logging: IPCLoggingSettings

    /// Default request timeout
    public let defaultTimeout: TimeInterval

    /// Whether IPC is enabled (can be disabled for testing)
    public let isEnabled: Bool

    /// Host process ID for isolation
    public let hostProcessId: Int32

    /// Feature flags
    public let featureFlags: IPCFeatureFlags

    public init(
        connection: IPCConnectionSettings = .default,
        queue: IPCQueueSettings = .default,
        logging: IPCLoggingSettings = .default,
        defaultTimeout: TimeInterval = 30.0,
        isEnabled: Bool = true,
        hostProcessId: Int32 = ProcessInfo.processInfo.processIdentifier,
        featureFlags: IPCFeatureFlags = .default
    ) {
        self.connection = connection
        self.queue = queue
        self.logging = logging
        self.defaultTimeout = defaultTimeout
        self.isEnabled = isEnabled
        self.hostProcessId = hostProcessId
        self.featureFlags = featureFlags
    }

    public static let `default` = IPCConfiguration()

    public static let testing = IPCConfiguration(
        connection: .aggressive,
        queue: .lowLatency,
        logging: .verbose,
        defaultTimeout: 5.0,
        featureFlags: .testing
    )

    public static let production = IPCConfiguration(
        connection: .default,
        queue: .default,
        logging: .minimal,
        defaultTimeout: 30.0,
        featureFlags: .default
    )
}

// MARK: - Feature Flags

/// Feature flags for IPC subsystem
public struct IPCFeatureFlags: Sendable {
    /// Enable request coalescing
    public let enableCoalescing: Bool

    /// Enable backpressure handling
    public let enableBackpressure: Bool

    /// Enable metrics collection
    public let enableMetrics: Bool

    /// Enable distributed tracing
    public let enableTracing: Bool

    public init(
        enableCoalescing: Bool = true,
        enableBackpressure: Bool = true,
        enableMetrics: Bool = true,
        enableTracing: Bool = true
    ) {
        self.enableCoalescing = enableCoalescing
        self.enableBackpressure = enableBackpressure
        self.enableMetrics = enableMetrics
        self.enableTracing = enableTracing
    }

    public static let `default` = IPCFeatureFlags()

    public static let testing = IPCFeatureFlags(
        enableCoalescing: true,
        enableBackpressure: true,
        enableMetrics: true,
        enableTracing: true
    )
}
