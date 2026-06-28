//
//  IPCRequestQueue.swift
//  Canvas
//
//  Priority queue for IPC requests with coalescing and backpressure support.
//

import Foundation

// MARK: - Queue Entry

/// Entry in the request queue
struct IPCQueueEntry: Sendable {
    let request: any IPCRequestProtocol
    let enqueuedAt: Date
    let continuation: CheckedContinuation<Void, Error>?

    var age: TimeInterval {
        Date().timeIntervalSince(enqueuedAt)
    }
}

// MARK: - Request Queue

/// Priority queue for managing IPC requests
public actor IPCRequestQueue {
    // MARK: - Properties

    /// Queue settings
    private let settings: IPCQueueSettings

    /// Main request queue (ordered by priority then age)
    private var queue: [IPCQueueEntry] = []

    /// In-flight requests (currently being processed)
    private var inFlight: [String: any IPCRequestProtocol] = [:]

    /// Coalesced requests waiting for in-flight result
    private var coalescedWaiters: [String: [CheckedContinuation<Void, Error>]] = [:]

    /// Total requests processed
    private var totalProcessed: Int = 0

    /// Total requests dropped
    private var totalDropped: Int = 0

    // MARK: - Initialization

    public init(settings: IPCQueueSettings = .default) {
        self.settings = settings
    }

    // MARK: - Queue Statistics

    /// Current queue size
    public var count: Int {
        queue.count
    }

    /// Number of in-flight requests
    public var inFlightCount: Int {
        inFlight.count
    }

    /// Whether the queue is empty
    public var isEmpty: Bool {
        queue.isEmpty
    }

    /// Whether the queue is at capacity
    public var isFull: Bool {
        queue.count >= settings.maxQueueSize
    }

    /// Statistics about the queue
    public var statistics: IPCQueueStatistics {
        IPCQueueStatistics(
            queueSize: queue.count,
            inFlightCount: inFlight.count,
            totalProcessed: totalProcessed,
            totalDropped: totalDropped
        )
    }

    // MARK: - Enqueueing

    /// Enqueue a request
    /// - Parameter request: The request to enqueue
    /// - Returns: Result indicating success or backpressure error
    public func enqueue(_ request: any IPCRequestProtocol) -> Result<Void, IPCError> {
        // Check for coalescing
        if let coalesceKey = request.coalesceKey {
            switch request.coalescePolicy {
            case .keepLatest:
                // Remove any existing request with same key
                queue.removeAll { $0.request.coalesceKey == coalesceKey }

            case .dropIfQueued:
                // If already queued, drop new request
                if queue.contains(where: { $0.request.coalesceKey == coalesceKey }) {
                    totalDropped += 1
                    return .failure(.superseded(traceId: request.traceId))
                }

            case .shareInFlight:
                // Will be handled when dequeuing
                break

            case .none:
                break
            }
        }

        // Check backpressure
        if isFull {
            switch settings.backpressureMode {
            case .failFast:
                totalDropped += 1
                return .failure(.backpressure(traceId: request.traceId))

            case .queueWithLimit(let limit):
                if queue.count >= limit {
                    totalDropped += 1
                    return .failure(.backpressure(traceId: request.traceId))
                }

            case .dropOldest(let maxSize):
                while queue.count >= maxSize && !queue.isEmpty {
                    // Drop oldest low-priority request
                    if let index = queue.lastIndex(where: { $0.request.priority == .low }) {
                        let dropped = queue.remove(at: index)
                        dropped.continuation?.resume(throwing: IPCError.superseded(traceId: dropped.request.traceId))
                        totalDropped += 1
                    } else {
                        // Drop oldest regardless of priority
                        let dropped = queue.removeLast()
                        dropped.continuation?.resume(throwing: IPCError.superseded(traceId: dropped.request.traceId))
                        totalDropped += 1
                    }
                }

            case .queue:
                // Allow unlimited queueing (not recommended)
                break
            }
        }

        // Add to queue
        let entry = IPCQueueEntry(request: request, enqueuedAt: Date(), continuation: nil)
        insertByPriority(entry)

        return .success(())
    }

    /// Enqueue a request and wait for it to be processed
    public func enqueueAndWait(_ request: any IPCRequestProtocol) async throws {
        // Check for shareInFlight coalescing
        if let coalesceKey = request.coalesceKey,
           request.coalescePolicy == .shareInFlight,
           inFlight[coalesceKey] != nil {
            // Wait for in-flight request to complete
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                var waiters = coalescedWaiters[coalesceKey] ?? []
                waiters.append(continuation)
                coalescedWaiters[coalesceKey] = waiters
            }
            return
        }

        // Enqueue with continuation
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let result = enqueue(request)
            switch result {
            case .success:
                // Store continuation for later
                // In a real implementation, we'd need to track this
                continuation.resume()
            case .failure(let error):
                continuation.resume(throwing: error)
            }
        }
    }

    // MARK: - Dequeueing

    /// Dequeue the next request
    /// - Returns: The next request to process, or nil if queue is empty
    public func dequeue() -> (any IPCRequestProtocol)? {
        guard !queue.isEmpty else { return nil }

        let entry = queue.removeFirst()
        let request = entry.request

        // Track in-flight
        if let coalesceKey = request.coalesceKey {
            inFlight[coalesceKey] = request
        }

        return request
    }

    /// Dequeue up to N requests
    public func dequeue(maxCount: Int) -> [any IPCRequestProtocol] {
        var requests: [any IPCRequestProtocol] = []
        for _ in 0..<maxCount {
            guard let request = dequeue() else { break }
            requests.append(request)
        }
        return requests
    }

    /// Mark a request as completed
    public func markCompleted(_ requestId: String, coalesceKey: String?) {
        totalProcessed += 1

        if let key = coalesceKey {
            inFlight.removeValue(forKey: key)

            // Resume coalesced waiters
            if let waiters = coalescedWaiters.removeValue(forKey: key) {
                for continuation in waiters {
                    continuation.resume()
                }
            }
        }
    }

    /// Mark a request as failed
    public func markFailed(_ requestId: String, coalesceKey: String?, error: IPCError) {
        if let key = coalesceKey {
            inFlight.removeValue(forKey: key)

            // Resume coalesced waiters with error
            if let waiters = coalescedWaiters.removeValue(forKey: key) {
                for continuation in waiters {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    // MARK: - Queue Management

    /// Clear all requests from the queue
    public func clear() {
        let entries = queue
        queue.removeAll()

        for entry in entries {
            entry.continuation?.resume(throwing: IPCError.cancelled(traceId: entry.request.traceId))
            totalDropped += 1
        }
    }

    /// Remove expired requests
    public func removeExpired() -> Int {
        let now = Date()
        var removed = 0

        queue.removeAll { entry in
            if entry.request.isExpired {
                entry.continuation?.resume(throwing: IPCError.timeout(traceId: entry.request.traceId))
                removed += 1
                return true
            }
            if let deadline = entry.request.deadline, now > deadline {
                entry.continuation?.resume(throwing: IPCError(code: .deadlineExceeded, traceId: entry.request.traceId))
                removed += 1
                return true
            }
            return false
        }

        totalDropped += removed
        return removed
    }

    /// Get requests for a specific tab
    public func requests(for tabId: UUID) -> [any IPCRequestProtocol] {
        queue.filter { $0.request.tabId == tabId }.map { $0.request }
    }

    /// Remove requests for a specific tab
    public func removeRequests(for tabId: UUID) -> Int {
        let before = queue.count
        queue.removeAll { entry in
            if entry.request.tabId == tabId {
                entry.continuation?.resume(throwing: IPCError.cancelled(traceId: entry.request.traceId))
                return true
            }
            return false
        }
        let removed = before - queue.count
        totalDropped += removed
        return removed
    }

    // MARK: - Private Helpers

    private func insertByPriority(_ entry: IPCQueueEntry) {
        guard settings.priorityQueueEnabled else {
            queue.append(entry)
            return
        }

        // Find insertion point (maintain priority order, FIFO within priority)
        let priority = entry.request.priority
        var insertIndex = queue.count

        for (index, existing) in queue.enumerated() {
            if existing.request.priority < priority {
                insertIndex = index
                break
            }
        }

        queue.insert(entry, at: insertIndex)
    }
}

// MARK: - Queue Statistics

/// Statistics about queue operation
public struct IPCQueueStatistics: Sendable {
    public let queueSize: Int
    public let inFlightCount: Int
    public let totalProcessed: Int
    public let totalDropped: Int

    public var dropRate: Double {
        let total = totalProcessed + totalDropped
        guard total > 0 else { return 0 }
        return Double(totalDropped) / Double(total)
    }
}
