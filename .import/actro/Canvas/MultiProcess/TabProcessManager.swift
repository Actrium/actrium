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

    var process: Process?
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

/// Manages tab renderer processes and their XPC connections
/// Uses XPC Broker for endpoint handshake, then direct XPC for communication
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

    /// Broker XPC connection
    private var brokerConnection: NSXPCConnection?

    /// Memory pressure source for responding to low memory
    private var memoryPressureSource: DispatchSourceMemoryPressure?

    /// Timeout for waiting for helper to register via broker
    private let registrationTimeout: TimeInterval = 10.0

    private init() {
        setupMemoryPressureHandler()
        connectToBroker()
    }

    // MARK: - Broker Connection

    private func connectToBroker() {
        brokerConnection = NSXPCConnection(serviceName: BrokerXPCInterface.serviceName)
        brokerConnection?.remoteObjectInterface = BrokerXPCInterface.brokerInterface()
        brokerConnection?.exportedInterface = BrokerXPCInterface.mainAppCallbackInterface()
        brokerConnection?.exportedObject = self

        brokerConnection?.interruptionHandler = {
            logger.warning("TabProcessManager: Broker connection interrupted")
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
        logger.info("TabProcessManager: Registered with broker as main app")
    }

    // MARK: - Helper App Path

    /// Get the path to the CanvasTabRenderer helper app
    private var helperAppPath: String? {
        guard let bundlePath = Bundle.main.bundlePath as String? else { return nil }

        let plugInsPath = (bundlePath as NSString).appendingPathComponent("Contents/PlugIns/CanvasTabRenderer.app/Contents/MacOS/CanvasTabRenderer")
        if FileManager.default.fileExists(atPath: plugInsPath) {
            return plugInsPath
        }

        let resourcesPath = (bundlePath as NSString).appendingPathComponent("Contents/Resources/CanvasTabRenderer.app/Contents/MacOS/CanvasTabRenderer")
        if FileManager.default.fileExists(atPath: resourcesPath) {
            return resourcesPath
        }

        logger.error("TabProcessManager: Helper app not found")
        return nil
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

    /// Get a renderer for a tab by launching a Helper App process
    func getRenderer(for tabId: UUID, config: TabXPCConfig, viewportSize: CGSize? = nil) async throws -> TabProcessInfo {
        logger.info("TabProcessManager: Launching Helper App for tab \(tabId)")
        return try await launchHelperApp(for: tabId, config: config, viewportSize: viewportSize)
    }

    // MARK: - Helper App Launch

    private func launchHelperApp(for tabId: UUID, config: TabXPCConfig, viewportSize: CGSize? = nil) async throws -> TabProcessInfo {
        guard let helperPath = helperAppPath else {
            throw TabProcessError.helperAppNotFound
        }

        let processInfo = TabProcessInfo(tabId: tabId)
        processInfo.state = .launching
        processes[tabId] = processInfo

        logger.info("TabProcessManager: Launching helper at \(helperPath)")

        // Create and launch process
        let process = Process()
        process.executableURL = URL(fileURLWithPath: helperPath)

        let width = Int(viewportSize?.width ?? 800)
        let height = Int(viewportSize?.height ?? 600)
        process.arguments = [
            "--tab-id", tabId.uuidString,
            "--width", String(width),
            "--height", String(height),
            "--background", config.backgroundColor
        ]

        // Capture stderr for logging
        let stderrPipe = Pipe()
        process.standardError = stderrPipe

        process.terminationHandler = { [weak self, weak processInfo] proc in
            let status = proc.terminationStatus
            logger.info("TabProcessManager: Helper terminated with status \(status)")

            DispatchQueue.main.async {
                guard let processInfo = processInfo else { return }
                processInfo.state = status != 0 ? .crashed(reason: "Exit code \(status)") : .terminated
                self?.processes.removeValue(forKey: tabId)
            }
        }

        do {
            try process.run()
        } catch {
            logger.error("TabProcessManager: Failed to launch helper: \(error)")
            processes.removeValue(forKey: tabId)
            throw TabProcessError.helperLaunchFailed(error.localizedDescription)
        }

        processInfo.process = process
        let pid = process.processIdentifier
        logger.info("TabProcessManager: Helper started with PID \(pid)")

        // Read stderr for logging
        let stderrHandle = stderrPipe.fileHandleForReading
        DispatchQueue.global(qos: .utility).async {
            while true {
                let data = stderrHandle.availableData
                if data.isEmpty { break }
                if let str = String(data: data, encoding: .utf8) {
                    for line in str.split(separator: "\n") {
                        logger.debug("TabRenderer[\(pid)]: \(line)")
                    }
                }
            }
        }

        // Wait for endpoint from broker
        let endpoint = try await waitForEndpoint(tabId: tabId.uuidString)

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
            process.terminate()
            processes.removeValue(forKey: tabId)
            throw TabProcessError.xpcConnectionFailed
        }

        processInfo.renderer = renderer
        processInfo.state = .running(pid: pid)

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
        logger.info("TabProcessManager: Direct XPC ready, surfaceID=\(surfaceID), PID=\(pid)")

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

    // MARK: - Process Management

    func terminateProcess(for tabId: UUID) {
        guard let processInfo = processes[tabId] else { return }

        logger.info("TabProcessManager: Terminating process for tab \(tabId)")

        processInfo.renderer?.terminate()

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            processInfo.connection?.invalidate()
            if processInfo.process?.isRunning == true {
                processInfo.process?.terminate()
            }
            self?.processes.removeValue(forKey: tabId)
        }
    }

    func terminateAllProcesses() {
        logger.info("TabProcessManager: Terminating all processes")
        for tabId in processes.keys {
            terminateProcess(for: tabId)
        }
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
        logger.info("TabProcessManager: Helper disconnected for tab \(tabId)")

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
