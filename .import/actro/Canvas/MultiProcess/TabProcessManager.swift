import Foundation
import Combine
import AppKit

private let logger = AppLogger.app

// MARK: - Tab Process State

/// State of a tab renderer process
enum TabProcessState {
    case idle
    case launching
    case running(pid: pid_t)
    case crashed(reason: String)
    case terminated
}

// MARK: - Tab Process Info

/// Information about a running tab process
class TabProcessInfo: ObservableObject {
    let tabId: UUID
    @Published var state: TabProcessState = .idle
    @Published var metrics: TabXPCMetrics?
    @Published var surfaceID: UInt32 = 0

    var connection: NSXPCConnection?
    var renderer: TabRendererProtocol?

    init(tabId: UUID) {
        self.tabId = tabId
    }

    var pid: pid_t? {
        if case .running(let pid) = state {
            return pid
        }
        return nil
    }

    var isRunning: Bool {
        if case .running = state {
            return true
        }
        return false
    }
}

// MARK: - Tab Process Manager

/// Manages tab renderer processes via CanvasService (Safari Pattern)
/// Canvas <-> CanvasService <-> CanvasTabRenderer
class TabProcessManager: ObservableObject {
    static let shared = TabProcessManager()

    /// Active tab processes keyed by tab ID
    @Published private(set) var processes: [UUID: TabProcessInfo] = [:]

    /// Pending endpoint continuations (waiting for service to deliver endpoints)
    private var pendingEndpoints: [String: CheckedContinuation<NSXPCListenerEndpoint, Error>] = [:]
    private let pendingLock = NSLock()

    /// Whether multi-process mode is enabled
    @Published var isMultiProcessEnabled: Bool = true

    /// Metrics update interval in seconds
    var metricsUpdateInterval: TimeInterval = 1.0

    private var metricsTimer: Timer?
    private var cancellables = Set<AnyCancellable>()

    /// CanvasService XPC connection
    private var serviceConnection: NSXPCConnection?

    /// Memory pressure source for responding to low memory
    private var memoryPressureSource: DispatchSourceMemoryPressure?

    /// Timeout for waiting for helper to register
    private let registrationTimeout: TimeInterval = 10.0

    private init() {
        setupMemoryPressureHandler()
        connectToService()
    }

    // MARK: - Service Connection

    /// Connect to CanvasService
    private func connectToService() {
        logger.info("TabProcessManager: Connecting to CanvasService: \(CanvasServiceXPCInterface.serviceName)")

        // Connect to the CanvasService XPC Service
        serviceConnection = NSXPCConnection(serviceName: CanvasServiceXPCInterface.serviceName)
        serviceConnection?.remoteObjectInterface = CanvasServiceXPCInterface.serviceInterface()
        serviceConnection?.exportedInterface = CanvasServiceXPCInterface.callbackInterface()
        serviceConnection?.exportedObject = self

        serviceConnection?.interruptionHandler = { [weak self] in
            logger.warning("TabProcessManager: Service connection interrupted")
            // Try to reconnect after a delay
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
                self?.connectToService()
            }
        }

        serviceConnection?.invalidationHandler = { [weak self] in
            logger.warning("TabProcessManager: Service connection invalidated")
            self?.serviceConnection = nil
        }

        serviceConnection?.resume()

        // Register as main app with service
        guard let service = serviceConnection?.remoteObjectProxyWithErrorHandler({ error in
            logger.error("TabProcessManager: Service proxy error: \(error)")
        }) as? CanvasServiceProtocol else {
            logger.error("TabProcessManager: Failed to get service proxy")
            return
        }

        service.registerMainApp(callback: self)
        logger.info("TabProcessManager: Registered with CanvasService")
    }

    // MARK: - Memory Pressure Handling

    private func setupMemoryPressureHandler() {
        memoryPressureSource = DispatchSource.makeMemoryPressureSource(
            eventMask: [.warning, .critical],
            queue: .main
        )

        memoryPressureSource?.setEventHandler { [weak self] in
            guard let self = self else { return }
            let event = self.memoryPressureSource?.data ?? []

            if event.contains(.critical) {
                logger.warning("TabProcessManager: Critical memory pressure")
                self.handleCriticalMemoryPressure()
            }
        }

        memoryPressureSource?.resume()
    }

    private func handleCriticalMemoryPressure() {
        NotificationCenter.default.post(name: .memoryPressureCritical, object: nil)
    }

    // MARK: - Get Renderer

    /// Get a renderer for a tab by launching via CanvasService
    func getRenderer(for tabId: UUID, config: TabXPCConfig, viewportSize: CGSize? = nil) async throws -> TabProcessInfo {
        logger.info("TabProcessManager: Requesting tab \(tabId) from CanvasService")

        let processInfo = TabProcessInfo(tabId: tabId)
        processInfo.state = .launching
        processes[tabId] = processInfo

        let width = Int(viewportSize?.width ?? 800)
        let height = Int(viewportSize?.height ?? 600)

        // Request CanvasService to launch the helper
        // Note: Config is passed directly to renderer via XPC later, not through service
        guard let service = serviceConnection?.remoteObjectProxyWithErrorHandler({ error in
            logger.error("TabProcessManager: Service proxy error: \(error)")
        }) as? CanvasServiceProtocol else {
            throw TabProcessError.xpcConnectionFailed
        }

        // Launch tab via service (config passed as empty data, actual config sent via direct XPC)
        let success = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Bool, Error>) in
            service.launchTab(tabId: tabId.uuidString, config: Data(), width: width, height: height) { success, errorMessage in
                if success {
                    continuation.resume(returning: true)
                } else {
                    continuation.resume(throwing: TabProcessError.helperLaunchFailed(errorMessage ?? "Unknown error"))
                }
            }
        }

        guard success else {
            processes.removeValue(forKey: tabId)
            throw TabProcessError.helperLaunchFailed("Service reported failure")
        }

        logger.info("TabProcessManager: Tab \(tabId) launch requested, waiting for endpoint...")

        // Wait for endpoint from service
        let endpoint = try await waitForEndpoint(tabId: tabId.uuidString)

        logger.info("TabProcessManager: Got endpoint for tab \(tabId), connecting directly...")

        // Connect directly to helper using the endpoint
        let connection = NSXPCConnection(listenerEndpoint: endpoint)
        connection.remoteObjectInterface = TabXPCInterface.rendererInterface()
        connection.exportedInterface = TabXPCInterface.callbackInterface()

        let callbackHandler = TabRendererCallbackHandler(tabId: tabId)
        callbackHandler.onSurfaceUpdate = { [weak processInfo] in
            processInfo?.renderer?.getIOSurfaceID { newId in
                DispatchQueue.main.async {
                    guard let info = processInfo, newId != info.surfaceID else { return }
                    logger.info("TabProcessManager: Surface ID updated: \(info.surfaceID) -> \(newId)")
                    info.surfaceID = newId
                    NotificationCenter.default.post(
                        name: .tabSurfaceUpdated,
                        object: nil,
                        userInfo: ["tabId": tabId, "surfaceID": newId]
                    )
                }
            }
        }
        connection.exportedObject = callbackHandler

        connection.interruptionHandler = {
            logger.warning("TabProcessManager: Direct XPC connection interrupted for tab \(tabId)")
        }

        connection.invalidationHandler = { [weak self, weak processInfo] in
            logger.warning("TabProcessManager: Direct XPC connection invalidated for tab \(tabId)")
            DispatchQueue.main.async {
                processInfo?.state = .terminated
                self?.processes.removeValue(forKey: tabId)
            }
        }

        connection.resume()
        processInfo.connection = connection

        guard let renderer = connection.remoteObjectProxyWithErrorHandler({ error in
            logger.error("TabProcessManager: Renderer proxy error: \(error)")
        }) as? TabRendererProtocol else {
            processInfo.state = .crashed(reason: "Failed to get renderer proxy")
            processes.removeValue(forKey: tabId)
            throw TabProcessError.xpcConnectionFailed
        }

        processInfo.renderer = renderer
        processInfo.state = .running(pid: 0)  // We don't have PID in this architecture

        // Set viewport and load content
        renderer.updateViewport(width: Double(width), height: Double(height))

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            renderer.loadContent(config) { success, errorMessage in
                if success {
                    continuation.resume()
                } else {
                    continuation.resume(throwing: TabProcessError.contentLoadFailed(errorMessage ?? "Unknown"))
                }
            }
        }

        // Get initial IOSurface ID
        let surfaceID = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<UInt32, Error>) in
            renderer.getIOSurfaceID { id in
                if id != 0 {
                    continuation.resume(returning: id)
                } else {
                    continuation.resume(throwing: TabProcessError.contentLoadFailed("No IOSurface ID"))
                }
            }
        }

        processInfo.surfaceID = surfaceID
        logger.info("TabProcessManager: Direct XPC ready, surfaceID=\(surfaceID)")

        return processInfo
    }

    /// Wait for endpoint from service
    private func waitForEndpoint(tabId: String) async throws -> NSXPCListenerEndpoint {
        try await withCheckedThrowingContinuation { continuation in
            pendingLock.lock()
            pendingEndpoints[tabId] = continuation
            pendingLock.unlock()

            // Set up timeout
            DispatchQueue.global().asyncAfter(deadline: .now() + registrationTimeout) { [weak self] in
                self?.pendingLock.lock()
                if let pending = self?.pendingEndpoints.removeValue(forKey: tabId) {
                    pending.resume(throwing: TabProcessError.endpointTimeout)
                }
                self?.pendingLock.unlock()
            }
        }
    }

    // MARK: - Process Management

    func terminateProcess(for tabId: UUID) {
        guard let processInfo = processes[tabId] else { return }

        logger.info("TabProcessManager: Terminating process for tab \(tabId)")

        // Tell service to terminate the tab
        (serviceConnection?.remoteObjectProxy as? CanvasServiceProtocol)?.terminateTab(tabId: tabId.uuidString)

        processInfo.connection?.invalidate()
        processes.removeValue(forKey: tabId)
    }

    func terminateAllProcesses() {
        logger.info("TabProcessManager: Terminating all processes")

        // Tell service to terminate all tabs
        (serviceConnection?.remoteObjectProxy as? CanvasServiceProtocol)?.terminateAllTabs()

        for (_, processInfo) in processes {
            processInfo.connection?.invalidate()
        }
        processes.removeAll()
    }

    // MARK: - Metrics Collection

    func startMetricsCollection() {
        guard metricsTimer == nil else { return }
        metricsTimer = Timer.scheduledTimer(withTimeInterval: metricsUpdateInterval, repeats: true) { [weak self] _ in
            self?.collectMetrics()
        }
    }

    func stopMetricsCollection() {
        metricsTimer?.invalidate()
        metricsTimer = nil
    }

    private func collectMetrics() {
        for (_, processInfo) in processes where processInfo.isRunning {
            processInfo.renderer?.getMetrics { [weak processInfo] metrics in
                DispatchQueue.main.async {
                    processInfo?.metrics = metrics
                }
            }
        }
    }

    // MARK: - Event Forwarding

    func forwardEvent(_ event: TabXPCEvent, to tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }
        processInfo.renderer?.handleEvent(event)
    }

    func updateViewport(width: Double, height: Double, for tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }
        processInfo.renderer?.updateViewport(width: width, height: height)
    }

    func getIOSurfaceID(for tabId: UUID) async -> UInt32? {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return nil }
        return processInfo.surfaceID != 0 ? processInfo.surfaceID : nil
    }

    func getProcessMetrics(pid: pid_t) -> (memory: UInt64, cpuTime: UInt64)? {
        var info = rusage_info_current()
        let result = withUnsafeMutablePointer(to: &info) { ptr in
            ptr.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { rusagePtr in
                proc_pid_rusage(pid, RUSAGE_INFO_CURRENT, rusagePtr)
            }
        }
        return result == 0 ? (info.ri_phys_footprint, info.ri_user_time + info.ri_system_time) : nil
    }
}

// MARK: - CanvasServiceCallbackProtocol

extension TabProcessManager: CanvasServiceCallbackProtocol {
    func tabDidBecomeReady(tabId: String, endpoint: NSXPCListenerEndpoint) {
        logger.info("TabProcessManager: Received endpoint for tab \(tabId) from service")

        pendingLock.lock()
        if let continuation = pendingEndpoints.removeValue(forKey: tabId) {
            continuation.resume(returning: endpoint)
        }
        pendingLock.unlock()
    }

    func tabDidTerminate(tabId: String, reason: String) {
        logger.info("TabProcessManager: Tab \(tabId) terminated: \(reason)")

        if let uuid = UUID(uuidString: tabId) {
            DispatchQueue.main.async {
                if let processInfo = self.processes[uuid] {
                    processInfo.state = .terminated
                }
                self.processes.removeValue(forKey: uuid)
            }
        }
    }

    func tabSurfaceDidUpdate(tabId: String, surfaceID: UInt32) {
        logger.info("TabProcessManager: Tab \(tabId) surface updated: \(surfaceID)")

        if let uuid = UUID(uuidString: tabId) {
            DispatchQueue.main.async {
                if let processInfo = self.processes[uuid] {
                    processInfo.surfaceID = surfaceID
                    NotificationCenter.default.post(
                        name: .tabSurfaceUpdated,
                        object: nil,
                        userInfo: ["tabId": uuid, "surfaceID": surfaceID]
                    )
                }
            }
        }
    }
}

// MARK: - Callback Handler

class TabRendererCallbackHandler: NSObject, TabRendererCallbackProtocol {
    let tabId: UUID
    var onSurfaceUpdate: (() -> Void)?

    init(tabId: UUID) {
        self.tabId = tabId
        super.init()
    }

    func consoleLog(level: String, message: String, source: String) {
        DispatchQueue.main.async {
            DevToolsManager.shared.logFromRenderer(tabId: self.tabId, level: level, message: message, source: source)
        }
    }

    func titleChanged(_ newTitle: String) {
        DispatchQueue.main.async {
            NotificationCenter.default.post(name: .tabTitleChanged, object: nil, userInfo: ["tabId": self.tabId, "title": newTitle])
        }
    }

    func navigationRequest(url: String, newTab: Bool) {
        DispatchQueue.main.async {
            NotificationCenter.default.post(name: .tabNavigationRequest, object: nil, userInfo: ["tabId": self.tabId, "url": url, "newTab": newTab])
        }
    }

    func surfaceDidUpdate() {
        DispatchQueue.main.async { [weak self] in
            self?.onSurfaceUpdate?()
            if let tabId = self?.tabId {
                NotificationCenter.default.post(name: .tabSurfaceUpdated, object: nil, userInfo: ["tabId": tabId])
            }
        }
    }

    func rendererDidCrash(reason: String) {
        DispatchQueue.main.async {
            if let processInfo = TabProcessManager.shared.processes[self.tabId] {
                processInfo.state = .crashed(reason: reason)
            }
            NotificationCenter.default.post(name: .tabRendererCrashed, object: nil, userInfo: ["tabId": self.tabId, "reason": reason])
        }
    }
}

// MARK: - Errors

enum TabProcessError: LocalizedError {
    case multiProcessDisabled
    case helperAppNotFound
    case helperLaunchFailed(String)
    case endpointTimeout
    case endpointDeserializationFailed
    case xpcConnectionFailed
    case contentLoadFailed(String)

    var errorDescription: String? {
        switch self {
        case .multiProcessDisabled: return "Multi-process mode is disabled"
        case .helperAppNotFound: return "Helper app not found"
        case .helperLaunchFailed(let reason): return "Failed to launch helper: \(reason)"
        case .endpointTimeout: return "Timeout waiting for endpoint"
        case .endpointDeserializationFailed: return "Failed to deserialize endpoint"
        case .xpcConnectionFailed: return "XPC connection failed"
        case .contentLoadFailed(let message): return "Content load failed: \(message)"
        }
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let tabTitleChanged = Notification.Name("tabTitleChanged")
    static let tabNavigationRequest = Notification.Name("tabNavigationRequest")
    static let tabSurfaceUpdated = Notification.Name("tabSurfaceUpdated")
    static let tabRendererCrashed = Notification.Name("tabRendererCrashed")
    static let memoryPressureCritical = Notification.Name("memoryPressureCritical")
}
