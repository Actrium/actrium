//
//  IPCLogger.swift
//  Actrium
//
//  Structured logging for IPC operations.
//

import Foundation
import os.log

// MARK: - Log Entry

/// A structured log entry for IPC operations
public struct IPCLogEntry: Sendable {
    public let timestamp: Date
    public let level: IPCLogLevel
    public let category: IPCLogCategory
    public let message: String
    public let traceId: String?
    public let serviceId: String?
    public let tabId: UUID?
    public let metadata: [String: String]
    public let duration: TimeInterval?

    public init(
        level: IPCLogLevel,
        category: IPCLogCategory,
        message: String,
        traceId: String? = nil,
        serviceId: String? = nil,
        tabId: UUID? = nil,
        metadata: [String: String] = [:],
        duration: TimeInterval? = nil
    ) {
        self.timestamp = Date()
        self.level = level
        self.category = category
        self.message = message
        self.traceId = traceId
        self.serviceId = serviceId
        self.tabId = tabId
        self.metadata = metadata
        self.duration = duration
    }

    /// Format as a log string
    public var formatted: String {
        var parts: [String] = []

        // Timestamp
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        parts.append(formatter.string(from: timestamp))

        // Level
        parts.append("[\(level)]")

        // Category
        parts.append("[\(category.rawValue)]")

        // Trace ID
        if let traceId = traceId {
            parts.append("trace=\(traceId)")
        }

        // Service ID
        if let serviceId = serviceId {
            let shortService = serviceId.components(separatedBy: ".").last ?? serviceId
            parts.append("service=\(shortService)")
        }

        // Tab ID
        if let tabId = tabId {
            parts.append("tab=\(tabId.uuidString.prefix(8))")
        }

        // Duration
        if let duration = duration {
            parts.append("duration=\(String(format: "%.2fms", duration * 1000))")
        }

        // Message
        parts.append(message)

        // Metadata
        if !metadata.isEmpty {
            let metadataStr = metadata.map { "\($0)=\($1)" }.joined(separator: " ")
            parts.append("[\(metadataStr)]")
        }

        return parts.joined(separator: " ")
    }
}

// MARK: - Log Category

/// Categories for IPC log entries
public enum IPCLogCategory: String, Sendable {
    case connection = "conn"
    case request = "req"
    case response = "res"
    case queue = "queue"
    case coalesce = "coal"
    case backpressure = "bp"
    case stateTransition = "state"
    case error = "err"
    case metrics = "metrics"
    case lifecycle = "life"
}

// MARK: - IPC Logger

/// Main logger for IPC operations
public final class IPCLogger: @unchecked Sendable {
    // MARK: - Singleton

    public static let shared = IPCLogger()

    // MARK: - Properties

    private let osLog: OSLog
    private var settings: IPCLoggingSettings
    private let lock = NSLock()
    private var entryBuffer: [IPCLogEntry] = []
    private let maxBufferSize: Int = 100

    /// External log handler (for forwarding to other systems)
    public var externalHandler: ((IPCLogEntry) -> Void)?

    // MARK: - Initialization

    private init() {
        self.osLog = OSLog(subsystem: "io.actrium.actro", category: "IPC")
        self.settings = .default
    }

    // MARK: - Configuration

    /// Update logging settings
    public func configure(settings: IPCLoggingSettings) {
        lock.lock()
        self.settings = settings
        lock.unlock()
    }

    // MARK: - Logging Methods

    /// Log a debug message
    public func debug(
        _ message: String,
        category: IPCLogCategory = .request,
        traceId: String? = nil,
        serviceId: String? = nil,
        tabId: UUID? = nil,
        metadata: [String: String] = [:],
        duration: TimeInterval? = nil
    ) {
        log(level: .debug, message: message, category: category,
            traceId: traceId, serviceId: serviceId, tabId: tabId,
            metadata: metadata, duration: duration)
    }

    /// Log an info message
    public func info(
        _ message: String,
        category: IPCLogCategory = .request,
        traceId: String? = nil,
        serviceId: String? = nil,
        tabId: UUID? = nil,
        metadata: [String: String] = [:],
        duration: TimeInterval? = nil
    ) {
        log(level: .info, message: message, category: category,
            traceId: traceId, serviceId: serviceId, tabId: tabId,
            metadata: metadata, duration: duration)
    }

    /// Log a warning message
    public func warning(
        _ message: String,
        category: IPCLogCategory = .error,
        traceId: String? = nil,
        serviceId: String? = nil,
        tabId: UUID? = nil,
        metadata: [String: String] = [:],
        duration: TimeInterval? = nil
    ) {
        log(level: .warning, message: message, category: category,
            traceId: traceId, serviceId: serviceId, tabId: tabId,
            metadata: metadata, duration: duration)
    }

    /// Log an error message
    public func error(
        _ message: String,
        category: IPCLogCategory = .error,
        traceId: String? = nil,
        serviceId: String? = nil,
        tabId: UUID? = nil,
        metadata: [String: String] = [:],
        duration: TimeInterval? = nil
    ) {
        log(level: .error, message: message, category: category,
            traceId: traceId, serviceId: serviceId, tabId: tabId,
            metadata: metadata, duration: duration)
    }

    /// Log a critical message
    public func critical(
        _ message: String,
        category: IPCLogCategory = .error,
        traceId: String? = nil,
        serviceId: String? = nil,
        tabId: UUID? = nil,
        metadata: [String: String] = [:],
        duration: TimeInterval? = nil
    ) {
        log(level: .critical, message: message, category: category,
            traceId: traceId, serviceId: serviceId, tabId: tabId,
            metadata: metadata, duration: duration)
    }

    // MARK: - Request/Response Logging

    /// Log a request being sent
    public func logRequest(
        _ request: any IPCRequestProtocol,
        serviceId: String
    ) {
        guard settings.logLevel <= .debug else { return }

        var metadata: [String: String] = [
            "kind": request.kind.rawValue,
            "priority": String(request.priority.rawValue)
        ]

        if let coalesceKey = request.coalesceKey {
            metadata["coalesceKey"] = coalesceKey
        }

        debug(
            "Sending request \(request.id.prefix(8))",
            category: .request,
            traceId: request.traceId,
            serviceId: serviceId,
            tabId: request.tabId,
            metadata: metadata
        )
    }

    /// Log a response received
    public func logResponse<T>(
        _ response: IPCResponse<T>,
        duration: TimeInterval
    ) {
        guard settings.logLevel <= .info else { return }

        if settings.logTiming {
            info(
                "Received response \(response.requestId.prefix(8)) status=\(response.status.rawValue)",
                category: .response,
                traceId: response.traceId,
                duration: duration
            )
        }
    }

    /// Log an error
    public func logError(
        _ error: IPCError,
        context: String? = nil
    ) {
        var message = error.message
        if let context = context {
            message = "\(context): \(message)"
        }

        self.error(
            message,
            category: .error,
            traceId: error.traceId,
            serviceId: error.serviceId,
            tabId: error.tabId
        )
    }

    // MARK: - State Transition Logging

    /// Log a state transition
    public func logStateTransition(
        _ transition: IPCStateTransition,
        serviceId: String
    ) {
        guard settings.logStateTransitions else { return }

        info(
            "State: \(transition.from) → \(transition.to) (event: \(transition.event))",
            category: .stateTransition,
            serviceId: serviceId
        )
    }

    // MARK: - Internal

    private func log(
        level: IPCLogLevel,
        message: String,
        category: IPCLogCategory,
        traceId: String?,
        serviceId: String?,
        tabId: UUID?,
        metadata: [String: String],
        duration: TimeInterval?
    ) {
        lock.lock()
        let currentSettings = settings
        lock.unlock()

        guard level >= currentSettings.logLevel else { return }

        let entry = IPCLogEntry(
            level: level,
            category: category,
            message: message,
            traceId: traceId,
            serviceId: serviceId,
            tabId: tabId,
            metadata: metadata,
            duration: duration
        )

        // Write to OS Log
        let osLogType: OSLogType
        switch level {
        case .debug: osLogType = .debug
        case .info: osLogType = .info
        case .warning: osLogType = .default
        case .error: osLogType = .error
        case .critical: osLogType = .fault
        case .off: return
        }

        os_log("%{public}@", log: osLog, type: osLogType, entry.formatted)

        // Buffer entry
        lock.lock()
        entryBuffer.append(entry)
        if entryBuffer.count > maxBufferSize {
            entryBuffer.removeFirst()
        }
        lock.unlock()

        // External handler
        externalHandler?(entry)
    }

    // MARK: - History

    /// Get recent log entries
    public func recentEntries(count: Int = 50) -> [IPCLogEntry] {
        lock.lock()
        defer { lock.unlock() }
        return Array(entryBuffer.suffix(count))
    }

    /// Clear log buffer
    public func clearBuffer() {
        lock.lock()
        entryBuffer.removeAll()
        lock.unlock()
    }
}

// MARK: - Convenience Extensions

extension IPCLogger {
    /// Log emoji prefix for NSLog compatibility
    private func logEmoji(for level: IPCLogLevel) -> String {
        switch level {
        case .debug: return "🔍"
        case .info: return "📡"
        case .warning: return "⚠️"
        case .error: return "❌"
        case .critical: return "🚨"
        case .off: return ""
        }
    }
}
