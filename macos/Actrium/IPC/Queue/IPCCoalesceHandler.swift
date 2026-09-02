//
//  IPCCoalesceHandler.swift
//  Actrium
//
//  Handler for coalescing multiple requests with the same key.
//

import Foundation

// MARK: - Coalesce Result

/// Result of attempting to coalesce a request
public enum IPCCoalesceResult<T: Sendable>: Sendable {
    /// Request should proceed normally
    case proceed

    /// Request was coalesced, use this result
    case coalesced(T)

    /// Request was dropped (superseded by another)
    case dropped

    /// Request should wait for in-flight result
    case waitForInFlight(key: String)
}

// MARK: - Pending Request

/// A pending request waiting for coalescing
public struct CoalescePendingRequest: Sendable {
    public let requestId: String
    public let coalesceKey: String
    public let payload: Data
    public let enqueuedAt: Date

    public init(requestId: String, coalesceKey: String, payload: Data, enqueuedAt: Date = Date()) {
        self.requestId = requestId
        self.coalesceKey = coalesceKey
        self.payload = payload
        self.enqueuedAt = enqueuedAt
    }
}

// MARK: - Coalesce Handler

/// Handler for coalescing requests
public actor IPCCoalesceHandler {
    // MARK: - Properties

    /// Window for grouping coalesced requests
    private let coalesceWindow: TimeInterval

    /// Pending requests by coalesce key
    private var pendingByKey: [String: [CoalescePendingRequest]] = [:]

    /// In-flight requests by coalesce key
    private var inFlightByKey: [String: String] = [:] // key -> requestId

    /// Continuation waiters for in-flight results
    private var waiters: [String: [CheckedContinuation<Data, Error>]] = [:]

    /// Timer for flushing pending requests
    private var flushTask: Task<Void, Never>?

    // MARK: - Callbacks

    /// Called when a batch of coalesced requests should be processed
    private var onBatchReady: (([CoalescePendingRequest]) async -> Void)?

    /// Set the batch ready callback
    public func setOnBatchReady(_ callback: (([CoalescePendingRequest]) async -> Void)?) {
        onBatchReady = callback
    }

    // MARK: - Initialization

    public init(coalesceWindow: TimeInterval = 0.025) {
        self.coalesceWindow = coalesceWindow
    }

    // MARK: - Request Handling

    /// Submit a request for potential coalescing
    /// - Parameters:
    ///   - requestId: Unique request ID
    ///   - coalesceKey: Key for grouping related requests
    ///   - policy: Coalescing policy
    ///   - payload: Request payload data
    /// - Returns: Coalesce result indicating how to proceed
    public func submit(
        requestId: String,
        coalesceKey: String,
        policy: IPCCoalescePolicy,
        payload: Data
    ) -> IPCCoalesceResult<Data> {
        switch policy {
        case .none:
            return .proceed

        case .keepLatest:
            // Replace any pending requests with same key
            pendingByKey[coalesceKey] = [
                CoalescePendingRequest(
                    requestId: requestId,
                    coalesceKey: coalesceKey,
                    payload: payload,
                    enqueuedAt: Date()
                )
            ]
            scheduleFlush()
            return .proceed

        case .dropIfQueued:
            // Drop if already queued or in-flight
            if pendingByKey[coalesceKey] != nil || inFlightByKey[coalesceKey] != nil {
                return .dropped
            }
            pendingByKey[coalesceKey] = [
                CoalescePendingRequest(
                    requestId: requestId,
                    coalesceKey: coalesceKey,
                    payload: payload,
                    enqueuedAt: Date()
                )
            ]
            scheduleFlush()
            return .proceed

        case .shareInFlight:
            // If already in-flight, wait for result
            if inFlightByKey[coalesceKey] != nil {
                return .waitForInFlight(key: coalesceKey)
            }
            // Otherwise proceed and mark as in-flight
            inFlightByKey[coalesceKey] = requestId
            return .proceed
        }
    }

    /// Wait for an in-flight request result
    public func waitForInFlight(key: String) async throws -> Data {
        try await withCheckedThrowingContinuation { continuation in
            var keyWaiters = waiters[key] ?? []
            keyWaiters.append(continuation)
            waiters[key] = keyWaiters
        }
    }

    /// Mark an in-flight request as completed
    public func markCompleted(key: String, result: Data) {
        inFlightByKey.removeValue(forKey: key)

        // Resume all waiters with result
        if let keyWaiters = waiters.removeValue(forKey: key) {
            for continuation in keyWaiters {
                continuation.resume(returning: result)
            }
        }
    }

    /// Mark an in-flight request as failed
    public func markFailed(key: String, error: Error) {
        inFlightByKey.removeValue(forKey: key)

        // Resume all waiters with error
        if let keyWaiters = waiters.removeValue(forKey: key) {
            for continuation in keyWaiters {
                continuation.resume(throwing: error)
            }
        }
    }

    // MARK: - Batch Processing

    /// Get and remove pending requests for a key
    public func consumePending(key: String) -> [CoalescePendingRequest] {
        pendingByKey.removeValue(forKey: key) ?? []
    }

    /// Flush all pending requests
    public func flushAll() {
        for (_, requests) in pendingByKey {
            Task {
                await onBatchReady?(requests)
            }
        }
        pendingByKey.removeAll()
    }

    // MARK: - Private

    private func scheduleFlush() {
        // Cancel existing flush task
        flushTask?.cancel()

        // Schedule new flush after coalesce window
        flushTask = Task {
            try? await Task.sleep(nanoseconds: UInt64(coalesceWindow * 1_000_000_000))

            guard !Task.isCancelled else { return }

            // Flush all pending requests
            let pending = pendingByKey
            pendingByKey.removeAll()

            for (_, requests) in pending {
                if let lastRequest = requests.last {
                    // For keepLatest, only process the last request
                    await onBatchReady?([lastRequest])
                }
            }
        }
    }
}

// MARK: - Coalesce Key Generator

/// Utility for generating coalesce keys
public enum IPCCoalesceKeyGenerator {
    /// Generate a coalesce key for size changes
    public static func sizeChange(tabId: UUID) -> String {
        "size-\(tabId.uuidString)"
    }

    /// Generate a coalesce key for config updates
    public static func configUpdate(tabId: UUID) -> String {
        "config-\(tabId.uuidString)"
    }

    /// Generate a coalesce key for mode changes
    public static func modeChange() -> String {
        "mode-change"
    }

    /// Generate a coalesce key for a specific operation
    public static func operation(_ name: String, tabId: UUID? = nil) -> String {
        if let tabId = tabId {
            return "\(name)-\(tabId.uuidString)"
        }
        return name
    }
}
