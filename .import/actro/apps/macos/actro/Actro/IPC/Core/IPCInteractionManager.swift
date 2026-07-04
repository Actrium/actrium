//
//  IPCInteractionManager.swift
//  Actro
//
//  Main coordinator for all IPC operations.
//  This is the primary entry point for the IPC subsystem.
//

import Foundation
import Combine

// MARK: - IPC Interaction Manager

/// Main coordinator for IPC operations
/// This actor manages connections, request queuing, coalescing, and observability.
@MainActor
public final class IPCInteractionManager: ObservableObject {
    // MARK: - Singleton

    public static let shared = IPCInteractionManager()

    // MARK: - Published State

    /// Whether the manager is enabled
    @Published public private(set) var isEnabled: Bool = true

    /// Current overview mode state
    @Published public private(set) var isOverviewMode: Bool = false

    /// Connected service IDs
    @Published public private(set) var connectedServices: Set<String> = []

    // MARK: - Configuration

    private var configuration: IPCConfiguration

    // MARK: - Core Components

    /// Transport layer
    private var transport: any IPCTransport

    /// Request queue
    private let requestQueue: IPCRequestQueue

    /// Coalesce handler
    private let coalesceHandler: IPCCoalesceHandler

    /// Backpressure handler
    private let backpressureHandler: IPCBackpressureHandler

    /// Connection state machines by service ID
    private var connectionStates: [String: IPCConnectionStateMachine] = [:]

    // MARK: - Tab State

    /// Tab states (mirrors XPCBridgeService for compatibility)
    private var tabStates: [UUID: IPCTabState] = [:]

    /// Pending requests by tab
    private var pendingByTab: [UUID: [PendingIPCRequest]] = [:]

    // MARK: - Subscriptions

    private var cancellables = Set<AnyCancellable>()

    // MARK: - Service State Listening

    /// Observer for XPC service state notifications
    private var serviceStateObserver: NSObjectProtocol?

    /// Pending requests queue (when state machine not ready)
    private var pendingRequestsByService: [String: [PendingIPCRequest]] = [:]

    // MARK: - Request Tracking

    /// In-flight request count
    private var inFlightCount: Int = 0

    /// Request ID counter
    private var requestIdCounter: Int = 0

    // MARK: - Initialization

    private init(
        configuration: IPCConfiguration = .default,
        transport: (any IPCTransport)? = nil
    ) {
        self.configuration = configuration
        self.transport = transport ?? IPCTransportFactory.create(configuration: configuration)
        self.requestQueue = IPCRequestQueue(settings: configuration.queue)
        self.coalesceHandler = IPCCoalesceHandler(coalesceWindow: configuration.queue.coalesceWindow)
        self.backpressureHandler = IPCBackpressureHandler()

        setupObservability()
        setupServiceStateListener()
    }

    // MARK: - Configuration

    /// Update configuration
    public func configure(_ configuration: IPCConfiguration) {
        self.configuration = configuration
        IPCLogger.shared.configure(settings: configuration.logging)
    }

    /// Set enabled state
    public func setEnabled(_ enabled: Bool) {
        isEnabled = enabled

        if !enabled {
            // Disable all connections
            Task {
                for (serviceId, stateMachine) in connectionStates {
                    await stateMachine.disable()
                }
            }
        }

        IPCLogger.shared.info(
            "IPC Manager \(enabled ? "enabled" : "disabled")",
            category: .lifecycle
        )
    }

    // MARK: - Overview Mode

    /// Update overview mode state
    public func setOverviewMode(_ isOverview: Bool) {
        guard isOverviewMode != isOverview else { return }

        isOverviewMode = isOverview

        IPCLogger.shared.info(
            "Overview mode: \(isOverview)",
            category: .lifecycle
        )

        // Notify all ready tabs
        for (tabId, state) in tabStates where state.isReady {
            guard let serviceId = state.serviceId, let lastSize = state.lastSentSize else { continue }

            Task {
                await sendContainerSizeChangedInternal(
                    tabId: tabId,
                    size: lastSize,
                    serviceId: serviceId,
                    requestId: nextRequestId()
                )
            }
        }
    }

    // MARK: - Service Lifecycle

    /// Allocate a service for a tab
    public func allocateService(for tabId: UUID, fingerprint: String) throws -> String {
        // Delegate to XPCServicePool for actual allocation
        let serviceId = try XPCServicePool.shared.allocateService(for: tabId, fingerprint: fingerprint)

        // Initialize tab state
        var state = tabStates[tabId] ?? IPCTabState()
        state.serviceId = serviceId
        tabStates[tabId] = state

        // Ensure connection state machine exists
        if connectionStates[serviceId] == nil {
            connectionStates[serviceId] = IPCConnectionStateMachine(serviceId: serviceId)
        }

        IPCLogger.shared.info(
            "Allocated service for tab",
            category: .lifecycle,
            serviceId: serviceId,
            tabId: tabId
        )

        return serviceId
    }

    /// Mark a tab as connected
    public func markTabConnected(_ tabId: UUID, connectionId: String? = nil) {
        guard var state = tabStates[tabId] else { return }

        state.isConnected = true
        state.connectionId = connectionId
        tabStates[tabId] = state

        IPCLogger.shared.info(
            "Tab connected",
            category: .connection,
            serviceId: state.serviceId,
            tabId: tabId
        )

        // Flush pending requests
        flushPendingRequests(for: tabId)
    }

    /// Mark service as ready
    public func markServiceReady(_ serviceId: String) {
        // Update all tabs using this service
        for (tabId, var state) in tabStates where state.serviceId == serviceId {
            state.isServiceReady = true
            tabStates[tabId] = state

            if state.isReady {
                connectedServices.insert(serviceId)
                flushPendingRequests(for: tabId)
            }
        }

        // Update connection state machine
        Task {
            if let stateMachine = connectionStates[serviceId] {
                await stateMachine.markReady()
            }
        }

        IPCLogger.shared.info(
            "Service ready",
            category: .connection,
            serviceId: serviceId
        )

        Task {
            await IPCMetrics.shared.increment(.connectionEstablished, serviceId: serviceId)
        }
    }

    /// Release a tab's resources
    public func releaseTab(_ tabId: UUID) {
        guard let state = tabStates[tabId] else { return }

        // Release from pool
        if state.serviceId != nil {
            XPCServicePool.shared.releaseService(for: tabId)
        }

        // Clean up state
        tabStates.removeValue(forKey: tabId)
        pendingByTab.removeValue(forKey: tabId)

        IPCLogger.shared.info(
            "Tab released",
            category: .lifecycle,
            serviceId: state.serviceId,
            tabId: tabId
        )
    }

    // MARK: - Request Execution

    /// Send tab configuration
    public func sendTabConfig(_ config: IPCXPCTabConfig) {
        guard let tabId = UUID(uuidString: config.tabId) else { return }

        guard let state = tabStates[tabId], state.isReady, let serviceId = state.serviceId else {
            // Queue for later
            let pending = PendingIPCRequest.tabConfig(config)
            appendPending(tabId: tabId, request: pending)

            IPCLogger.shared.debug(
                "Tab config deferred (not ready)",
                category: .request,
                tabId: tabId
            )
            return
        }

        Task {
            await sendTabConfigInternal(config, serviceId: serviceId)
        }
    }

    /// Send container size change
    public func sendContainerSizeChanged(tabId: UUID, size: CGSize, requestId: Int? = nil) {
        guard let state = tabStates[tabId], state.isReady, let serviceId = state.serviceId else {
            // Queue for later
            let id = requestId ?? nextRequestId()
            let pending = PendingIPCRequest.sizeChange(size, id)
            appendPending(tabId: tabId, request: pending)
            return
        }

        // Skip if same size
        if let lastSize = state.lastSentSize, lastSize == size {
            return
        }

        // Update state
        var updatedState = state
        updatedState.lastSentSize = size
        tabStates[tabId] = updatedState

        let id = requestId ?? nextRequestId()

        Task {
            await sendContainerSizeChangedInternal(
                tabId: tabId,
                size: size,
                serviceId: serviceId,
                requestId: id
            )
        }
    }

    /// Perform a generic request
    public func perform<P: Codable, R: Codable>(
        _ request: IPCRequest<P>,
        responseType: R.Type
    ) async -> Result<R, IPCError> {
        guard isEnabled else {
            return .failure(.internalError("IPC Manager is disabled"))
        }

        // Check backpressure
        let canAccept = await backpressureHandler.canAccept(priority: request.priority)
        guard canAccept else {
            await IPCMetrics.shared.increment(.backpressureTriggered, serviceId: request.targetServiceId)
            return .failure(.backpressure(traceId: request.traceId))
        }

        // Apply suggested delay
        let delay = await backpressureHandler.suggestedDelay(priority: request.priority)
        if delay > 0 {
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
        }

        // Log request
        IPCLogger.shared.logRequest(request, serviceId: request.targetServiceId)

        let startTime = Date()

        // Try to send
        do {
            // Serialize payload
            let encoder = JSONEncoder()
            let data = try encoder.encode(request.payload)

            // Send via transport
            try await transport.send(data, to: request.targetServiceId, timeout: request.timeout)

            // For now, assume success for write operations
            // In a full implementation, we'd wait for the reply
            await IPCMetrics.shared.increment(.requestSent, serviceId: request.targetServiceId)
            await IPCMetrics.shared.increment(.responseReceived, serviceId: request.targetServiceId)

            let duration = Date().timeIntervalSince(startTime)
            await IPCMetrics.shared.recordLatency(duration, operation: request.kind.rawValue, serviceId: request.targetServiceId)

            // Return empty response for now
            // A real implementation would decode the actual response
            if let emptyResponse = EmptyPayload() as? R {
                return .success(emptyResponse)
            } else {
                return .failure(.deserializationError(NSError(domain: "IPC", code: -1, userInfo: [NSLocalizedDescriptionKey: "Response type mismatch"]), traceId: request.traceId))
            }
        } catch let error as IPCError {
            await IPCMetrics.shared.increment(.requestFailed, serviceId: request.targetServiceId)
            return .failure(error)
        } catch {
            await IPCMetrics.shared.increment(.requestFailed, serviceId: request.targetServiceId)
            return .failure(.internalError(error.localizedDescription, traceId: request.traceId))
        }
    }

    /// Perform a coalesced request
    public func performCoalesced<P: Codable>(
        key: String,
        _ request: IPCRequest<P>,
        policy: IPCCoalescePolicy
    ) async -> Result<Void, IPCError> {
        guard isEnabled else {
            return .failure(.internalError("IPC Manager is disabled"))
        }

        // Encode payload
        let encoder = JSONEncoder()
        guard let data = try? encoder.encode(request.payload) else {
            return .failure(.serializationError(NSError(domain: "IPC", code: -1), traceId: request.traceId))
        }

        // Submit to coalesce handler
        let result = await coalesceHandler.submit(
            requestId: request.id,
            coalesceKey: key,
            policy: policy,
            payload: data
        )

        switch result {
        case .proceed:
            // Proceed with request
            let performResult: Result<EmptyPayload, IPCError> = await perform(request, responseType: EmptyPayload.self)
            return performResult.map { _ in () }

        case .coalesced:
            // Already handled by coalescing
            await IPCMetrics.shared.increment(.requestCoalesced, serviceId: request.targetServiceId)
            return .success(())

        case .dropped:
            await IPCMetrics.shared.increment(.requestDropped, serviceId: request.targetServiceId)
            return .failure(.superseded(traceId: request.traceId))

        case .waitForInFlight(let waitKey):
            do {
                _ = try await coalesceHandler.waitForInFlight(key: waitKey)
                return .success(())
            } catch let error as IPCError {
                return .failure(error)
            } catch {
                return .failure(.internalError(error.localizedDescription, traceId: request.traceId))
            }
        }
    }

    // MARK: - Warm Up

    /// Pre-warm connections to services
    public func warmUp(serviceIds: [String]) async {
        for serviceId in serviceIds {
            // Ensure state machine exists
            if connectionStates[serviceId] == nil {
                connectionStates[serviceId] = IPCConnectionStateMachine(serviceId: serviceId)
            }

            // Attempt connection
            do {
                try await transport.connect(to: serviceId)

                if let stateMachine = connectionStates[serviceId] {
                    await stateMachine.markReady()
                }

                connectedServices.insert(serviceId)

                IPCLogger.shared.info(
                    "Warmed up service",
                    category: .connection,
                    serviceId: serviceId
                )
            } catch {
                IPCLogger.shared.warning(
                    "Failed to warm up service: \(error.localizedDescription)",
                    category: .connection,
                    serviceId: serviceId
                )
            }
        }
    }

    /// Invalidate and clean up
    public func invalidate() async {
        await transport.invalidate()

        for (_, stateMachine) in connectionStates {
            await stateMachine.markInvalidated(reason: "Manager invalidated")
        }

        connectionStates.removeAll()
        tabStates.removeAll()
        pendingByTab.removeAll()
        connectedServices.removeAll()

        IPCLogger.shared.info(
            "IPC Manager invalidated",
            category: .lifecycle
        )
    }

    // MARK: - State Queries

    /// Get tab state
    public func tabState(for tabId: UUID) -> IPCTabState? {
        tabStates[tabId]
    }

    /// Check if tab is ready
    public func isTabReady(_ tabId: UUID) -> Bool {
        tabStates[tabId]?.isReady ?? false
    }

    /// Get service ID for tab
    public func serviceId(for tabId: UUID) -> String? {
        tabStates[tabId]?.serviceId
    }

    // MARK: - Service State Listening

    /// Setup listener for XPC service state notifications
    /// XPC services report their state changes via DistributedNotification
    private func setupServiceStateListener() {
        let hostProcessId = configuration.hostProcessId
        let notificationName = "com.actro.serviceState.\(hostProcessId)"

        serviceStateObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(notificationName),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            Task { @MainActor in
                self?.handleServiceStateNotification(notification)
            }
        }

        IPCLogger.shared.info(
            "Service state listener setup for pid=\(hostProcessId)",
            category: .lifecycle
        )
    }

    /// Handle state notification from XPC service
    private func handleServiceStateNotification(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let serviceId = userInfo["serviceId"] as? String,
              let stateString = userInfo["state"] as? String else {
            return
        }

        let timestamp = userInfo["timestamp"] as? String ?? "unknown"

        IPCLogger.shared.info(
            "Service state received: \(stateString)",
            category: .connection,
            serviceId: serviceId,
            metadata: ["timestamp": timestamp]
        )

        Task {
            await updateStateMachine(serviceId: serviceId, stateString: stateString)
        }
    }

    /// Update state machine based on service state report
    private func updateStateMachine(serviceId: String, stateString: String) async {
        // Ensure state machine exists
        if connectionStates[serviceId] == nil {
            connectionStates[serviceId] = IPCConnectionStateMachine(serviceId: serviceId)
        }

        guard let stateMachine = connectionStates[serviceId] else { return }

        let previousState = await stateMachine.state

        switch stateString {
        case "ready":
            // State machine requires: idle → connecting → ready
            // If in idle state, we need to first transition to connecting
            if previousState == .idle {
                await stateMachine.connect()  // idle → connecting
            }
            let transition = await stateMachine.markReady()  // connecting → ready
            if transition.wasSuccessful {
                connectedServices.insert(serviceId)
                IPCLogger.shared.info(
                    "State machine transition: \(previousState) → ready",
                    category: .connection,
                    serviceId: serviceId
                )
                // Flush any pending requests for this service
                flushPendingRequestsForService(serviceId)
            } else {
                IPCLogger.shared.warning(
                    "State machine transition failed: \(previousState) → ready (current: \(await stateMachine.state))",
                    category: .connection,
                    serviceId: serviceId
                )
            }

        case "error", "busy":
            let transition = await stateMachine.markInterrupted()
            if transition.wasSuccessful {
                IPCLogger.shared.warning(
                    "State machine transition: \(previousState) → interrupted (\(stateString))",
                    category: .connection,
                    serviceId: serviceId
                )
            }

        case "terminating":
            let transition = await stateMachine.markInvalidated(reason: "Service terminating")
            if transition.wasSuccessful {
                connectedServices.remove(serviceId)
                IPCLogger.shared.warning(
                    "State machine transition: \(previousState) → invalidated",
                    category: .connection,
                    serviceId: serviceId
                )
            }

        default:
            IPCLogger.shared.warning(
                "Unknown service state: \(stateString)",
                category: .connection,
                serviceId: serviceId
            )
        }
    }

    /// Flush pending requests for a service when it becomes ready
    private func flushPendingRequestsForService(_ serviceId: String) {
        guard let pending = pendingRequestsByService[serviceId], !pending.isEmpty else { return }

        pendingRequestsByService.removeValue(forKey: serviceId)

        IPCLogger.shared.debug(
            "Flushing \(pending.count) pending requests for service",
            category: .queue,
            serviceId: serviceId
        )

        for request in pending {
            switch request {
            case .tabConfig(let config):
                Task {
                    await sendTabConfigInternal(config, serviceId: serviceId)
                }
            case .sizeChange(let size, let requestId):
                // Size changes are also queued in pendingByTab with tabId context
                // Skip here since they'll be flushed via flushPendingRequests(for:)
                IPCLogger.shared.debug(
                    "Skipping size change in service flush (handled by tab pending queue)",
                    category: .queue,
                    serviceId: serviceId,
                    metadata: ["size": "\(size.width)x\(size.height)", "requestId": "\(requestId)"]
                )
            }
        }
    }

    /// Queue a request for later when service becomes ready
    private func queueRequestForService(_ request: PendingIPCRequest, serviceId: String) {
        var pending = pendingRequestsByService[serviceId] ?? []

        // For size changes, replace existing
        if case .sizeChange = request {
            pending.removeAll { if case .sizeChange = $0 { return true } else { return false } }
        }

        pending.append(request)
        pendingRequestsByService[serviceId] = pending

        IPCLogger.shared.debug(
            "Request queued (state machine not ready)",
            category: .queue,
            serviceId: serviceId
        )
    }

    // MARK: - Private Implementation

    private func setupObservability() {
        // Configure coalesce handler callback
        Task {
            await coalesceHandler.setOnBatchReady { [weak self] requests in
                guard self != nil else { return }

                for request in requests {
                    // Process batched request
                    IPCLogger.shared.debug(
                        "Processing coalesced request",
                        category: .coalesce,
                        metadata: ["key": request.coalesceKey]
                    )
                }
            }
        }

        // Setup backpressure state change handler
        Task {
            await backpressureHandler.onStateChange { [weak self] state in
                guard let self = self else { return }

                Task { @MainActor in
                    IPCLogger.shared.info(
                        "Backpressure state: \(state.description)",
                        category: .backpressure
                    )
                }
            }
        }
    }

    private func nextRequestId() -> Int {
        requestIdCounter += 1
        return requestIdCounter
    }

    private func appendPending(tabId: UUID, request: PendingIPCRequest) {
        var pending = pendingByTab[tabId] ?? []

        // For size changes, replace existing
        if case .sizeChange = request {
            pending.removeAll { if case .sizeChange = $0 { return true } else { return false } }
        }

        pending.append(request)
        pendingByTab[tabId] = pending
    }

    private func flushPendingRequests(for tabId: UUID) {
        guard let pending = pendingByTab[tabId], !pending.isEmpty else { return }
        guard let state = tabStates[tabId], state.isReady, let serviceId = state.serviceId else { return }

        pendingByTab.removeValue(forKey: tabId)

        IPCLogger.shared.debug(
            "Flushing \(pending.count) pending requests",
            category: .queue,
            serviceId: serviceId,
            tabId: tabId
        )

        for request in pending {
            switch request {
            case .tabConfig(let config):
                Task {
                    await sendTabConfigInternal(config, serviceId: serviceId)
                }
            case .sizeChange(let size, let requestId):
                Task {
                    await sendContainerSizeChangedInternal(
                        tabId: tabId,
                        size: size,
                        serviceId: serviceId,
                        requestId: requestId
                    )
                }
            }
        }
    }

    private func sendTabConfigInternal(_ config: IPCXPCTabConfig, serviceId: String) async {
        // 🔑 State machine check: only send when state is ready
        if let stateMachine = connectionStates[serviceId] {
            let canSend = await stateMachine.canAcceptRequests
            if !canSend {
                // Queue request for later
                queueRequestForService(.tabConfig(config), serviceId: serviceId)
                IPCLogger.shared.warning(
                    "Tab config queued: state machine not ready",
                    category: .request,
                    serviceId: serviceId,
                    metadata: ["tabId": config.tabId, "state": "\(await stateMachine.state)"]
                )
                return
            }
        }

        IPCLogger.shared.debug(
            "Sending tab config (state=ready)",
            category: .request,
            serviceId: serviceId,
            metadata: [
                "tabId": config.tabId,
                "size": "\(config.containerWidth)x\(config.containerHeight)",
                "isOverviewMode": "\(isOverviewMode)"
            ]
        )

        // Legacy compatibility: use DistributedNotification for now
        // In Phase 6, this will switch to NSXPCConnection
        let dict = configToDictionary(config)

        guard let jsonData = try? JSONSerialization.data(withJSONObject: dict),
              let jsonString = String(data: jsonData, encoding: .utf8) else {
            IPCLogger.shared.error(
                "Failed to serialize tab config",
                category: .request,
                serviceId: serviceId
            )
            return
        }

        let notificationName = "com.actro.tabConfig.\(configuration.hostProcessId).\(serviceId)"

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: ["config": jsonString],
            deliverImmediately: true
        )

        await IPCMetrics.shared.increment(.requestSent, serviceId: serviceId)
    }

    private func sendContainerSizeChangedInternal(
        tabId: UUID,
        size: CGSize,
        serviceId: String,
        requestId: Int
    ) async {
        // 🔑 State machine check: only send when state is ready
        if let stateMachine = connectionStates[serviceId] {
            let canSend = await stateMachine.canAcceptRequests
            if !canSend {
                IPCLogger.shared.warning(
                    "Container size NOT sent: state machine not ready (will use tab pending queue)",
                    category: .request,
                    serviceId: serviceId,
                    tabId: tabId,
                    metadata: ["size": "\(size.width)x\(size.height)", "state": "\(await stateMachine.state)"]
                )
                // Note: size changes are queued via pendingByTab, not pendingRequestsByService
                // The caller (sendContainerSizeChanged) handles queueing
                return
            }
        }

        IPCLogger.shared.debug(
            "Sending container size (state=ready)",
            category: .request,
            serviceId: serviceId,
            tabId: tabId,
            metadata: [
                "size": "\(size.width)x\(size.height)",
                "isOverviewMode": "\(isOverviewMode)",
                "requestId": "\(requestId)"
            ]
        )

        // Legacy compatibility
        let sizeInfo: [String: Any] = [
            RuntimeBridgeContract.tabId: tabId.uuidString,
            "width": size.width,
            "height": size.height,
            "requestId": requestId,
            "isOverviewMode": isOverviewMode
        ]

        guard let jsonData = try? JSONSerialization.data(withJSONObject: sizeInfo),
              let jsonString = String(data: jsonData, encoding: .utf8) else {
            IPCLogger.shared.error(
                "Failed to serialize size info",
                category: .request,
                serviceId: serviceId
            )
            return
        }

        let notificationName = "com.actro.containerSizeChanged.\(configuration.hostProcessId).\(serviceId)"

        NSLog("📤 [IPC] sendContainerSizeChanged: notificationName=%@ size=%.0fx%.0f", notificationName, size.width, size.height)

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: ["sizeInfo": jsonString],
            deliverImmediately: true
        )

        await IPCMetrics.shared.increment(.requestSent, serviceId: serviceId)
    }

    private func configToDictionary(_ config: IPCXPCTabConfig) -> [String: Any] {
        var dict: [String: Any] = [
            RuntimeBridgeContract.tabId: config.tabId,
            RuntimeBridgeContract.title: config.title,
            "backgroundColor": config.backgroundColor,
            RuntimeBridgeContract.containerWidth: config.containerWidth,
            RuntimeBridgeContract.containerHeight: config.containerHeight,
            "isOverviewMode": isOverviewMode,
            "leftPanelVisible": config.leftPanelVisible,
            "rightPanelVisible": config.rightPanelVisible
        ]
        if let systemAppType = config.systemAppType {
            dict[RuntimeBridgeContract.systemAppType] = systemAppType
        }
        if let unifiedScript = config.unifiedScript {
            dict["unifiedScript"] = unifiedScript
        }
        if let leftPanelScript = config.leftPanelScript {
            dict["leftPanelScript"] = leftPanelScript
        }
        return dict
    }
}

// MARK: - Supporting Types

/// Tab state for IPC tracking
public struct IPCTabState: Sendable {
    public var serviceId: String?
    public var isServiceReady: Bool = false
    public var isConnected: Bool = false
    public var lastSentSize: CGSize?
    public var connectionId: String?

    public var isReady: Bool {
        serviceId != nil && isServiceReady && isConnected
    }
}

/// Pending request types
enum PendingIPCRequest {
    case tabConfig(IPCXPCTabConfig)
    case sizeChange(CGSize, Int)
}
