//
//  IPCError.swift
//  Actrium
//
//  Unified error types for IPC communication.
//

import Foundation

// MARK: - Error Code

/// IPC error codes representing all possible failure modes
public enum IPCErrorCode: Int, Sendable, CustomStringConvertible {
    // Connection errors (100-199)
    case notConnected = 100
    case interrupted = 101
    case invalidated = 102
    case connectionFailed = 103

    // Timeout errors (200-299)
    case timeout = 200
    case deadlineExceeded = 201

    // Cancellation errors (300-399)
    case cancelled = 300
    case superseded = 301

    // Capacity errors (400-499)
    case backpressure = 400
    case queueFull = 401
    case serviceUnavailable = 402

    // Serialization errors (500-599)
    case serializationError = 500
    case deserializationError = 501
    case payloadTooLarge = 502

    // Security errors (600-699)
    case authorizationFailed = 600
    case codeSignatureInvalid = 601

    // Protocol errors (700-799)
    case protocolError = 700
    case versionMismatch = 701
    case unknownMethod = 702

    // Internal errors (900-999)
    case internalError = 900
    case unknownError = 999

    public var description: String {
        switch self {
        case .notConnected: return "Not connected to XPC service"
        case .interrupted: return "XPC connection was interrupted"
        case .invalidated: return "XPC connection was invalidated"
        case .connectionFailed: return "Failed to establish XPC connection"
        case .timeout: return "Request timed out"
        case .deadlineExceeded: return "Request deadline exceeded"
        case .cancelled: return "Request was cancelled"
        case .superseded: return "Request was superseded by a newer request"
        case .backpressure: return "Backpressure limit reached"
        case .queueFull: return "Request queue is full"
        case .serviceUnavailable: return "XPC service is unavailable"
        case .serializationError: return "Failed to serialize request"
        case .deserializationError: return "Failed to deserialize response"
        case .payloadTooLarge: return "Payload exceeds size limit"
        case .authorizationFailed: return "Authorization failed"
        case .codeSignatureInvalid: return "Code signature validation failed"
        case .protocolError: return "Protocol error"
        case .versionMismatch: return "Protocol version mismatch"
        case .unknownMethod: return "Unknown protocol method"
        case .internalError: return "Internal error"
        case .unknownError: return "Unknown error"
        }
    }

    /// Whether this error indicates a retryable condition
    public var isRetryable: Bool {
        switch self {
        case .interrupted, .timeout, .backpressure, .serviceUnavailable:
            return true
        case .notConnected, .invalidated, .connectionFailed,
             .deadlineExceeded, .cancelled, .superseded,
             .queueFull, .serializationError, .deserializationError,
             .payloadTooLarge, .authorizationFailed, .codeSignatureInvalid,
             .protocolError, .versionMismatch, .unknownMethod,
             .internalError, .unknownError:
            return false
        }
    }

    /// Whether this error indicates a connection issue
    public var isConnectionError: Bool {
        switch self {
        case .notConnected, .interrupted, .invalidated, .connectionFailed:
            return true
        default:
            return false
        }
    }
}

// MARK: - IPCError

/// Unified error type for all IPC operations
public struct IPCError: Error, Sendable, CustomStringConvertible, LocalizedError {
    /// The error code
    public let code: IPCErrorCode

    /// Human-readable error message
    public let message: String

    /// Whether this error can be retried
    public let isRetryable: Bool

    /// Trace ID for correlating logs
    public let traceId: String?

    /// Service ID where the error occurred
    public let serviceId: String?

    /// Tab ID if the error is tab-specific
    public let tabId: UUID?

    /// Underlying error if any
    public let underlyingError: Error?

    /// Timestamp when the error occurred
    public let timestamp: Date

    public init(
        code: IPCErrorCode,
        message: String? = nil,
        isRetryable: Bool? = nil,
        traceId: String? = nil,
        serviceId: String? = nil,
        tabId: UUID? = nil,
        underlyingError: Error? = nil
    ) {
        self.code = code
        self.message = message ?? code.description
        self.isRetryable = isRetryable ?? code.isRetryable
        self.traceId = traceId
        self.serviceId = serviceId
        self.tabId = tabId
        self.underlyingError = underlyingError
        self.timestamp = Date()
    }

    public var description: String {
        var parts = ["[\(code.rawValue)] \(message)"]
        if let traceId = traceId {
            parts.append("trace=\(traceId)")
        }
        if let serviceId = serviceId {
            parts.append("service=\(serviceId)")
        }
        if let tabId = tabId {
            parts.append("tab=\(tabId.uuidString.prefix(8))")
        }
        return parts.joined(separator: " ")
    }

    public var errorDescription: String? {
        description
    }

    // MARK: - Convenience Factories

    public static func notConnected(serviceId: String? = nil, traceId: String? = nil) -> IPCError {
        IPCError(code: .notConnected, traceId: traceId, serviceId: serviceId)
    }

    public static func interrupted(serviceId: String? = nil, traceId: String? = nil) -> IPCError {
        IPCError(code: .interrupted, traceId: traceId, serviceId: serviceId)
    }

    public static func invalidated(serviceId: String? = nil, traceId: String? = nil) -> IPCError {
        IPCError(code: .invalidated, traceId: traceId, serviceId: serviceId)
    }

    public static func timeout(traceId: String? = nil) -> IPCError {
        IPCError(code: .timeout, traceId: traceId)
    }

    public static func cancelled(traceId: String? = nil) -> IPCError {
        IPCError(code: .cancelled, traceId: traceId)
    }

    public static func superseded(traceId: String? = nil) -> IPCError {
        IPCError(code: .superseded, traceId: traceId)
    }

    public static func backpressure(traceId: String? = nil) -> IPCError {
        IPCError(code: .backpressure, traceId: traceId)
    }

    public static func serviceUnavailable(serviceId: String, traceId: String? = nil) -> IPCError {
        IPCError(code: .serviceUnavailable, traceId: traceId, serviceId: serviceId)
    }

    public static func serializationError(_ error: Error, traceId: String? = nil) -> IPCError {
        IPCError(code: .serializationError, message: "Serialization failed: \(error.localizedDescription)", traceId: traceId, underlyingError: error)
    }

    public static func deserializationError(_ error: Error, traceId: String? = nil) -> IPCError {
        IPCError(code: .deserializationError, message: "Deserialization failed: \(error.localizedDescription)", traceId: traceId, underlyingError: error)
    }

    public static func internalError(_ message: String, traceId: String? = nil) -> IPCError {
        IPCError(code: .internalError, message: message, traceId: traceId)
    }
}

// MARK: - IPCError Extensions

extension IPCError: Equatable {
    public static func == (lhs: IPCError, rhs: IPCError) -> Bool {
        lhs.code == rhs.code &&
        lhs.message == rhs.message &&
        lhs.serviceId == rhs.serviceId &&
        lhs.tabId == rhs.tabId
    }
}

// MARK: - Result Extension

extension Result where Failure == IPCError {
    /// Whether the result is a retryable failure
    public var isRetryable: Bool {
        switch self {
        case .success:
            return false
        case .failure(let error):
            return error.isRetryable
        }
    }

    /// Convert to a throwing async pattern
    public func get() throws -> Success {
        switch self {
        case .success(let value):
            return value
        case .failure(let error):
            throw error
        }
    }
}
