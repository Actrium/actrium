//
//  IPCMetrics.swift
//  Actro
//
//  Metrics collection for IPC operations.
//

import Foundation

// MARK: - Metric Types

/// Types of metrics collected
public enum IPCMetricType: String, Sendable, CaseIterable {
    case requestSent = "request_sent"
    case responseReceived = "response_received"
    case requestFailed = "request_failed"
    case requestTimeout = "request_timeout"
    case requestCoalesced = "request_coalesced"
    case requestDropped = "request_dropped"
    case connectionEstablished = "connection_established"
    case connectionInterrupted = "connection_interrupted"
    case connectionInvalidated = "connection_invalidated"
    case backpressureTriggered = "backpressure_triggered"
    case queueOverflow = "queue_overflow"
}

// MARK: - Metric Sample

/// A single metric sample
public struct IPCMetricSample: Sendable {
    public let type: IPCMetricType
    public let value: Double
    public let timestamp: Date
    public let serviceId: String?
    public let tabId: UUID?
    public let labels: [String: String]

    public init(
        type: IPCMetricType,
        value: Double = 1.0,
        serviceId: String? = nil,
        tabId: UUID? = nil,
        labels: [String: String] = [:]
    ) {
        self.type = type
        self.value = value
        self.timestamp = Date()
        self.serviceId = serviceId
        self.tabId = tabId
        self.labels = labels
    }
}

// MARK: - Histogram Bucket

/// Histogram bucket for latency tracking
public struct IPCHistogramBucket: Sendable {
    public let upperBound: TimeInterval
    public var count: Int

    public init(upperBound: TimeInterval, count: Int = 0) {
        self.upperBound = upperBound
        self.count = count
    }
}

// MARK: - Latency Histogram

/// Histogram for tracking latency distribution
public struct IPCLatencyHistogram: Sendable {
    public var buckets: [IPCHistogramBucket]
    public var sum: TimeInterval
    public var count: Int

    public init() {
        // Default buckets: 1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 5s, 10s
        self.buckets = [
            IPCHistogramBucket(upperBound: 0.001),
            IPCHistogramBucket(upperBound: 0.005),
            IPCHistogramBucket(upperBound: 0.010),
            IPCHistogramBucket(upperBound: 0.025),
            IPCHistogramBucket(upperBound: 0.050),
            IPCHistogramBucket(upperBound: 0.100),
            IPCHistogramBucket(upperBound: 0.250),
            IPCHistogramBucket(upperBound: 0.500),
            IPCHistogramBucket(upperBound: 1.000),
            IPCHistogramBucket(upperBound: 5.000),
            IPCHistogramBucket(upperBound: 10.000),
            IPCHistogramBucket(upperBound: .infinity)
        ]
        self.sum = 0
        self.count = 0
    }

    public mutating func record(_ value: TimeInterval) {
        sum += value
        count += 1

        for i in 0..<buckets.count {
            if value <= buckets[i].upperBound {
                buckets[i].count += 1
                break
            }
        }
    }

    public var average: TimeInterval {
        count > 0 ? sum / Double(count) : 0
    }

    public func percentile(_ p: Double) -> TimeInterval {
        guard count > 0 else { return 0 }

        let targetCount = Int(Double(count) * p)
        var cumulative = 0

        for bucket in buckets {
            cumulative += bucket.count
            if cumulative >= targetCount {
                return bucket.upperBound
            }
        }

        return buckets.last?.upperBound ?? 0
    }
}

// MARK: - IPC Metrics

/// Metrics collector for IPC operations
public actor IPCMetrics {
    // MARK: - Singleton

    public static let shared = IPCMetrics()

    // MARK: - Properties

    /// Whether metrics collection is enabled
    private var isEnabled: Bool = true

    /// Counters by metric type
    private var counters: [IPCMetricType: Int] = [:]

    /// Latency histograms by operation
    private var latencyHistograms: [String: IPCLatencyHistogram] = [:]

    /// Recent samples buffer
    private var recentSamples: [IPCMetricSample] = []
    private let maxSamplesBuffer = 1000

    /// Metrics by service
    private var serviceMetrics: [String: ServiceMetrics] = [:]

    /// Last reset time
    private var lastResetTime: Date = Date()

    // MARK: - Initialization

    private init() {
        // Initialize counters
        for type in IPCMetricType.allCases {
            counters[type] = 0
        }
    }

    // MARK: - Configuration

    /// Enable or disable metrics collection
    public func setEnabled(_ enabled: Bool) {
        isEnabled = enabled
    }

    // MARK: - Counter Operations

    /// Increment a counter
    public func increment(_ type: IPCMetricType, serviceId: String? = nil, tabId: UUID? = nil) {
        guard isEnabled else { return }

        counters[type, default: 0] += 1

        if let serviceId = serviceId {
            var metrics = serviceMetrics[serviceId] ?? ServiceMetrics()
            metrics.increment(type)
            serviceMetrics[serviceId] = metrics
        }

        let sample = IPCMetricSample(type: type, serviceId: serviceId, tabId: tabId)
        addSample(sample)
    }

    /// Get counter value
    public func counter(_ type: IPCMetricType) -> Int {
        counters[type] ?? 0
    }

    // MARK: - Latency Operations

    /// Record a latency measurement
    public func recordLatency(
        _ latency: TimeInterval,
        operation: String,
        serviceId: String? = nil
    ) {
        guard isEnabled else { return }

        var histogram = latencyHistograms[operation] ?? IPCLatencyHistogram()
        histogram.record(latency)
        latencyHistograms[operation] = histogram

        if let serviceId = serviceId {
            var metrics = serviceMetrics[serviceId] ?? ServiceMetrics()
            metrics.recordLatency(latency, operation: operation)
            serviceMetrics[serviceId] = metrics
        }
    }

    /// Get latency histogram for an operation
    public func latencyHistogram(for operation: String) -> IPCLatencyHistogram? {
        latencyHistograms[operation]
    }

    // MARK: - Sample Access

    private func addSample(_ sample: IPCMetricSample) {
        recentSamples.append(sample)
        if recentSamples.count > maxSamplesBuffer {
            recentSamples.removeFirst(recentSamples.count - maxSamplesBuffer)
        }
    }

    /// Get recent samples
    public func getRecentSamples(count: Int = 100) -> [IPCMetricSample] {
        Array(recentSamples.suffix(count))
    }

    // MARK: - Aggregated Metrics

    /// Get all current metrics as a snapshot
    public func snapshot() -> IPCMetricsSnapshot {
        IPCMetricsSnapshot(
            counters: counters,
            latencyHistograms: latencyHistograms,
            serviceMetrics: serviceMetrics,
            collectionDuration: Date().timeIntervalSince(lastResetTime)
        )
    }

    /// Get metrics for a specific service
    public func metrics(for serviceId: String) -> ServiceMetrics? {
        serviceMetrics[serviceId]
    }

    // MARK: - Reset

    /// Reset all metrics
    public func reset() {
        for type in IPCMetricType.allCases {
            counters[type] = 0
        }
        latencyHistograms.removeAll()
        serviceMetrics.removeAll()
        recentSamples.removeAll()
        lastResetTime = Date()
    }
}

// MARK: - Service Metrics

/// Metrics for a specific service
public struct ServiceMetrics: Sendable {
    public var requestsSent: Int = 0
    public var responsesReceived: Int = 0
    public var requestsFailed: Int = 0
    public var connectionInterruptions: Int = 0
    private var latencies: [String: IPCLatencyHistogram] = [:]

    public init() {}

    public mutating func increment(_ type: IPCMetricType) {
        switch type {
        case .requestSent:
            requestsSent += 1
        case .responseReceived:
            responsesReceived += 1
        case .requestFailed, .requestTimeout:
            requestsFailed += 1
        case .connectionInterrupted:
            connectionInterruptions += 1
        default:
            break
        }
    }

    public mutating func recordLatency(_ latency: TimeInterval, operation: String) {
        var histogram = latencies[operation] ?? IPCLatencyHistogram()
        histogram.record(latency)
        latencies[operation] = histogram
    }

    public var successRate: Double {
        let total = requestsSent
        guard total > 0 else { return 1.0 }
        return Double(responsesReceived) / Double(total)
    }

    public func averageLatency(for operation: String) -> TimeInterval? {
        latencies[operation]?.average
    }
}

// MARK: - Metrics Snapshot

/// Snapshot of all metrics at a point in time
public struct IPCMetricsSnapshot: Sendable {
    public let counters: [IPCMetricType: Int]
    public let latencyHistograms: [String: IPCLatencyHistogram]
    public let serviceMetrics: [String: ServiceMetrics]
    public let collectionDuration: TimeInterval

    /// Total requests sent
    public var totalRequests: Int {
        counters[.requestSent] ?? 0
    }

    /// Total failures
    public var totalFailures: Int {
        (counters[.requestFailed] ?? 0) + (counters[.requestTimeout] ?? 0)
    }

    /// Overall success rate
    public var successRate: Double {
        let total = totalRequests
        guard total > 0 else { return 1.0 }
        return Double((counters[.responseReceived] ?? 0)) / Double(total)
    }

    /// Requests per second
    public var requestsPerSecond: Double {
        guard collectionDuration > 0 else { return 0 }
        return Double(totalRequests) / collectionDuration
    }
}

// MARK: - Trace Context

/// Context for distributed tracing
public struct IPCTraceContext: Sendable {
    public let traceId: String
    public let spanId: String
    public let parentSpanId: String?
    public let startTime: Date
    public let serviceName: String

    public init(
        traceId: String = UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(32).lowercased(),
        spanId: String = UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(16).lowercased(),
        parentSpanId: String? = nil,
        serviceName: String = "actro-host"
    ) {
        self.traceId = String(traceId)
        self.spanId = String(spanId)
        self.parentSpanId = parentSpanId
        self.startTime = Date()
        self.serviceName = serviceName
    }

    /// Create a child span
    public func createChildSpan() -> IPCTraceContext {
        IPCTraceContext(
            traceId: traceId,
            parentSpanId: spanId,
            serviceName: serviceName
        )
    }

    /// Duration since start
    public var duration: TimeInterval {
        Date().timeIntervalSince(startTime)
    }
}
