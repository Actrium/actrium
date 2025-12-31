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
    var process: Process?

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

/// Manages tab renderer processes via CanvasBroker (LaunchAgent Pattern)
/// Canvas -> CanvasBroker (LaunchAgent) <- CanvasTabRenderer
/// After endpoint handshake, Canvas connects directly to CanvasTabRenderer
class TabProcessManager: ObservableObject {
    static let shared = TabProcessManager()

    /// Active tab processes keyed by tab ID
    @Published private(set) var processes: [UUID: TabProcessInfo] = [:]

    /// Pending endpoint continuations (waiting for broker to deliver endpoints)
    private var pendingEndpoints: [String: CheckedContinuation<NSXPCListenerEndpoint, Error>] = [:]
    private let pendingLock = NSLock()

    /// Whether multi-process mode is enabled
    @Published var isMultiProcessEnabled: Bool = true

    /// Metrics update interval in seconds
    var metricsUpdateInterval: TimeInterval = 1.0

    private var metricsTimer: Timer?
    private var cancellables = Set<AnyCancellable>()

    /// CanvasBroker XPC connection
    private var brokerConnection: NSXPCConnection?

    /// Memory pressure source for responding to low memory
    private var memoryPressureSource: DispatchSourceMemoryPressure?

    /// Timeout for waiting for helper to register
    private let registrationTimeout: TimeInterval = 10.0

    private init() {
        setupMemoryPressureHandler()
    }

    // MARK: - Broker Connection

    /// Ensure broker is running and connect to it
    func ensureBrokerConnection() async throws {
        // First ensure the LaunchAgent is installed and running
        try await LaunchAgentManager.shared.ensureBrokerRunning()

        // Connect to the broker
        await MainActor.run {
            connectToBroker()
        }
    }

    /// Connect to CanvasBroker via machServiceName
    private func connectToBroker() {
        guard brokerConnection == nil else { return }

        NSLog("🔵 [DEBUG] TabProcessManager.connectToBroker: serviceName=\(BrokerXPCInterface.serviceName)")
        logger.info("TabProcessManager: Connecting to CanvasBroker: \(BrokerXPCInterface.serviceName)")

        // Connect to the CanvasBroker LaunchAgent via Mach service name
        brokerConnection = NSXPCConnection(machServiceName: BrokerXPCInterface.serviceName)
        brokerConnection?.remoteObjectInterface = BrokerXPCInterface.brokerInterface()
        brokerConnection?.exportedInterface = BrokerXPCInterface.mainAppCallbackInterface()
        brokerConnection?.exportedObject = self

        brokerConnection?.interruptionHandler = { [weak self] in
            logger.warning("TabProcessManager: Broker connection interrupted")
            self?.brokerConnection = nil
            // Try to reconnect after a delay
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
                self?.connectToBroker()
            }
        }

        brokerConnection?.invalidationHandler = { [weak self] in
            logger.warning("TabProcessManager: Broker connection invalidated")
            self?.brokerConnection = nil
        }

        brokerConnection?.resume()

        // Register as main app with broker
        guard let broker = brokerConnection?.remoteObjectProxyWithErrorHandler({ error in
            logger.error("TabProcessManager: Broker proxy error: \(error)")
        }) as? CanvasBrokerProtocol else {
            logger.error("TabProcessManager: Failed to get broker proxy")
            return
        }

        broker.registerMainApp(callback: self)
        logger.info("TabProcessManager: Registered with CanvasBroker")
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

    /// Get a renderer for a tab by launching helper process
    func getRenderer(for tabId: UUID, config: TabXPCConfig, viewportSize: CGSize? = nil) async throws -> TabProcessInfo {
        NSLog("🔵 [DEBUG] TabProcessManager.getRenderer: tabId=\(tabId), isMultiProcessEnabled=\(isMultiProcessEnabled)")
        logger.info("TabProcessManager: Launching tab \(tabId)")

        // Ensure broker connection
        try await ensureBrokerConnection()

        let processInfo = TabProcessInfo(tabId: tabId)
        processInfo.state = .launching
        processes[tabId] = processInfo

        let width = Int(viewportSize?.width ?? 800)
        let height = Int(viewportSize?.height ?? 600)

        // Launch helper process directly
        guard let helperPath = findHelperPath() else {
            processes.removeValue(forKey: tabId)
            throw TabProcessError.helperAppNotFound
        }

        NSLog("🔵 [DEBUG] TabProcessManager: Launching helper at \(helperPath)")

        let process = Process()
        process.executableURL = URL(fileURLWithPath: helperPath)
        process.arguments = [
            "--tab-id", tabId.uuidString,
            "--width", String(width),
            "--height", String(height),
            "--broker-mode"  // Use broker for endpoint registration
        ]

        // Set up termination handler
        process.terminationHandler = { [weak self, weak processInfo] proc in
            let status = proc.terminationStatus
            NSLog("TabProcessManager: Helper for tab \(tabId) terminated with status \(status)")

            DispatchQueue.main.async {
                processInfo?.state = .terminated
                self?.processes.removeValue(forKey: tabId)
            }
        }

        do {
            try process.run()
            let pid = process.processIdentifier
            NSLog("🔵 [DEBUG] TabProcessManager: Helper launched with PID \(pid)")

            processInfo.process = process
            processInfo.state = .running(pid: pid)
        } catch {
            NSLog("🔴 [DEBUG] TabProcessManager: Failed to launch helper: \(error)")
            processes.removeValue(forKey: tabId)
            throw TabProcessError.helperLaunchFailed(error.localizedDescription)
        }

        logger.info("TabProcessManager: Tab \(tabId) helper launched, waiting for endpoint...")

        // Wait for endpoint from broker
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

    /// Wait for endpoint from broker
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

    // MARK: - Helper Path Discovery

    private func findHelperPath() -> String? {
        // Helper is at: Canvas.app/Contents/Helpers/CanvasTabRenderer.app/Contents/MacOS/CanvasTabRenderer
        // Or: Canvas.app/Contents/PlugIns/CanvasTabRenderer.app/Contents/MacOS/CanvasTabRenderer

        let bundlePath = Bundle.main.bundlePath

        // Try Helpers directory first
        let helpersPath = bundlePath + "/Contents/Helpers/CanvasTabRenderer.app/Contents/MacOS/CanvasTabRenderer"
        if FileManager.default.fileExists(atPath: helpersPath) {
            return helpersPath
        }

        // Fallback to PlugIns directory
        let plugInsPath = bundlePath + "/Contents/PlugIns/CanvasTabRenderer.app/Contents/MacOS/CanvasTabRenderer"
        if FileManager.default.fileExists(atPath: plugInsPath) {
            return plugInsPath
        }

        NSLog("🔴 [DEBUG] TabProcessManager: Helper not found in Helpers or PlugIns")
        return nil
    }

    // MARK: - Process Management

    func terminateProcess(for tabId: UUID) {
        guard let processInfo = processes[tabId] else { return }

        logger.info("TabProcessManager: Terminating process for tab \(tabId)")

        // Terminate the helper process
        processInfo.process?.terminate()

        // Unregister from broker
        (brokerConnection?.remoteObjectProxy as? CanvasBrokerProtocol)?.unregisterHelper(tabId: tabId.uuidString)

        processInfo.connection?.invalidate()
        processes.removeValue(forKey: tabId)
    }

    func terminateAllProcesses() {
        logger.info("TabProcessManager: Terminating all processes")

        for (tabId, processInfo) in processes {
            processInfo.process?.terminate()
            (brokerConnection?.remoteObjectProxy as? CanvasBrokerProtocol)?.unregisterHelper(tabId: tabId.uuidString)
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

// MARK: - MainAppCallbackProtocol

extension TabProcessManager: MainAppCallbackProtocol {
    func helperDidRegister(tabId: String, endpoint: NSXPCListenerEndpoint) {
        logger.info("TabProcessManager: Received endpoint for tab \(tabId) from broker")

        pendingLock.lock()
        if let continuation = pendingEndpoints.removeValue(forKey: tabId) {
            continuation.resume(returning: endpoint)
        }
        pendingLock.unlock()
    }

    func helperDidDisconnect(tabId: String) {
        logger.info("TabProcessManager: Helper \(tabId) disconnected")

        if let uuid = UUID(uuidString: tabId) {
            DispatchQueue.main.async {
                if let processInfo = self.processes[uuid] {
                    processInfo.state = .terminated
                }
                self.processes.removeValue(forKey: uuid)
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
