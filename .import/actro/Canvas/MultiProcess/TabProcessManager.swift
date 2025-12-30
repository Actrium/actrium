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

/// Manages tab renderer processes and their XPC connections
class TabProcessManager: ObservableObject {
    static let shared = TabProcessManager()

    /// Active tab processes keyed by tab ID
    @Published private(set) var processes: [UUID: TabProcessInfo] = [:]

    /// Whether multi-process mode is enabled
    @Published var isMultiProcessEnabled: Bool = true

    /// Metrics update interval in seconds
    var metricsUpdateInterval: TimeInterval = 1.0

    private var metricsTimer: Timer?
    private var cancellables = Set<AnyCancellable>()

    /// Service name for XPC Service connection
    private let xpcServiceName = "chat.askaway.CanvasTabRendererXPC"

    /// Memory pressure source for responding to low memory
    private var memoryPressureSource: DispatchSourceMemoryPressure?

    private init() {
        setupMemoryPressureHandler()
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
            } else if event.contains(.warning) {
                logger.warning("TabProcessManager: Memory pressure warning")
            }
        }

        memoryPressureSource?.resume()
    }

    private func handleCriticalMemoryPressure() {
        // Notify that memory is critical (UI can choose to show warnings)
        NotificationCenter.default.post(
            name: .memoryPressureCritical,
            object: nil
        )
    }

    /// Get a renderer for a tab via XPC Service
    func getRenderer(for tabId: UUID, config: TabXPCConfig, viewportSize: CGSize? = nil) async throws -> TabProcessInfo {
        logger.info("TabProcessManager: Connecting to XPC Service for tab \(tabId)")
        return try await connectViaXPCService(for: tabId, config: config, viewportSize: viewportSize)
    }

    // MARK: - XPC Service Connection

    /// Connect to renderer via XPC Service
    func connectViaXPCService(for tabId: UUID, config: TabXPCConfig, viewportSize: CGSize? = nil) async throws -> TabProcessInfo {
        let processInfo = TabProcessInfo(tabId: tabId)
        processInfo.state = .launching
        processes[tabId] = processInfo

        logger.info("TabProcessManager: Connecting to XPC Service for tab \(tabId)")

        // Create XPC connection to the service
        // XPC Service automatically launches when first connection is made
        let connection = NSXPCConnection(serviceName: xpcServiceName)
        connection.remoteObjectInterface = TabXPCInterface.rendererInterface()
        connection.exportedInterface = TabXPCInterface.callbackInterface()

        // Set up callback handler for renderer -> main app notifications
        let callbackHandler = TabRendererCallbackHandler(tabId: tabId)
        callbackHandler.onSurfaceUpdate = { [weak processInfo] in
            // Surface was updated, fetch new ID
            processInfo?.renderer?.getIOSurfaceID { newId in
                DispatchQueue.main.async {
                    guard let info = processInfo, newId != info.surfaceID else { return }
                    logger.info("TabProcessManager: Surface ID updated via XPC: \(info.surfaceID) -> \(newId)")
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

        connection.interruptionHandler = { [weak processInfo] in
            logger.warning("TabProcessManager: XPC connection interrupted for tab \(processInfo?.tabId.uuidString ?? "unknown")")
        }

        connection.invalidationHandler = { [weak self, weak processInfo] in
            guard let tabId = processInfo?.tabId else { return }
            logger.warning("TabProcessManager: XPC connection invalidated for tab \(tabId)")
            DispatchQueue.main.async {
                processInfo?.state = .terminated
                self?.processes.removeValue(forKey: tabId)
            }
        }

        connection.resume()
        processInfo.connection = connection

        // Get the remote object proxy
        guard let renderer = connection.remoteObjectProxyWithErrorHandler({ error in
            logger.error("TabProcessManager: XPC proxy error: \(error)")
        }) as? TabRendererProtocol else {
            processInfo.state = .crashed(reason: "Failed to get XPC proxy")
            processes.removeValue(forKey: tabId)
            throw TabProcessError.xpcConnectionFailed
        }

        processInfo.renderer = renderer
        // XPC Service doesn't give us a PID easily, use -1 as placeholder
        processInfo.state = .running(pid: -1)

        // Set viewport size BEFORE loading content
        let width = viewportSize?.width ?? 800
        let height = viewportSize?.height ?? 600
        renderer.updateViewport(width: width, height: height)
        logger.info("TabProcessManager: Set viewport to \(Int(width))x\(Int(height))")

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
        logger.info("TabProcessManager: XPC connection ready, surfaceID=\(surfaceID)")

        return processInfo
    }

    /// Terminate a tab's renderer process
    func terminateProcess(for tabId: UUID) {
        guard let processInfo = processes[tabId] else { return }

        logger.info("TabProcessManager: Terminating process for tab \(tabId)")

        // Send terminate command via XPC
        processInfo.renderer?.terminate()

        // Give it a moment to terminate gracefully, then invalidate connection
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
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

    // MARK: - Event Forwarding

    /// Forward an input event to a tab's renderer via XPC
    func forwardEvent(_ event: TabXPCEvent, to tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }
        processInfo.renderer?.handleEvent(event)
    }

    /// Update viewport size for a tab via XPC
    func updateViewport(width: Double, height: Double, for tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }

        NSLog("📐 [DIAG] updateViewport: \(Int(width))x\(Int(height)) for tab \(tabId)")

        if let renderer = processInfo.renderer {
            renderer.updateViewport(width: width, height: height)
        }
    }

    /// Get IOSurface ID for cross-process rendering
    func getIOSurfaceID(for tabId: UUID) async -> UInt32? {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return nil }
        return processInfo.surfaceID != 0 ? processInfo.surfaceID : nil
    }

    /// Get metrics for a specific process using OS APIs
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
}

// MARK: - Callback Handler

/// Handles callbacks from renderer processes
class TabRendererCallbackHandler: NSObject, TabRendererCallbackProtocol {
    let tabId: UUID

    /// Closure called when surface is updated (for XPC mode to fetch new surface ID)
    var onSurfaceUpdate: (() -> Void)?

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
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }

            // Call closure if set (XPC mode uses this to fetch new surface ID)
            self.onSurfaceUpdate?()

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
    case xpcConnectionFailed
    case contentLoadFailed(String)

    var errorDescription: String? {
        switch self {
        case .multiProcessDisabled:
            return "Multi-process mode is disabled"
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
    static let memoryPressureCritical = Notification.Name("memoryPressureCritical")
}
