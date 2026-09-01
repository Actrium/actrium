//
//  IPCRequest.swift
//  Actro
//
//  Request model for IPC communication.
//

import Foundation

// MARK: - Request Kind

/// The type of IPC request
public enum IPCRequestKind: String, Sendable, Codable {
    /// Read-only query that doesn't modify state
    case read
    /// Write operation that modifies state
    case write
    /// Control operation (lifecycle, connection management)
    case control
}

// MARK: - Priority

/// Priority levels for IPC requests
public enum IPCPriority: Int, Sendable, Comparable, Codable {
    case low = 0
    case normal = 50
    case high = 100
    case critical = 200

    public static func < (lhs: IPCPriority, rhs: IPCPriority) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

// MARK: - Coalesce Policy

/// Policy for coalescing multiple requests with the same key
public enum IPCCoalescePolicy: Sendable, Codable {
    /// Replace queued requests with the same key, keeping only the latest
    /// Use for: size changes, configuration updates
    case keepLatest

    /// Share the result of an in-flight request with subsequent requests
    /// Use for: read-only queries
    case shareInFlight

    /// Drop new requests if one with the same key is already queued
    /// Use for: debounced operations
    case dropIfQueued

    /// No coalescing - each request is processed independently
    case none
}

// MARK: - Retry Policy

/// Policy for retrying failed requests
public struct IPCRetryPolicy: Sendable, Codable {
    /// Maximum number of retry attempts
    public let maxAttempts: Int

    /// Base delay between retries (multiplied by backoff factor)
    public let baseDelay: TimeInterval

    /// Backoff multiplier applied to delay after each retry
    public let backoffFactor: Double

    /// Maximum delay between retries
    public let maxDelay: TimeInterval

    /// Error codes that should trigger a retry
    public let retryableCodes: Set<Int>

    public init(
        maxAttempts: Int = 3,
        baseDelay: TimeInterval = 0.1,
        backoffFactor: Double = 2.0,
        maxDelay: TimeInterval = 5.0,
        retryableCodes: Set<Int>? = nil
    ) {
        self.maxAttempts = maxAttempts
        self.baseDelay = baseDelay
        self.backoffFactor = backoffFactor
        self.maxDelay = maxDelay
        self.retryableCodes = retryableCodes ?? Set([
            IPCErrorCode.interrupted.rawValue,
            IPCErrorCode.timeout.rawValue,
            IPCErrorCode.backpressure.rawValue,
            IPCErrorCode.serviceUnavailable.rawValue
        ])
    }

    /// Calculate delay for a given attempt number (0-indexed)
    public func delay(forAttempt attempt: Int) -> TimeInterval {
        let delay = baseDelay * pow(backoffFactor, Double(attempt))
        return min(delay, maxDelay)
    }

    /// Check if an error code should trigger a retry
    public func shouldRetry(errorCode: IPCErrorCode, attempt: Int) -> Bool {
        attempt < maxAttempts && retryableCodes.contains(errorCode.rawValue)
    }

    // MARK: - Presets

    public static let none = IPCRetryPolicy(maxAttempts: 0)
    public static let `default` = IPCRetryPolicy()
    public static let aggressive = IPCRetryPolicy(maxAttempts: 5, baseDelay: 0.05, backoffFactor: 1.5)
    public static let patient = IPCRetryPolicy(maxAttempts: 3, baseDelay: 0.5, backoffFactor: 2.0, maxDelay: 10.0)
}

// MARK: - Cancellation Token

/// Token for cancelling in-flight requests
public actor IPCCancellationToken {
    private var isCancelledFlag = false
    private var cancellationHandlers: [() -> Void] = []

    public init() {}

    public var isCancelled: Bool {
        isCancelledFlag
    }

    public func cancel() {
        guard !isCancelledFlag else { return }
        isCancelledFlag = true
        for handler in cancellationHandlers {
            handler()
        }
        cancellationHandlers.removeAll()
    }

    public func onCancel(_ handler: @escaping () -> Void) {
        if isCancelledFlag {
            handler()
        } else {
            cancellationHandlers.append(handler)
        }
    }
}

// MARK: - Request Protocol

/// Protocol for type-erased request access
public protocol IPCRequestProtocol: Sendable {
    var id: String { get }
    var kind: IPCRequestKind { get }
    var targetServiceId: String { get }
    var tabId: UUID? { get }
    var coalesceKey: String? { get }
    var coalescePolicy: IPCCoalescePolicy { get }
    var idempotencyKey: String? { get }
    var timeout: TimeInterval { get }
    var deadline: Date? { get }
    var priority: IPCPriority { get }
    var retryPolicy: IPCRetryPolicy { get }
    var traceId: String { get }
    var createdAt: Date { get }
    var isExpired: Bool { get }
}

// MARK: - IPCRequest

/// Generic request type for IPC operations
public struct IPCRequest<Payload: Codable>: IPCRequestProtocol, @unchecked Sendable {
    /// Unique request identifier
    public let id: String

    /// Type of request (read/write/control)
    public let kind: IPCRequestKind

    /// Request payload
    public let payload: Payload

    /// Target XPC service identifier
    public let targetServiceId: String

    /// Associated tab ID (if tab-specific)
    public let tabId: UUID?

    // MARK: - Coalescing

    /// Key for coalescing multiple requests
    public let coalesceKey: String?

    /// Policy for how to coalesce requests
    public let coalescePolicy: IPCCoalescePolicy

    /// Key for idempotency checking
    public let idempotencyKey: String?

    // MARK: - Timing

    /// Request timeout duration
    public let timeout: TimeInterval

    /// Absolute deadline for the request
    public let deadline: Date?

    /// Request priority
    public let priority: IPCPriority

    // MARK: - Retry

    /// Retry policy for failed requests
    public let retryPolicy: IPCRetryPolicy

    // MARK: - Tracing

    /// Trace ID for distributed tracing
    public let traceId: String

    /// Timestamp when request was created
    public let createdAt: Date

    public init(
        id: String = UUID().uuidString,
        kind: IPCRequestKind,
        payload: Payload,
        targetServiceId: String,
        tabId: UUID? = nil,
        coalesceKey: String? = nil,
        coalescePolicy: IPCCoalescePolicy = .none,
        idempotencyKey: String? = nil,
        timeout: TimeInterval = 30.0,
        deadline: Date? = nil,
        priority: IPCPriority = .normal,
        retryPolicy: IPCRetryPolicy = .default,
        traceId: String = UUID().uuidString.prefix(8).lowercased()
    ) {
        self.id = id
        self.kind = kind
        self.payload = payload
        self.targetServiceId = targetServiceId
        self.tabId = tabId
        self.coalesceKey = coalesceKey
        self.coalescePolicy = coalescePolicy
        self.idempotencyKey = idempotencyKey
        self.timeout = timeout
        self.deadline = deadline
        self.priority = priority
        self.retryPolicy = retryPolicy
        self.traceId = traceId
        self.createdAt = Date()
    }

    /// Create a new request with updated payload
    public func with(payload: Payload) -> IPCRequest<Payload> {
        IPCRequest(
            id: UUID().uuidString, // New ID for new request
            kind: kind,
            payload: payload,
            targetServiceId: targetServiceId,
            tabId: tabId,
            coalesceKey: coalesceKey,
            coalescePolicy: coalescePolicy,
            idempotencyKey: idempotencyKey,
            timeout: timeout,
            deadline: deadline,
            priority: priority,
            retryPolicy: retryPolicy,
            traceId: traceId // Keep same trace ID
        )
    }

    /// Check if request has exceeded its deadline
    public var isExpired: Bool {
        if let deadline = deadline {
            return Date() > deadline
        }
        return false
    }

    /// Remaining time until timeout
    public var remainingTime: TimeInterval {
        if let deadline = deadline {
            return max(0, deadline.timeIntervalSinceNow)
        }
        let elapsed = Date().timeIntervalSince(createdAt)
        return max(0, timeout - elapsed)
    }
}

// MARK: - Common Request Payloads

/// Empty payload for requests that don't need data
public struct EmptyPayload: Codable, @unchecked Sendable {
    public init() {}
}

/// Ping request payload
public struct PingPayload: Codable, @unchecked Sendable {
    public let timestamp: Date

    public init(timestamp: Date = Date()) {
        self.timestamp = timestamp
    }
}

/// Container size change payload
public struct ContainerSizePayload: Codable, @unchecked Sendable {
    public let tabId: UUID
    public let width: CGFloat
    public let height: CGFloat
    public let isOverviewMode: Bool
    public let requestId: Int

    public init(tabId: UUID, width: CGFloat, height: CGFloat, isOverviewMode: Bool, requestId: Int = 0) {
        self.tabId = tabId
        self.width = width
        self.height = height
        self.isOverviewMode = isOverviewMode
        self.requestId = requestId
    }

    public var size: CGSize {
        CGSize(width: width, height: height)
    }
}

/// Tab binding payload
public struct TabBindingPayload: Codable, @unchecked Sendable {
    public let hostTabId: UUID
    public let internalId: String

    public init(hostTabId: UUID, internalId: String) {
        self.hostTabId = hostTabId
        self.internalId = internalId
    }
}

// MARK: - Request Builders

/// Builder for creating common request types
public enum IPCRequestBuilder {
    /// Create a ping request
    public static func ping(
        serviceId: String,
        timeout: TimeInterval = 5.0
    ) -> IPCRequest<PingPayload> {
        IPCRequest(
            kind: .control,
            payload: PingPayload(),
            targetServiceId: serviceId,
            timeout: timeout,
            priority: .high,
            retryPolicy: .none
        )
    }

    /// Create a container size change request
    public static func containerSize(
        tabId: UUID,
        size: CGSize,
        isOverviewMode: Bool,
        serviceId: String,
        requestId: Int = 0
    ) -> IPCRequest<ContainerSizePayload> {
        IPCRequest(
            kind: .write,
            payload: ContainerSizePayload(
                tabId: tabId,
                width: size.width,
                height: size.height,
                isOverviewMode: isOverviewMode,
                requestId: requestId
            ),
            targetServiceId: serviceId,
            tabId: tabId,
            coalesceKey: "size-\(tabId.uuidString)",
            coalescePolicy: .keepLatest,
            priority: .high
        )
    }

    /// Create a tab binding request
    public static func bindTab(
        hostTabId: UUID,
        internalId: String,
        serviceId: String
    ) -> IPCRequest<TabBindingPayload> {
        IPCRequest(
            kind: .control,
            payload: TabBindingPayload(hostTabId: hostTabId, internalId: internalId),
            targetServiceId: serviceId,
            tabId: hostTabId,
            priority: .high,
            retryPolicy: .aggressive
        )
    }
}
