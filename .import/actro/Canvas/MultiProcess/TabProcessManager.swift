import Foundation
import Combine

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

    var connection: NSXPCConnection?
    var process: Process?
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
class TabProcessManager: ObservableObject {
    static let shared = TabProcessManager()

    /// Active tab processes keyed by tab ID
    @Published private(set) var processes: [UUID: TabProcessInfo] = [:]

    /// Whether multi-process mode is enabled
    @Published var isMultiProcessEnabled: Bool = false

    /// Metrics update interval in seconds
    var metricsUpdateInterval: TimeInterval = 1.0

    private var metricsTimer: Timer?
    private var cancellables = Set<AnyCancellable>()

    /// Service name for XPC connection
    private let xpcServiceName = "com.canvas.TabRenderer"

    /// Path to the helper app bundle
    private var helperAppPath: URL? {
        guard let bundlePath = Bundle.main.builtInPlugInsPath else { return nil }
        return URL(fileURLWithPath: bundlePath)
            .appendingPathComponent("CanvasTabRenderer.app")
    }

    private init() {
        // Check if helper app exists
        if let path = helperAppPath, FileManager.default.fileExists(atPath: path.path) {
            logger.info("TabProcessManager: Helper app found at \(path.path)")
            isMultiProcessEnabled = true
        } else {
            logger.info("TabProcessManager: Helper app not found, multi-process disabled")
            isMultiProcessEnabled = false
        }
    }

    // MARK: - Process Lifecycle

    /// Spawn a new renderer process for a tab
    func spawnProcess(for tabId: UUID, config: TabXPCConfig) async throws -> TabProcessInfo {
        guard isMultiProcessEnabled else {
            throw TabProcessError.multiProcessDisabled
        }

        guard let helperPath = helperAppPath else {
            throw TabProcessError.helperAppNotFound
        }

        let processInfo = TabProcessInfo(tabId: tabId)
        processInfo.state = .launching
        processes[tabId] = processInfo

        logger.info("TabProcessManager: Launching process for tab \(tabId)")

        do {
            // Launch the helper app as a subprocess
            let process = Process()
            process.executableURL = helperPath.appendingPathComponent("Contents/MacOS/CanvasTabRenderer")
            process.arguments = ["--tab-id", tabId.uuidString, "--xpc-service", xpcServiceName]

            // Set up termination handler
            process.terminationHandler = { [weak self, weak processInfo] terminatedProcess in
                DispatchQueue.main.async {
                    guard let self = self, let info = processInfo else { return }
                    let status = terminatedProcess.terminationStatus
                    if status == 0 {
                        info.state = .terminated
                        logger.info("TabProcessManager: Process for tab \(tabId) terminated normally")
                    } else {
                        info.state = .crashed(reason: "Exit code: \(status)")
                        logger.error("TabProcessManager: Process for tab \(tabId) crashed with status \(status)")
                    }
                    self.processes.removeValue(forKey: tabId)
                }
            }

            try process.run()
            processInfo.process = process
            processInfo.state = .running(pid: process.processIdentifier)

            logger.info("TabProcessManager: Process launched with PID \(process.processIdentifier)")

            // Establish XPC connection
            try await connectToRenderer(processInfo: processInfo, config: config)

            return processInfo
        } catch {
            processInfo.state = .crashed(reason: error.localizedDescription)
            processes.removeValue(forKey: tabId)
            throw error
        }
    }

    /// Connect to a renderer process via XPC
    private func connectToRenderer(processInfo: TabProcessInfo, config: TabXPCConfig) async throws {
        // For embedded helper apps, we use anonymous XPC connections
        // The helper app will create a listener and we connect to it via Mach service

        // Create XPC connection to the service
        let connection = NSXPCConnection(serviceName: "\(xpcServiceName).\(processInfo.tabId.uuidString)")
        connection.remoteObjectInterface = TabXPCInterface.rendererInterface()
        connection.exportedInterface = TabXPCInterface.callbackInterface()
        connection.exportedObject = TabRendererCallbackHandler(tabId: processInfo.tabId)

        connection.interruptionHandler = { [weak processInfo] in
            logger.warning("TabProcessManager: XPC connection interrupted for tab \(processInfo?.tabId.uuidString ?? "unknown")")
        }

        connection.invalidationHandler = { [weak self, weak processInfo] in
            guard let tabId = processInfo?.tabId else { return }
            logger.warning("TabProcessManager: XPC connection invalidated for tab \(tabId)")
            DispatchQueue.main.async {
                self?.processes.removeValue(forKey: tabId)
            }
        }

        connection.resume()
        processInfo.connection = connection

        // Get the remote object proxy
        guard let renderer = connection.remoteObjectProxyWithErrorHandler({ error in
            logger.error("TabProcessManager: XPC proxy error: \(error)")
        }) as? TabRendererProtocol else {
            throw TabProcessError.xpcConnectionFailed
        }

        processInfo.renderer = renderer

        // Load content into the renderer
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            renderer.loadContent(config) { success, errorMessage in
                if success {
                    continuation.resume()
                } else {
                    continuation.resume(throwing: TabProcessError.contentLoadFailed(errorMessage ?? "Unknown error"))
                }
            }
        }
    }

    /// Terminate a tab's renderer process
    func terminateProcess(for tabId: UUID) {
        guard let processInfo = processes[tabId] else { return }

        logger.info("TabProcessManager: Terminating process for tab \(tabId)")

        // Send terminate command via XPC
        processInfo.renderer?.terminate()

        // Give it a moment to terminate gracefully
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            // Force terminate if still running
            if let process = processInfo.process, process.isRunning {
                process.terminate()
            }
            processInfo.connection?.invalidate()
            self?.processes.removeValue(forKey: tabId)
        }
    }

    /// Terminate all renderer processes
    func terminateAllProcesses() {
        logger.info("TabProcessManager: Terminating all processes")
        for tabId in processes.keys {
            terminateProcess(for: tabId)
        }
    }

    // MARK: - Metrics Collection

    /// Start collecting metrics from all running processes
    func startMetricsCollection() {
        guard metricsTimer == nil else { return }

        metricsTimer = Timer.scheduledTimer(withTimeInterval: metricsUpdateInterval, repeats: true) { [weak self] _ in
            self?.collectMetrics()
        }
    }

    /// Stop collecting metrics
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

    /// Get metrics for a specific tab using OS APIs (for when XPC is not available)
    func getProcessMetrics(pid: pid_t) -> (memory: UInt64, cpuTime: UInt64)? {
        var info = rusage_info_current()
        let result = withUnsafeMutablePointer(to: &info) { ptr in
            ptr.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { rusagePtr in
                proc_pid_rusage(pid, RUSAGE_INFO_CURRENT, rusagePtr)
            }
        }

        if result == 0 {
            return (info.ri_phys_footprint, info.ri_user_time + info.ri_system_time)
        }
        return nil
    }

    // MARK: - Event Forwarding

    /// Forward an input event to a tab's renderer
    func forwardEvent(_ event: TabXPCEvent, to tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }
        processInfo.renderer?.handleEvent(event)
    }

    /// Update viewport size for a tab
    func updateViewport(width: Double, height: Double, for tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }
        processInfo.renderer?.updateViewport(width: width, height: height)
    }

    /// Get IOSurface ID for cross-process rendering
    func getIOSurfaceID(for tabId: UUID) async -> UInt32? {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return nil }

        return await withCheckedContinuation { continuation in
            processInfo.renderer?.getIOSurfaceID { surfaceID in
                continuation.resume(returning: surfaceID)
            }
        }
    }
}

// MARK: - Callback Handler

/// Handles callbacks from renderer processes
class TabRendererCallbackHandler: NSObject, TabRendererCallbackProtocol {
    let tabId: UUID

    init(tabId: UUID) {
        self.tabId = tabId
        super.init()
    }

    func consoleLog(level: String, message: String, source: String) {
        // Forward to DevTools console
        DispatchQueue.main.async {
            DevToolsManager.shared.logFromRenderer(tabId: self.tabId, level: level, message: message, source: source)
        }
    }

    func titleChanged(_ newTitle: String) {
        // Post notification for title update
        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .tabTitleChanged,
                object: nil,
                userInfo: ["tabId": self.tabId, "title": newTitle]
            )
        }
    }

    func navigationRequest(url: String, newTab: Bool) {
        // Handle navigation request from renderer
        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .tabNavigationRequest,
                object: nil,
                userInfo: ["tabId": self.tabId, "url": url, "newTab": newTab]
            )
        }
    }

    func surfaceDidUpdate() {
        // Signal that the IOSurface content has been updated
        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .tabSurfaceUpdated,
                object: nil,
                userInfo: ["tabId": self.tabId]
            )
        }
    }

    func rendererDidCrash(reason: String) {
        logger.error("TabProcessManager: Renderer crashed for tab \(self.tabId): \(reason)")
        DispatchQueue.main.async {
            if let processInfo = TabProcessManager.shared.processes[self.tabId] {
                processInfo.state = .crashed(reason: reason)
            }
            NotificationCenter.default.post(
                name: .tabRendererCrashed,
                object: nil,
                userInfo: ["tabId": self.tabId, "reason": reason]
            )
        }
    }
}

// MARK: - Errors

enum TabProcessError: LocalizedError {
    case multiProcessDisabled
    case helperAppNotFound
    case xpcConnectionFailed
    case contentLoadFailed(String)

    var errorDescription: String? {
        switch self {
        case .multiProcessDisabled:
            return "Multi-process mode is disabled"
        case .helperAppNotFound:
            return "Tab renderer helper app not found"
        case .xpcConnectionFailed:
            return "Failed to establish XPC connection"
        case .contentLoadFailed(let message):
            return "Failed to load content: \(message)"
        }
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let tabTitleChanged = Notification.Name("tabTitleChanged")
    static let tabNavigationRequest = Notification.Name("tabNavigationRequest")
    static let tabSurfaceUpdated = Notification.Name("tabSurfaceUpdated")
    static let tabRendererCrashed = Notification.Name("tabRendererCrashed")
}
