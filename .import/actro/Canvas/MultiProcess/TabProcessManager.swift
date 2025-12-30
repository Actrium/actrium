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

/// Manages tab renderer processes and their XPC connections
/// Uses Helper App with NSXPCListener.anonymous() for cross-process communication
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

    /// Timeout for waiting for endpoint from helper process
    private let endpointTimeout: TimeInterval = 10.0

    private init() {
        setupMemoryPressureHandler()
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

    // MARK: - Helper App Launch and XPC Connection

    /// Launch a Helper App process and establish XPC connection via anonymous endpoint
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

        // Set up pipes for stdout (endpoint data) and stderr (errors/logs)
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

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
                    logger.debug("TabRenderer[\(pid)]: \(str.trimmingCharacters(in: .newlines))")
                }
            }
        }

        // Read endpoint from stdout
        let stdoutHandle = stdoutPipe.fileHandleForReading
        let endpoint = try await readEndpointFromStdout(handle: stdoutHandle, pid: pid)

        // Create XPC connection using the endpoint
        let connection = NSXPCConnection(listenerEndpoint: endpoint)
        connection.remoteObjectInterface = TabXPCInterface.rendererInterface()
        connection.exportedInterface = TabXPCInterface.callbackInterface()

        // Set up callback handler
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
            process.terminate()
            processes.removeValue(forKey: tabId)
            throw TabProcessError.xpcConnectionFailed
        }

        processInfo.renderer = renderer
        processInfo.state = .running(pid: pid)

        // Set viewport size
        renderer.updateViewport(width: Double(width), height: Double(height))
        logger.info("TabProcessManager: Set viewport to \(width)x\(height)")

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
        logger.info("TabProcessManager: XPC connection ready, surfaceID=\(surfaceID), PID=\(pid)")

        return processInfo
    }

    /// Read NSXPCListenerEndpoint from stdout pipe
    /// Format: [4-byte length (big-endian)][endpoint data]
    private func readEndpointFromStdout(handle: FileHandle, pid: pid_t) async throws -> NSXPCListenerEndpoint {
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let startTime = Date()

                // Read 4-byte length prefix
                var lengthData = Data()
                while lengthData.count < 4 {
                    if Date().timeIntervalSince(startTime) > self.endpointTimeout {
                        continuation.resume(throwing: TabProcessError.endpointTimeout)
                        return
                    }

                    let chunk = handle.availableData
                    if chunk.isEmpty {
                        // Check if process is still running
                        Thread.sleep(forTimeInterval: 0.01)
                        continue
                    }
                    lengthData.append(chunk)
                }

                // Parse length (big-endian)
                let length = lengthData.withUnsafeBytes { $0.load(as: UInt32.self).bigEndian }
                logger.debug("TabProcessManager: Endpoint data length = \(length) bytes")

                // Read endpoint data
                var endpointData = Data()
                if lengthData.count > 4 {
                    // We read some endpoint data along with the length
                    endpointData.append(lengthData.subdata(in: 4..<lengthData.count))
                }

                while endpointData.count < Int(length) {
                    if Date().timeIntervalSince(startTime) > self.endpointTimeout {
                        continuation.resume(throwing: TabProcessError.endpointTimeout)
                        return
                    }

                    let chunk = handle.availableData
                    if chunk.isEmpty {
                        Thread.sleep(forTimeInterval: 0.01)
                        continue
                    }
                    endpointData.append(chunk)
                }

                // Deserialize endpoint
                do {
                    guard let endpoint = try NSKeyedUnarchiver.unarchivedObject(
                        ofClass: NSXPCListenerEndpoint.self,
                        from: endpointData
                    ) else {
                        continuation.resume(throwing: TabProcessError.endpointDeserializationFailed)
                        return
                    }
                    logger.info("TabProcessManager: Successfully received XPC endpoint from PID \(pid)")
                    continuation.resume(returning: endpoint)
                } catch {
                    logger.error("TabProcessManager: Failed to deserialize endpoint: \(error)")
                    continuation.resume(throwing: TabProcessError.endpointDeserializationFailed)
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

        // Give it a moment to terminate gracefully, then force terminate if needed
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            // Invalidate XPC connection
            processInfo.connection?.invalidate()

            // Force terminate the process if still running
            if processInfo.process?.isRunning == true {
                processInfo.process?.terminate()
            }

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
