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
    @Published var surfaceID: UInt32 = 0

    var connection: NSXPCConnection?
    var process: Process?
    var renderer: TabRendererProtocol?
    var inputPipe: Pipe?  // stdin pipe for sending events to helper

    init(tabId: UUID) {
        self.tabId = tabId
    }

    /// Send an event to the helper process via stdin
    func sendEvent(_ event: TabXPCEvent) {
        guard let pipe = inputPipe,
              let json = event.toJSON(),
              let data = (json + "\n").data(using: .utf8) else { return }
        pipe.fileHandleForWriting.write(data)
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

    /// Whether multi-process mode is enabled (always true, helper app required)
    @Published var isMultiProcessEnabled: Bool = true

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

    // MARK: - Process Pool

    /// Pre-warmed processes ready for immediate use
    private var processPool: [TabProcessInfo] = []

    /// Maximum number of pre-warmed processes to keep
    private let maxPoolSize: Int = 2

    /// Whether process pooling is enabled
    @Published var isPoolingEnabled: Bool = true

    /// Memory pressure source for responding to low memory
    private var memoryPressureSource: DispatchSourceMemoryPressure?

    private init() {
        // Verify helper app exists (required for multi-process mode)
        if let path = helperAppPath, FileManager.default.fileExists(atPath: path.path) {
            logger.info("TabProcessManager: Helper app found at \(path.path)")
        } else {
            logger.error("TabProcessManager: Helper app not found! Multi-process mode will not work.")
        }
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
                logger.warning("TabProcessManager: Critical memory pressure - terminating pool and background tabs")
                self.handleCriticalMemoryPressure()
            } else if event.contains(.warning) {
                logger.warning("TabProcessManager: Memory pressure warning - clearing process pool")
                self.handleMemoryPressureWarning()
            }
        }

        memoryPressureSource?.resume()
    }

    private func handleMemoryPressureWarning() {
        // Clear the process pool to free memory
        clearProcessPool()
    }

    private func handleCriticalMemoryPressure() {
        // Clear pool
        clearProcessPool()

        // Notify that memory is critical (UI can choose to show warnings)
        NotificationCenter.default.post(
            name: .memoryPressureCritical,
            object: nil
        )
    }

    /// Warm up the process pool with ready-to-use helper processes
    func warmUpProcessPool() {
        guard isPoolingEnabled else { return }
        guard processPool.count < maxPoolSize else { return }

        let needed = maxPoolSize - processPool.count
        logger.info("TabProcessManager: Warming up \(needed) processes")

        for _ in 0..<needed {
            Task {
                await warmUpSingleProcess()
            }
        }
    }

    private func warmUpSingleProcess() async {
        guard let helperPath = helperAppPath else { return }

        let poolId = UUID()
        let processInfo = TabProcessInfo(tabId: poolId)
        processInfo.state = .launching

        do {
            let process = Process()
            process.executableURL = helperPath.appendingPathComponent("Contents/MacOS/CanvasTabRenderer")
            process.arguments = [
                "--tab-id", poolId.uuidString,
                "--width", "800",
                "--height", "600",
                "--background", "#1a1a2e",
                "--pool-mode"  // Special flag indicating this is a pooled process
            ]

            let outputPipe = Pipe()
            process.standardOutput = outputPipe

            let inputPipe = Pipe()
            process.standardInput = inputPipe
            processInfo.inputPipe = inputPipe

            process.terminationHandler = { [weak self, weak processInfo] _ in
                DispatchQueue.main.async { [weak self] in
                    // Remove from pool if it terminates
                    guard let self = self, let info = processInfo else { return }
                    self.processPool.removeAll { $0.tabId == info.tabId }
                }
            }

            try process.run()
            processInfo.process = process
            processInfo.state = .running(pid: process.processIdentifier)

            // Read surface ID
            let surfaceID = try await readSurfaceID(from: outputPipe, processInfo: processInfo)
            processInfo.surfaceID = surfaceID

            await MainActor.run {
                self.processPool.append(processInfo)
                logger.info("TabProcessManager: Process pool now has \(self.processPool.count) ready processes")
            }
        } catch {
            logger.error("TabProcessManager: Failed to warm up process: \(error)")
        }
    }

    /// Get a pre-warmed process from the pool, or spawn a new one
    func getPooledProcess(for tabId: UUID, config: TabXPCConfig) async throws -> TabProcessInfo {
        // Try to get from pool first
        if let pooledProcess = await MainActor.run(body: { self.processPool.popLast() }) {
            logger.info("TabProcessManager: Using pre-warmed process from pool")

            // Reassign the process to the new tab
            let reassignedInfo = TabProcessInfo(tabId: tabId)
            reassignedInfo.process = pooledProcess.process
            reassignedInfo.inputPipe = pooledProcess.inputPipe
            reassignedInfo.surfaceID = pooledProcess.surfaceID
            reassignedInfo.state = pooledProcess.state

            await MainActor.run {
                self.processes[tabId] = reassignedInfo
            }

            // Warm up a replacement in the background
            Task {
                await warmUpSingleProcess()
            }

            return reassignedInfo
        }

        // No pooled process available, spawn a new one
        logger.info("TabProcessManager: No pooled process available, spawning new")
        return try await spawnProcess(for: tabId, config: config)
    }

    /// Clear all pre-warmed processes from the pool
    func clearProcessPool() {
        logger.info("TabProcessManager: Clearing process pool (\(self.processPool.count) processes)")
        for processInfo in processPool {
            processInfo.process?.terminate()
        }
        processPool.removeAll()
    }

    // MARK: - Process Lifecycle

    /// Spawn a new renderer process for a tab
    func spawnProcess(for tabId: UUID, config: TabXPCConfig) async throws -> TabProcessInfo {
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
            process.arguments = [
                "--tab-id", tabId.uuidString,
                "--width", "\(Int(config.leftPanelVisible ? 800 : 600))",
                "--height", "600",
                "--background", config.backgroundColor
            ]

            // Capture stdout for IOSurface ID
            let outputPipe = Pipe()
            process.standardOutput = outputPipe

            // Set up stdin pipe for sending events
            let inputPipe = Pipe()
            process.standardInput = inputPipe
            processInfo.inputPipe = inputPipe

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

            // Read IOSurface ID from stdout
            let surfaceID = try await readSurfaceID(from: outputPipe, processInfo: processInfo)
            processInfo.surfaceID = surfaceID

            logger.info("TabProcessManager: Got IOSurface ID \(surfaceID) from process")

            return processInfo
        } catch {
            processInfo.state = .crashed(reason: error.localizedDescription)
            processes.removeValue(forKey: tabId)
            throw error
        }
    }

    /// Read the IOSurface ID from the helper process stdout
    private func readSurfaceID(from pipe: Pipe, processInfo: TabProcessInfo) async throws -> UInt32 {
        // Use a simple polling approach - more reliable than async notifications
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let handle = pipe.fileHandleForReading
                var accumulatedOutput = ""
                let startTime = Date()
                let timeout: TimeInterval = 5.0

                // Poll for data with small intervals
                while Date().timeIntervalSince(startTime) < timeout {
                    let data = handle.availableData
                    if !data.isEmpty, let output = String(data: data, encoding: .utf8) {
                        accumulatedOutput += output
                        logger.debug("TabProcessManager: Read from helper: \(output)")

                        // Check for SURFACE_ID in accumulated output
                        for line in accumulatedOutput.components(separatedBy: "\n") {
                            if line.hasPrefix("SURFACE_ID:") {
                                let idString = line.replacingOccurrences(of: "SURFACE_ID:", with: "")
                                if let id = UInt32(idString.trimmingCharacters(in: .whitespaces)) {
                                    logger.info("TabProcessManager: Parsed surface ID: \(id)")
                                    continuation.resume(returning: id)
                                    return
                                }
                            }
                        }
                    }

                    // Check for ERROR
                    if accumulatedOutput.contains("ERROR:") {
                        for line in accumulatedOutput.components(separatedBy: "\n") {
                            if line.hasPrefix("ERROR:") {
                                let errorMsg = line.replacingOccurrences(of: "ERROR:", with: "")
                                continuation.resume(throwing: TabProcessError.contentLoadFailed(errorMsg))
                                return
                            }
                        }
                    }

                    // Small sleep to avoid busy waiting
                    Thread.sleep(forTimeInterval: 0.05)
                }

                // Timeout
                logger.error("TabProcessManager: Timeout waiting for surface ID. Output: \(accumulatedOutput)")
                continuation.resume(throwing: TabProcessError.contentLoadFailed("Timeout waiting for IOSurface ID"))
            }
        }
    }

    // Legacy XPC connection (kept for future use)
    private func connectToRendererViaXPC(processInfo: TabProcessInfo, config: TabXPCConfig) async throws {
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
        return processInfo.surfaceID != 0 ? processInfo.surfaceID : nil
    }

    /// Send an input event to a tab's renderer process via stdin
    func sendEvent(_ event: TabXPCEvent, to tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }
        processInfo.sendEvent(event)
    }

    /// Create and send a mouse event to a tab
    func sendMouseEvent(type: TabXPCEvent.EventType, location: CGPoint, to tabId: UUID) {
        let event = TabXPCEvent(
            eventType: type,
            locationX: Double(location.x),
            locationY: Double(location.y)
        )
        sendEvent(event, to: tabId)
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
    static let memoryPressureCritical = Notification.Name("memoryPressureCritical")
}
