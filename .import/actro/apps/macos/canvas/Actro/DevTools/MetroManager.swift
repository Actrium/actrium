import Foundation
import Combine
import os.log

private let logger = OSLog(subsystem: "com.canvas.devtools", category: "Metro")

/// Metro bundler process manager
/// Manages the lifecycle of Metro bundler with process-specific address binding
@MainActor
class MetroManager: ObservableObject {

    static let shared = MetroManager()

    // MARK: - State

    enum State: Equatable {
        case stopped
        case starting
        case running
        case error(String)

        var isRunning: Bool {
            if case .running = self { return true }
            return false
        }
    }

    @Published private(set) var state: State = .stopped
    @Published private(set) var logs: [LogEntry] = []
    @Published private(set) var watchFolders: [WatchFolder] = []
    @Published private(set) var actualPort: UInt16?  // Assigned by kernel, parsed from Metro output

    struct LogEntry: Identifiable {
        let id = UUID()
        let timestamp: Date
        let message: String
        let isError: Bool
    }

    struct WatchFolder: Identifiable, Equatable {
        let id = UUID()
        let path: String
        let name: String
        let isSystemApp: Bool

        init(path: String, isSystemApp: Bool = false) {
            self.path = path
            self.name = (path as NSString).lastPathComponent
            self.isSystemApp = isSystemApp
        }
    }

    // MARK: - Address

    let address = MetroAddressResolver.address

    var host: String? {
        guard let port = actualPort else { return nil }
        return MetroAddressResolver.host(port: port)
    }

    var baseURL: URL? {
        guard let port = actualPort else { return nil }
        return MetroAddressResolver.baseURL(port: port)
    }

    // MARK: - Process

    private var process: Process?
    private var stdoutPipe: Pipe?
    private var stderrPipe: Pipe?

    // MARK: - Paths

    /// Path to RCTNApps directory (system apps)
    private var rctnAppsPath: String {
        // In development, use source directory
        // In production, use app bundle
        #if DEBUG
        return "/Users/leo/XData/Starbucks/GoAskAway/Canvas/RCTNApps"
        #else
        return Bundle.main.path(forResource: "RCTNApps", ofType: nil) ?? ""
        #endif
    }

    /// Path to Bun or Node executable
    private var runtimePath: String {
        // First check for embedded Bun in app bundle
        if let embeddedBun = Bundle.main.path(forResource: "bun", ofType: nil, inDirectory: "Vendor") {
            return embeddedBun
        }

        // Fall back to system installations
        let candidates = [
            "/opt/homebrew/bin/bun",
            "/usr/local/bin/bun",
            "/opt/homebrew/bin/node",
            "/usr/local/bin/node",
            "/usr/bin/node"
        ]

        for path in candidates {
            if FileManager.default.isExecutableFile(atPath: path) {
                return path
            }
        }

        return "node"  // Hope it's in PATH
    }

    private var isUsingBun: Bool {
        runtimePath.contains("bun")
    }

    // MARK: - Init

    private init() {
        #if DEBUG
        // Add system apps folder by default (only in debug builds)
        watchFolders = [
            WatchFolder(path: rctnAppsPath, isSystemApp: true)
        ]
        #else
        watchFolders = []
        #endif
    }

    // MARK: - Public API

    /// Start Metro bundler
    func start() async throws {
        guard state != .running && state != .starting else {
            os_log(.info, log: logger, "Metro already running or starting")
            return
        }

        state = .starting
        clearLogs()

        // Retry with different ports if binding fails (race condition with other processes)
        let maxRetries = 3
        var lastError: Error?

        for attempt in 1...maxRetries {
            let port = MetroAddressResolver.findAvailablePort()
            actualPort = port
            appendLog("Starting Metro on port \(port)..." + (attempt > 1 ? " (attempt \(attempt))" : ""))

            do {
                try await startProcess(port: port)
                state = .running
                appendLog("Metro is running on http://\(address):\(port)")
                broadcastMetroConfig()
                return  // Success
            } catch {
                lastError = error
                actualPort = nil

                // Check if it's a port conflict (process started but Metro failed to bind)
                if attempt < maxRetries {
                    appendLog("Port \(port) may be taken, retrying...", isError: true)
                    try? await Task.sleep(nanoseconds: 100_000_000)  // 100ms before retry
                }
            }
        }

        // All retries failed
        state = .error(lastError?.localizedDescription ?? "Unknown error")
        appendLog("Failed to start after \(maxRetries) attempts: \(lastError?.localizedDescription ?? "Unknown")", isError: true)
        throw lastError ?? MetroError.failedToStart
    }

    /// Stop Metro bundler
    func stop() {
        guard let process = process, process.isRunning else {
            state = .stopped
            actualPort = nil
            return
        }

        appendLog("Stopping Metro...")
        process.terminate()
        self.process = nil
        actualPort = nil
        state = .stopped
        appendLog("Metro stopped")
    }

    /// Reload Metro (restart)
    func reload() async throws {
        stop()
        try await Task.sleep(nanoseconds: 500_000_000)  // 0.5s delay
        try await start()
    }

    /// Add a watch folder for guest app development
    func addWatchFolder(path: String) {
        guard !watchFolders.contains(where: { $0.path == path }) else { return }
        watchFolders.append(WatchFolder(path: path))
        appendLog("Added watch folder: \(path)")

        // If running, need to restart to pick up new folder
        if state == .running {
            Task {
                try? await reload()
            }
        }
    }

    /// Remove a watch folder
    func removeWatchFolder(id: UUID) {
        guard let index = watchFolders.firstIndex(where: { $0.id == id }) else { return }
        let folder = watchFolders[index]
        guard !folder.isSystemApp else { return }  // Can't remove system apps

        watchFolders.remove(at: index)
        appendLog("Removed watch folder: \(folder.path)")

        if state == .running {
            Task {
                try? await reload()
            }
        }
    }

    /// Trigger reload on connected clients
    func triggerHotReload() async throws {
        guard let host = host else {
            throw MetroError.notRunning
        }

        let reloadURL = URL(string: "http://\(host)/reload")!
        var request = URLRequest(url: reloadURL)
        request.httpMethod = "POST"

        let (_, response) = try await URLSession.shared.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse,
              httpResponse.statusCode == 200 else {
            throw MetroError.reloadFailed
        }

        appendLog("Hot reload triggered")
    }

    // MARK: - Private

    /// Result of Metro startup detection
    private enum StartupResult {
        case success
        case portConflict
        case failed(String)
    }

    private var startupContinuation: CheckedContinuation<StartupResult, Never>?

    private func startProcess(port: UInt16) async throws {
        let process = Process()
        self.process = process

        // Setup pipes
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        self.stdoutPipe = stdoutPipe
        self.stderrPipe = stderrPipe

        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        // Build command
        process.executableURL = URL(fileURLWithPath: runtimePath)
        process.currentDirectoryURL = URL(fileURLWithPath: rctnAppsPath)

        // Arguments for Metro
        let watchFolderArgs = watchFolders
            .filter { !$0.isSystemApp }  // System app is the projectRoot
            .map { $0.path }
            .joined(separator: ",")

        // Use pre-allocated available port
        if isUsingBun {
            process.arguments = [
                "run", "start",
                "--",
                "--host", address,
                "--port", String(port)
            ]
            if !watchFolderArgs.isEmpty {
                process.arguments?.append(contentsOf: ["--watchFolders", watchFolderArgs])
            }
        } else {
            process.arguments = [
                "node_modules/.bin/react-native", "start",
                "--host", address,
                "--port", String(port)
            ]
            if !watchFolderArgs.isEmpty {
                process.arguments?.append(contentsOf: ["--watchFolders", watchFolderArgs])
            }
        }

        // Patterns to detect success or failure
        let successPatterns = ["Dev server ready", "Metro waiting on", "Welcome to Metro"]
        let errorPatterns = ["EADDRINUSE", "EADDRNOTAVAIL", "address already in use", "address not available"]

        // Handle stdout - detect success
        stdoutPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let output = String(data: data, encoding: .utf8) else { return }
            Task { @MainActor [weak self] in
                guard let self = self else { return }
                for line in output.components(separatedBy: .newlines) where !line.isEmpty {
                    self.appendLog(line)
                    // Check for success
                    for pattern in successPatterns {
                        if line.contains(pattern) {
                            // Parse actual port from Metro output (e.g., "Metro waiting on http://127.0.0.1:49758")
                            if let actualPort = self.parsePortFromOutput(line) {
                                self.actualPort = actualPort
                                os_log(.info, log: logger, "Metro actual port parsed from output: %{public}d", actualPort)
                            }
                            self.startupContinuation?.resume(returning: .success)
                            self.startupContinuation = nil
                            return
                        }
                    }
                }
            }
        }

        // Handle stderr - detect errors
        stderrPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let output = String(data: data, encoding: .utf8) else { return }
            Task { @MainActor [weak self] in
                guard let self = self else { return }
                for line in output.components(separatedBy: .newlines) where !line.isEmpty {
                    self.appendLog(line, isError: true)
                    // Check for port conflict
                    for pattern in errorPatterns {
                        if line.contains(pattern) {
                            self.startupContinuation?.resume(returning: .portConflict)
                            self.startupContinuation = nil
                            return
                        }
                    }
                }
            }
        }

        // Handle termination
        process.terminationHandler = { [weak self] proc in
            Task { @MainActor [weak self] in
                guard let self = self else { return }
                self.stdoutPipe?.fileHandleForReading.readabilityHandler = nil
                self.stderrPipe?.fileHandleForReading.readabilityHandler = nil

                // If still waiting for startup, report failure
                if self.startupContinuation != nil {
                    self.startupContinuation?.resume(returning: .failed("Process terminated with code \(proc.terminationStatus)"))
                    self.startupContinuation = nil
                }

                if self.state == .running || self.state == .starting {
                    self.state = .stopped
                    self.actualPort = nil
                    self.appendLog("Metro process terminated")
                }
            }
        }

        try process.run()

        // Wait for startup result (event-driven, not polling)
        let result = await withCheckedContinuation { continuation in
            self.startupContinuation = continuation

            // Timeout after 30s
            Task {
                try? await Task.sleep(nanoseconds: 30_000_000_000)
                if self.startupContinuation != nil {
                    self.startupContinuation?.resume(returning: .failed("Startup timeout"))
                    self.startupContinuation = nil
                }
            }
        }

        // Handle result
        switch result {
        case .success:
            return  // Success!
        case .portConflict:
            process.terminate()
            throw MetroError.portConflict
        case .failed:
            process.terminate()
            throw MetroError.failedToStart
        }
    }

    /// Parse port number from Metro output line
    /// Handles formats like: "Metro waiting on http://127.0.0.1:49758" or "Dev server ready at http://localhost:8081"
    private func parsePortFromOutput(_ line: String) -> UInt16? {
        // Match port number after colon in URL pattern (e.g., ":49758" or ":8081")
        let pattern = #":(\d{4,5})(?:[/\s]|$)"#
        guard let regex = try? NSRegularExpression(pattern: pattern),
              let match = regex.firstMatch(in: line, range: NSRange(line.startIndex..., in: line)),
              let portRange = Range(match.range(at: 1), in: line),
              let port = UInt16(line[portRange]) else {
            return nil
        }
        return port
    }

    private func appendLog(_ message: String, isError: Bool = false) {
        let entry = LogEntry(timestamp: Date(), message: message, isError: isError)
        logs.append(entry)

        // Keep only last 1000 entries
        if logs.count > 1000 {
            logs.removeFirst(logs.count - 1000)
        }

        if isError {
            os_log(.error, log: logger, "%{public}@", message)
        } else {
            os_log(.info, log: logger, "%{public}@", message)
        }
    }

    func clearLogs() {
        logs.removeAll()
    }

    /// App Group for shared Metro config (XPC Service can read on startup)
    private static let appGroupId = "group.chat.askaway.canvas"
    private static let metroHostKey = "metro.host"
    private static let metroPortKey = "metro.port"

    /// Broadcast Metro config to XPC services via DistributedNotificationCenter
    /// Also writes to shared UserDefaults for XPC Services to query on startup
    private func broadcastMetroConfig() {
        guard let port = actualPort else { return }

        // 1. Write to shared UserDefaults (for XPC Service startup query)
        if let sharedDefaults = UserDefaults(suiteName: Self.appGroupId) {
            sharedDefaults.set(address, forKey: Self.metroHostKey)
            sharedDefaults.set(Int(port), forKey: Self.metroPortKey)
            sharedDefaults.synchronize()
            os_log(.info, log: logger, "Wrote Metro config to shared UserDefaults: %{public}@:%{public}d", address, port)
        }

        // 2. Broadcast notification (for runtime updates)
        let config: [String: Any] = [
            "host": address,
            "port": port
        ]

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.metro.config"),
            object: nil,
            userInfo: config,
            deliverImmediately: true
        )

        os_log(.info, log: logger, "Broadcasted Metro config: %{public}@:%{public}d", address, port)
    }

    /// Re-broadcast Metro config (called when XPC Service starts and may have missed initial broadcast)
    func reBroadcastConfig() {
        guard state.isRunning else { return }
        broadcastMetroConfig()
    }
}

// MARK: - Errors

enum MetroError: LocalizedError {
    case failedToStart
    case reloadFailed
    case runtimeNotFound
    case notRunning
    case portConflict

    var errorDescription: String? {
        switch self {
        case .failedToStart:
            return "Failed to start Metro bundler"
        case .reloadFailed:
            return "Failed to trigger hot reload"
        case .runtimeNotFound:
            return "Node.js or Bun runtime not found"
        case .notRunning:
            return "Metro is not running"
        case .portConflict:
            return "Port is already in use"
        }
    }
}
