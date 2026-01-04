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
    let port = MetroAddressResolver.currentPort
    var host: String { "\(address):\(port)" }
    var baseURL: URL { MetroAddressResolver.currentBaseURL }

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
        // Add system apps folder by default
        watchFolders = [
            WatchFolder(path: rctnAppsPath, isSystemApp: true)
        ]
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
        appendLog("Starting Metro on \(host)...")

        do {
            try await startProcess()
            state = .running
            appendLog("Metro is running")
            broadcastMetroConfig()
        } catch {
            state = .error(error.localizedDescription)
            appendLog("Failed to start: \(error.localizedDescription)", isError: true)
            throw error
        }
    }

    /// Stop Metro bundler
    func stop() {
        guard let process = process, process.isRunning else {
            state = .stopped
            return
        }

        appendLog("Stopping Metro...")
        process.terminate()
        self.process = nil
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

    private func startProcess() async throws {
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

        if isUsingBun {
            // Bun can run npm scripts directly
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
            // Node needs npx
            process.arguments = [
                "node_modules/.bin/react-native", "start",
                "--host", address,
                "--port", String(port)
            ]
            if !watchFolderArgs.isEmpty {
                process.arguments?.append(contentsOf: ["--watchFolders", watchFolderArgs])
            }
        }

        // Handle stdout
        stdoutPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let output = String(data: data, encoding: .utf8) else { return }
            Task { @MainActor [weak self] in
                for line in output.components(separatedBy: .newlines) where !line.isEmpty {
                    self?.appendLog(line)
                }
            }
        }

        // Handle stderr
        stderrPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let output = String(data: data, encoding: .utf8) else { return }
            Task { @MainActor [weak self] in
                for line in output.components(separatedBy: .newlines) where !line.isEmpty {
                    self?.appendLog(line, isError: true)
                }
            }
        }

        // Handle termination
        process.terminationHandler = { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.stdoutPipe?.fileHandleForReading.readabilityHandler = nil
                self?.stderrPipe?.fileHandleForReading.readabilityHandler = nil
                if self?.state == .running {
                    self?.state = .stopped
                    self?.appendLog("Metro process terminated")
                }
            }
        }

        try process.run()

        // Wait a bit and check if process started successfully
        try await Task.sleep(nanoseconds: 1_000_000_000)  // 1s

        if !process.isRunning {
            throw MetroError.failedToStart
        }
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

    private func clearLogs() {
        logs.removeAll()
    }

    /// Broadcast Metro config to XPC services via DistributedNotificationCenter
    private func broadcastMetroConfig() {
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

        os_log(.info, log: logger, "Broadcasted Metro config: %{public}@", host)
    }
}

// MARK: - Errors

enum MetroError: LocalizedError {
    case failedToStart
    case reloadFailed
    case runtimeNotFound

    var errorDescription: String? {
        switch self {
        case .failedToStart:
            return "Failed to start Metro bundler"
        case .reloadFailed:
            return "Failed to trigger hot reload"
        case .runtimeNotFound:
            return "Node.js or Bun runtime not found"
        }
    }
}
