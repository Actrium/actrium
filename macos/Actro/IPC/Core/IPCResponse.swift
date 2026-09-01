//
//  IPCResponse.swift
//  Actro
//
//  Response model for IPC communication.
//

import Foundation

// MARK: - Response Status

/// Status of an IPC response
public enum IPCResponseStatus: String, Sendable, Codable {
    case success
    case error
    case cancelled
    case timeout
}

// MARK: - IPCResponse

/// Generic response type for IPC operations
public struct IPCResponse<Payload: Codable>: @unchecked Sendable {
    /// Response status
    public let status: IPCResponseStatus

    /// Response payload (nil if error)
    public let payload: Payload?

    /// Error if status is error
    public let error: IPCError?

    /// Original request ID
    public let requestId: String

    /// Trace ID for distributed tracing
    public let traceId: String

    /// Timestamp when response was created
    public let createdAt: Date

    /// Round-trip duration from request creation
    public let duration: TimeInterval?

    public init(
        status: IPCResponseStatus,
        payload: Payload? = nil,
        error: IPCError? = nil,
        requestId: String,
        traceId: String,
        duration: TimeInterval? = nil
    ) {
        self.status = status
        self.payload = payload
        self.error = error
        self.requestId = requestId
        self.traceId = traceId
        self.createdAt = Date()
        self.duration = duration
    }

    /// Whether the response indicates success
    public var isSuccess: Bool {
        status == .success && payload != nil
    }

    /// Whether the response indicates failure
    public var isError: Bool {
        status == .error || error != nil
    }

    /// Convert to Result type
    public func toResult() -> Result<Payload, IPCError> {
        if let payload = payload, status == .success {
            return .success(payload)
        } else if let error = error {
            return .failure(error)
        } else {
            return .failure(IPCError(code: .unknownError, message: "No payload or error in response", traceId: traceId))
        }
    }

    // MARK: - Factory Methods

    /// Create a success response
    public static func success(
        payload: Payload,
        requestId: String,
        traceId: String,
        duration: TimeInterval? = nil
    ) -> IPCResponse<Payload> {
        IPCResponse(
            status: .success,
            payload: payload,
            requestId: requestId,
            traceId: traceId,
            duration: duration
        )
    }

    /// Create an error response
    public static func error(
        _ error: IPCError,
        requestId: String,
        traceId: String,
        duration: TimeInterval? = nil
    ) -> IPCResponse<Payload> {
        IPCResponse(
            status: .error,
            error: error,
            requestId: requestId,
            traceId: traceId,
            duration: duration
        )
    }

    /// Create a cancelled response
    public static func cancelled(
        requestId: String,
        traceId: String,
        duration: TimeInterval? = nil
    ) -> IPCResponse<Payload> {
        IPCResponse(
            status: .cancelled,
            error: IPCError.cancelled(traceId: traceId),
            requestId: requestId,
            traceId: traceId,
            duration: duration
        )
    }

    /// Create a timeout response
    public static func timeout(
        requestId: String,
        traceId: String,
        duration: TimeInterval? = nil
    ) -> IPCResponse<Payload> {
        IPCResponse(
            status: .timeout,
            error: IPCError.timeout(traceId: traceId),
            requestId: requestId,
            traceId: traceId,
            duration: duration
        )
    }
}

// MARK: - Void Response

/// Typealias for responses that don't return data
public typealias IPCVoidResponse = IPCResponse<EmptyPayload>

extension IPCResponse where Payload == EmptyPayload {
    /// Create a success response for void operations
    public static func success(
        requestId: String,
        traceId: String,
        duration: TimeInterval? = nil
    ) -> IPCVoidResponse {
        IPCResponse(
            status: .success,
            payload: EmptyPayload(),
            requestId: requestId,
            traceId: traceId,
            duration: duration
        )
    }
}

// MARK: - Pong Response

/// Response payload for ping requests
public struct PongPayload: Codable, @unchecked Sendable {
    public let timestamp: Date
    public let serviceId: String
    public let processId: Int32

    public init(timestamp: Date = Date(), serviceId: String, processId: Int32) {
        self.timestamp = timestamp
        self.serviceId = serviceId
        self.processId = processId
    }
}

/// Typealias for ping responses
public typealias IPCPongResponse = IPCResponse<PongPayload>

// MARK: - Size Applied Response

/// Response payload for size change requests
public struct SizeAppliedPayload: Codable, @unchecked Sendable {
    public let tabId: UUID
    public let appliedWidth: CGFloat
    public let appliedHeight: CGFloat
    public let requestId: Int

    public init(tabId: UUID, appliedSize: CGSize, requestId: Int) {
        self.tabId = tabId
        self.appliedWidth = appliedSize.width
        self.appliedHeight = appliedSize.height
        self.requestId = requestId
    }

    public var appliedSize: CGSize {
        CGSize(width: appliedWidth, height: appliedHeight)
    }
}

/// Typealias for size change responses
public typealias IPCSizeAppliedResponse = IPCResponse<SizeAppliedPayload>
