import Foundation
import Combine
import AppKit
import CanvasIPC

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

    var ipcConnection: IPCConnection?  // Unix Socket IPC
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

/// Manages tab renderer processes via Unix Socket + Protobuf IPC
/// Canvas creates a Unix socket and launches CanvasTabRenderer which connects to it
class LegacyRendererProcessManager: ObservableObject {
    static let shared = LegacyRendererProcessManager()

    /// Active tab processes keyed by tab ID
    @Published private(set) var processes: [UUID: TabProcessInfo] = [:]

    /// Whether multi-process mode is enabled
    @Published var isMultiProcessEnabled: Bool = true

    /// Metrics update interval in seconds
    var metricsUpdateInterval: TimeInterval = 1.0

    private var metricsTimer: Timer?
    private var cancellables = Set<AnyCancellable>()

    /// Memory pressure source for responding to low memory
    private var memoryPressureSource: DispatchSourceMemoryPressure?

    /// Timeout for waiting for helper to connect
    private let connectionTimeout: TimeInterval = 10.0

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
                logger.warning("LegacyRendererProcessManager: Critical memory pressure")
                self.handleCriticalMemoryPressure()
            }
        }

        memoryPressureSource?.resume()
    }

    private func handleCriticalMemoryPressure() {
        NotificationCenter.default.post(name: .memoryPressureCritical, object: nil)
    }

    // MARK: - Get Renderer

    /// Get a renderer for a tab by launching helper process with Unix Socket IPC
    func getRenderer(for tabId: UUID, config: TabXPCConfig, viewportSize: CGSize? = nil) async throws -> TabProcessInfo {
        NSLog("🔵 [DEBUG] LegacyRendererProcessManager.getRenderer: tabId=\(tabId), isMultiProcessEnabled=\(isMultiProcessEnabled)")
        logger.info("LegacyRendererProcessManager: Launching tab \(tabId)")

        let processInfo = TabProcessInfo(tabId: tabId)
        processInfo.state = .launching
        processes[tabId] = processInfo

        let width = Int(viewportSize?.width ?? 800)
        let height = Int(viewportSize?.height ?? 600)

        // Create IPC connection (Unix Socket server)
        let ipcConnection = IPCConnection(tabId: tabId.uuidString)
        processInfo.ipcConnection = ipcConnection

        // Set up IPC callbacks
        ipcConnection.onSurfaceUpdated = { [weak processInfo] newSurfaceID in
            DispatchQueue.main.async {
                guard let info = processInfo, newSurfaceID != info.surfaceID else { return }
                logger.info("LegacyRendererProcessManager: Surface ID updated: \(info.surfaceID) -> \(newSurfaceID)")
                info.surfaceID = newSurfaceID
                NotificationCenter.default.post(
                    name: .tabSurfaceUpdated,
                    object: nil,
                    userInfo: ["tabId": tabId, "surfaceID": newSurfaceID]
                )
            }
        }

        ipcConnection.onTitleChanged = { newTitle in
            DispatchQueue.main.async {
                NotificationCenter.default.post(
                    name: .tabTitleChanged,
                    object: nil,
                    userInfo: ["tabId": tabId, "title": newTitle]
                )
            }
        }

        ipcConnection.onNavigationRequest = { url, newTab in
            DispatchQueue.main.async {
                NotificationCenter.default.post(
                    name: .tabNavigationRequest,
                    object: nil,
                    userInfo: ["tabId": tabId, "url": url, "newTab": newTab]
                )
            }
        }

        ipcConnection.onConsoleLog = { level, message, source in
            DispatchQueue.main.async {
                DevToolsManager.shared.logFromRenderer(tabId: tabId, level: level, message: message, source: source)
            }
        }

        ipcConnection.onCrash = { [weak processInfo] reason in
            DispatchQueue.main.async {
                processInfo?.state = .crashed(reason: reason)
                NotificationCenter.default.post(
                    name: .tabRendererCrashed,
                    object: nil,
                    userInfo: ["tabId": tabId, "reason": reason]
                )
            }
        }

        ipcConnection.onDisconnected = { [weak self, weak processInfo] in
            DispatchQueue.main.async {
                processInfo?.state = .terminated
                self?.processes.removeValue(forKey: tabId)
            }
        }

        // Start the IPC server (creates Unix socket)
        do {
            try ipcConnection.start()
            NSLog("🔵 [DEBUG] LegacyRendererProcessManager: IPC server started at \(ipcConnection.socketPath)")
        } catch {
            NSLog("🔴 [DEBUG] LegacyRendererProcessManager: Failed to start IPC server: \(error)")
            processes.removeValue(forKey: tabId)
            throw TabProcessError.ipcServerFailed(error.localizedDescription)
        }

        // Launch helper process
        guard let helperPath = findHelperPath() else {
            ipcConnection.stop()
            processes.removeValue(forKey: tabId)
            throw TabProcessError.helperAppNotFound
        }

        NSLog("🔵 [DEBUG] LegacyRendererProcessManager: Launching helper at \(helperPath)")

        let process = Process()
        process.executableURL = URL(fileURLWithPath: helperPath)
        process.arguments = [
            "--tab-id", tabId.uuidString,
            "--width", String(width),
            "--height", String(height),
            "--socket-path", ipcConnection.socketPath  // Pass socket path to helper
        ]

        // Set up termination handler
        process.terminationHandler = { [weak self, weak processInfo] proc in
            let status = proc.terminationStatus
            NSLog("LegacyRendererProcessManager: Helper for tab \(tabId) terminated with status \(status)")

            DispatchQueue.main.async {
                processInfo?.ipcConnection?.stop()
                processInfo?.state = .terminated
                self?.processes.removeValue(forKey: tabId)
            }
        }

        do {
            try process.run()
            let pid = process.processIdentifier
            NSLog("🔵 [DEBUG] LegacyRendererProcessManager: Helper launched with PID \(pid)")

            processInfo.process = process
            processInfo.state = .running(pid: pid)
        } catch {
            NSLog("🔴 [DEBUG] LegacyRendererProcessManager: Failed to launch helper: \(error)")
            ipcConnection.stop()
            processes.removeValue(forKey: tabId)
            throw TabProcessError.helperLaunchFailed(error.localizedDescription)
        }

        logger.info("LegacyRendererProcessManager: Tab \(tabId) helper launched, waiting for connection...")

        // Wait for helper to connect (just the connection, not surface ID yet)
        try await waitForHelperConnection(processInfo: processInfo, timeout: connectionTimeout)

        // Update viewport and load content FIRST (this creates the IOSurface)
        try ipcConnection.updateViewport(width: Double(width), height: Double(height))

        // Determine content type and load
        // Priority: unifiedScript > leftPanelScript/rightPanelScript > htmlContent > url
        // Note: url field in Tab is often used to track the source directory, not as content to load
        let contentType: String
        let content: String
        if let script = config.unifiedScript {
            contentType = "javascript"
            content = script
            NSLog("🔵 [DEBUG] LegacyRendererProcessManager: Loading unifiedScript (\(script.count) bytes)")
        } else if let html = config.htmlContent {
            contentType = "html"
            content = html
        } else if let url = config.url {
            contentType = "url"
            content = url
        } else {
            contentType = "html"
            content = "<html><body></body></html>"
        }

        NSLog("📤 [IPC-SEND] loadContent: leftPanelVisible=\(config.leftPanelVisible), rightPanelVisible=\(config.rightPanelVisible), tabId=\(config.tabId)")
        _ = try await ipcConnection.loadContent(
            type: contentType,
            content: content,
            backgroundColor: config.backgroundColor,
            tabId: config.tabId,
            title: config.title,
            leftPanelVisible: config.leftPanelVisible,
            rightPanelVisible: config.rightPanelVisible,
            reloadKey: config.reloadKey,
            contractName: config.contractName,
            contractVersion: config.contractVersion,
            permissions: config.permissions
        )

        // NOW wait for a valid surface ID (after loadContent has created the surface)
        let surfaceID = try await waitForValidSurfaceID(processInfo: processInfo, timeout: connectionTimeout)

        processInfo.surfaceID = surfaceID
        logger.info("LegacyRendererProcessManager: IPC ready, surfaceID=\(surfaceID)")

        return processInfo
    }

    /// Wait for helper to connect and send any ready message (connection only, not surface)
    private func waitForHelperConnection(processInfo: TabProcessInfo, timeout: TimeInterval) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            var didResume = false
            let lock = NSLock()

            // Set up surface update handler to detect connection (ready message)
            let originalHandler = processInfo.ipcConnection?.onSurfaceUpdated
            processInfo.ipcConnection?.onSurfaceUpdated = { surfaceID in
                lock.lock()
                // Resume on ANY surface update (including 0), meaning helper is connected
                if !didResume {
                    didResume = true
                    lock.unlock()
                    NSLog("🔵 [DEBUG] waitForHelperConnection: Helper connected, initial surfaceID=\(surfaceID)")
                    continuation.resume()
                } else {
                    lock.unlock()
                }
                // Call original handler for all updates
                originalHandler?(surfaceID)
            }

            // Set up timeout
            DispatchQueue.global().asyncAfter(deadline: .now() + timeout) {
                lock.lock()
                if !didResume {
                    didResume = true
                    lock.unlock()
                    continuation.resume(throwing: TabProcessError.connectionTimeout)
                } else {
                    lock.unlock()
                }
            }
        }
    }

    /// Wait for a valid (non-zero) surface ID after loadContent
    private func waitForValidSurfaceID(processInfo: TabProcessInfo, timeout: TimeInterval) async throws -> UInt32 {
        // Check if we already have a valid surface ID
        if processInfo.surfaceID != 0 {
            NSLog("🔵 [DEBUG] waitForValidSurfaceID: Already have valid surfaceID=\(processInfo.surfaceID)")
            return processInfo.surfaceID
        }

        return try await withCheckedThrowingContinuation { continuation in
            var didResume = false
            let lock = NSLock()

            // Set up surface update handler to capture valid surface ID
            let originalHandler = processInfo.ipcConnection?.onSurfaceUpdated
            processInfo.ipcConnection?.onSurfaceUpdated = { surfaceID in
                lock.lock()
                // Only resume when we get a valid (non-zero) surface ID
                if !didResume && surfaceID != 0 {
                    didResume = true
                    lock.unlock()
                    NSLog("🔵 [DEBUG] waitForValidSurfaceID: Got valid surfaceID=\(surfaceID)")
                    continuation.resume(returning: surfaceID)
                } else {
                    lock.unlock()
                    if surfaceID == 0 && !didResume {
                        NSLog("🔵 [DEBUG] waitForValidSurfaceID: Ignoring surfaceID=0")
                    }
                }
                // Call original handler for all updates
                originalHandler?(surfaceID)
            }

            // Set up timeout
            DispatchQueue.global().asyncAfter(deadline: .now() + timeout) {
                lock.lock()
                if !didResume {
                    didResume = true
                    lock.unlock()
                    NSLog("🔴 [DEBUG] waitForValidSurfaceID: Timeout waiting for surface ID")
                    continuation.resume(throwing: TabProcessError.connectionTimeout)
                } else {
                    lock.unlock()
                }
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

        NSLog("🔴 [DEBUG] LegacyRendererProcessManager: Helper not found in Helpers or PlugIns")
        return nil
    }

    // MARK: - Process Management

    func terminateProcess(for tabId: UUID) {
        guard let processInfo = processes[tabId] else { return }

        logger.info("LegacyRendererProcessManager: Terminating process for tab \(tabId)")

        // Send terminate message via IPC
        processInfo.ipcConnection?.terminate(reason: "User requested")

        // Stop IPC connection
        processInfo.ipcConnection?.stop()

        // Terminate the helper process
        processInfo.process?.terminate()

        processes.removeValue(forKey: tabId)
    }

    func terminateAllProcesses() {
        logger.info("LegacyRendererProcessManager: Terminating all processes")

        for (_, processInfo) in processes {
            processInfo.ipcConnection?.terminate(reason: "App terminating")
            processInfo.ipcConnection?.stop()
            processInfo.process?.terminate()
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
        for (tabId, processInfo) in processes where processInfo.isRunning {
            Task {
                do {
                    let metricsResponse = try await processInfo.ipcConnection?.getMetrics()
                    if let resp = metricsResponse {
                        let metrics = TabXPCMetrics(
                            tabId: tabId.uuidString,
                            pid: processInfo.pid ?? 0,
                            memoryBytes: resp.memoryUsage,
                            cpuTimeNanoseconds: UInt64(resp.cpuUsage * 1_000_000_000)
                        )
                        await MainActor.run {
                            processInfo.metrics = metrics
                        }
                    }
                } catch {
                    logger.warning("LegacyRendererProcessManager: Failed to get metrics for tab \(tabId): \(error)")
                }
            }
        }
    }

    // MARK: - Event Forwarding

    func forwardEvent(_ event: TabXPCEvent, to tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }

        // Convert TabXPCEvent to Canvas_Ipc_InputEvent
        var ipcEvent = Canvas_Ipc_InputEvent()
        ipcEvent.type = convertEventType(event.eventType)
        ipcEvent.x = event.locationX
        ipcEvent.y = event.locationY
        ipcEvent.scrollDeltaX = event.scrollDeltaX
        ipcEvent.scrollDeltaY = event.scrollDeltaY
        ipcEvent.modifiers = UInt32(event.modifierFlags)
        ipcEvent.keyCode = UInt32(event.keyCode)
        ipcEvent.characters = event.characters ?? ""

        try? processInfo.ipcConnection?.sendInputEvent(ipcEvent)
    }

    private func convertEventType(_ rawValue: Int) -> Canvas_Ipc_InputEvent.EventType {
        switch rawValue {
        case 0: return .mouseDown
        case 1: return .mouseUp
        case 2: return .mouseMove
        case 3: return .mouseMove  // mouseDragged -> mouseMove
        case 4: return .scroll
        case 5: return .keyDown
        case 6: return .keyUp
        default: return .mouseMove
        }
    }

    func updateViewport(width: Double, height: Double, for tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }
        try? processInfo.ipcConnection?.updateViewport(width: width, height: height)
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

// MARK: - Errors

enum TabProcessError: LocalizedError {
    case multiProcessDisabled
    case helperAppNotFound
    case helperLaunchFailed(String)
    case ipcServerFailed(String)
    case connectionTimeout
    case contentLoadFailed(String)

    var errorDescription: String? {
        switch self {
        case .multiProcessDisabled: return "Multi-process mode is disabled"
        case .helperAppNotFound: return "Helper app not found"
        case .helperLaunchFailed(let reason): return "Failed to launch helper: \(reason)"
        case .ipcServerFailed(let reason): return "Failed to start IPC server: \(reason)"
        case .connectionTimeout: return "Timeout waiting for helper connection"
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

@available(*, deprecated, message: "Use LegacyRendererProcessManager or RendererLifecycleManager instead.")
typealias TabProcessManager = LegacyRendererProcessManager
