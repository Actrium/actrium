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
    var stdinPipe: Pipe?
    var stdoutPipe: Pipe?

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

    /// Send a message to the helper process via stdin
    func sendMessage(_ message: HelperMessage) {
        guard let data = XPCBroker.encodeMessage(message),
              let stdinHandle = stdinPipe?.fileHandleForWriting else {
            return
        }

        do {
            try stdinHandle.write(contentsOf: data)
        } catch {
            logger.error("TabProcessInfo: Failed to send message: \(error)")
        }
    }
}

// MARK: - Tab Process Manager

/// Manages tab renderer processes and their XPC connections
/// Uses Helper App with XPC Broker pattern for cross-process communication
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

    /// Memory pressure source for responding to low memory
    private var memoryPressureSource: DispatchSourceMemoryPressure?

    /// Timeout for waiting for helper to register via broker
    private let registrationTimeout: TimeInterval = 10.0

    private init() {
        setupMemoryPressureHandler()
        // Start the XPC broker
        XPCBroker.shared.start()
    }

    // MARK: - Helper App Path

    /// Get the path to the CanvasTabRenderer helper app
    private var helperAppPath: String? {
        // Helper app is embedded in main app bundle: Canvas.app/Contents/PlugIns/CanvasTabRenderer.app
        guard let bundlePath = Bundle.main.bundlePath as String? else { return nil }

        // Primary location: PlugIns directory
        let plugInsPath = (bundlePath as NSString).appendingPathComponent("Contents/PlugIns/CanvasTabRenderer.app/Contents/MacOS/CanvasTabRenderer")
        if FileManager.default.fileExists(atPath: plugInsPath) {
            return plugInsPath
        }

        // Fallback: Resources directory
        let resourcesPath = (bundlePath as NSString).appendingPathComponent("Contents/Resources/CanvasTabRenderer.app/Contents/MacOS/CanvasTabRenderer")
        if FileManager.default.fileExists(atPath: resourcesPath) {
            return resourcesPath
        }

        logger.error("TabProcessManager: Helper app not found at PlugIns or Resources")
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

    /// Get a renderer for a tab by launching a Helper App process
    func getRenderer(for tabId: UUID, config: TabXPCConfig, viewportSize: CGSize? = nil) async throws -> TabProcessInfo {
        logger.info("TabProcessManager: Launching Helper App for tab \(tabId)")
        return try await launchHelperApp(for: tabId, config: config, viewportSize: viewportSize)
    }

    // MARK: - Helper App Launch and JSON-based IPC

    /// Launch a Helper App process and establish JSON-based communication
    private func launchHelperApp(for tabId: UUID, config: TabXPCConfig, viewportSize: CGSize? = nil) async throws -> TabProcessInfo {
        guard let helperPath = helperAppPath else {
            throw TabProcessError.helperAppNotFound
        }

        let processInfo = TabProcessInfo(tabId: tabId)
        processInfo.state = .launching
        processes[tabId] = processInfo

        logger.info("TabProcessManager: Launching helper at \(helperPath)")

        // Create process
        let process = Process()
        process.executableURL = URL(fileURLWithPath: helperPath)

        // Pass arguments
        let width = Int(viewportSize?.width ?? 800)
        let height = Int(viewportSize?.height ?? 600)
        process.arguments = [
            "--tab-id", tabId.uuidString,
            "--width", String(width),
            "--height", String(height),
            "--background", config.backgroundColor
        ]

        // Set up pipes for stdin/stdout (JSON IPC) and stderr (logs)
        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()

        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        processInfo.stdinPipe = stdinPipe
        processInfo.stdoutPipe = stdoutPipe

        // Handle process termination
        process.terminationHandler = { [weak self, weak processInfo] proc in
            let status = proc.terminationStatus
            let reason = proc.terminationReason
            logger.info("TabProcessManager: Helper process terminated with status \(status), reason \(reason.rawValue)")

            DispatchQueue.main.async {
                guard let processInfo = processInfo else { return }
                if status != 0 {
                    processInfo.state = .crashed(reason: "Exit code \(status)")
                } else {
                    processInfo.state = .terminated
                }
                self?.processes.removeValue(forKey: tabId)
                XPCBroker.shared.unregisterMessageHandler(tabId: tabId.uuidString)
            }
        }

        // Start the process
        do {
            try process.run()
        } catch {
            logger.error("TabProcessManager: Failed to launch helper: \(error)")
            processes.removeValue(forKey: tabId)
            throw TabProcessError.helperLaunchFailed(error.localizedDescription)
        }

        processInfo.process = process
        let pid = process.processIdentifier
        logger.info("TabProcessManager: Helper process started with PID \(pid)")

        // Read stderr in background for logging
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

        // Set up stdout reader for JSON messages
        let stdoutHandle = stdoutPipe.fileHandleForReading
        setupStdoutReader(handle: stdoutHandle, tabId: tabId, pid: pid)

        // Register message handler for this tab
        XPCBroker.shared.registerMessageHandler(tabId: tabId.uuidString) { [weak processInfo] message in
            self.handleHelperMessage(message, processInfo: processInfo)
        }

        // Wait for helper to send surfaceReady message
        let surfaceID = try await waitForSurfaceReady(tabId: tabId, config: config)

        processInfo.surfaceID = surfaceID
        processInfo.state = .running(pid: pid)

        logger.info("TabProcessManager: Helper ready, surfaceID=\(surfaceID), PID=\(pid)")

        return processInfo
    }

    /// Set up stdout reader to parse JSON messages from helper
    private func setupStdoutReader(handle: FileHandle, tabId: UUID, pid: pid_t) {
        var buffer = Data()

        DispatchQueue.global(qos: .userInitiated).async {
            while true {
                let data = handle.availableData
                if data.isEmpty { break }

                buffer.append(data)

                // Process complete messages (newline-delimited JSON)
                while let newlineIndex = buffer.firstIndex(of: 0x0A) {
                    let messageData = buffer[..<newlineIndex]
                    buffer = buffer[(newlineIndex + 1)...]

                    if let message = XPCBroker.decodeMessage(Data(messageData)) {
                        DispatchQueue.main.async {
                            XPCBroker.shared.processMessage(message)
                        }
                    }
                }
            }
        }
    }

    /// Handle messages from helper
    private func handleHelperMessage(_ message: HelperMessage, processInfo: TabProcessInfo?) {
        guard let processInfo = processInfo else { return }
        let tabId = processInfo.tabId

        switch message.type {
        case .surfaceUpdated:
            if let newSurfaceId = message.surfaceId, newSurfaceId != processInfo.surfaceID {
                logger.info("TabProcessManager: Surface ID updated: \(processInfo.surfaceID) -> \(newSurfaceId)")
                processInfo.surfaceID = newSurfaceId
            }
            NotificationCenter.default.post(
                name: .tabSurfaceUpdated,
                object: nil,
                userInfo: ["tabId": tabId, "surfaceID": processInfo.surfaceID]
            )

        case .consoleLog:
            DevToolsManager.shared.logFromRenderer(
                tabId: tabId,
                level: message.logLevel ?? "log",
                message: message.logMessage ?? "",
                source: message.logSource ?? ""
            )

        case .titleChanged:
            if let title = message.title {
                NotificationCenter.default.post(
                    name: .tabTitleChanged,
                    object: nil,
                    userInfo: ["tabId": tabId, "title": title]
                )
            }

        case .metrics:
            // Metrics are collected via OS-level APIs in collectMetrics()
            // This message is just an acknowledgment from the helper
            break

        case .error:
            logger.error("TabProcessManager: Helper error for \(tabId): \(message.error ?? "unknown")")
            processInfo.state = .crashed(reason: message.error ?? "Unknown error")
            NotificationCenter.default.post(
                name: .tabRendererCrashed,
                object: nil,
                userInfo: ["tabId": tabId, "reason": message.error ?? "Unknown"]
            )

        default:
            break
        }
    }

    /// Wait for helper to send surfaceReady message
    private func waitForSurfaceReady(tabId: UUID, config: TabXPCConfig) async throws -> UInt32 {
        // Send loadContent message to helper
        if let processInfo = processes[tabId] {
            let message = HelperMessage.withConfig(.loadContent, tabId: tabId.uuidString, config: config)
            processInfo.sendMessage(message)
        }

        return try await withCheckedThrowingContinuation { continuation in
            var hasResumed = false
            let resumeLock = NSLock()

            // Set up timeout
            DispatchQueue.global().asyncAfter(deadline: .now() + registrationTimeout) {
                resumeLock.lock()
                defer { resumeLock.unlock() }
                if !hasResumed {
                    hasResumed = true
                    XPCBroker.shared.cancelWait(tabId: tabId.uuidString)
                    continuation.resume(throwing: TabProcessError.endpointTimeout)
                }
            }

            // Wait for surfaceReady from broker
            XPCBroker.shared.waitForRegistration(tabId: tabId.uuidString) { surfaceId, pid in
                resumeLock.lock()
                defer { resumeLock.unlock() }
                if !hasResumed {
                    hasResumed = true
                    logger.info("TabProcessManager: Surface ready for tab \(tabId), surfaceID=\(surfaceId)")
                    continuation.resume(returning: surfaceId)
                }
            }
        }
    }

    /// Terminate a tab's renderer process
    func terminateProcess(for tabId: UUID) {
        guard let processInfo = processes[tabId] else { return }

        logger.info("TabProcessManager: Terminating process for tab \(tabId)")

        // Send terminate command via JSON message
        let message = HelperMessage(type: .terminate, tabId: tabId.uuidString)
        processInfo.sendMessage(message)

        // Give it a moment to terminate gracefully, then force terminate if needed
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            // Close pipes
            try? processInfo.stdinPipe?.fileHandleForWriting.close()

            // Force terminate the process if still running
            if processInfo.process?.isRunning == true {
                processInfo.process?.terminate()
            }

            XPCBroker.shared.unregisterMessageHandler(tabId: tabId.uuidString)
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
        for (tabId, processInfo) in processes where processInfo.isRunning {
            guard let pid = processInfo.pid else { continue }

            // Collect metrics using OS-level APIs
            if let (memory, cpuTime) = getProcessMetrics(pid: pid) {
                let metrics = TabXPCMetrics(
                    tabId: tabId.uuidString,
                    pid: pid,
                    memoryBytes: memory,
                    cpuTimeNanoseconds: cpuTime
                )
                DispatchQueue.main.async {
                    processInfo.metrics = metrics
                }
            }
        }
    }

    // MARK: - Event Forwarding

    /// Forward an input event to a tab's renderer via JSON message
    func forwardEvent(_ event: TabXPCEvent, to tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }

        let message = HelperMessage.withEvent(.handleEvent, tabId: tabId.uuidString, event: event)
        processInfo.sendMessage(message)
    }

    /// Update viewport size for a tab via JSON message
    func updateViewport(width: Double, height: Double, for tabId: UUID) {
        guard let processInfo = processes[tabId], processInfo.isRunning else { return }

        NSLog("📐 [DIAG] updateViewport: \(Int(width))x\(Int(height)) for tab \(tabId)")

        let message = HelperMessage(
            type: .updateViewport,
            tabId: tabId.uuidString,
            width: width,
            height: height
        )
        processInfo.sendMessage(message)
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
    case helperAppNotFound
    case helperLaunchFailed(String)
    case endpointTimeout
    case endpointDeserializationFailed
    case xpcConnectionFailed
    case contentLoadFailed(String)

    var errorDescription: String? {
        switch self {
        case .multiProcessDisabled:
            return "Multi-process mode is disabled"
        case .helperAppNotFound:
            return "Helper app (CanvasTabRenderer) not found in bundle"
        case .helperLaunchFailed(let reason):
            return "Failed to launch helper app: \(reason)"
        case .endpointTimeout:
            return "Timeout waiting for XPC endpoint from helper"
        case .endpointDeserializationFailed:
            return "Failed to deserialize XPC endpoint"
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
