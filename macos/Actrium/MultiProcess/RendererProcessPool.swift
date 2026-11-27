import ActriumIPC
import AppKit
import Combine
import Darwin
import Foundation

enum RendererProcessPoolError: LocalizedError {
    case poolExhausted(runningApps: [String])
    case helperNotFound(String)
    case rendererExited(String)
    case rendererReadyTimeout
    case invalidConfiguration

    var errorDescription: String? {
        switch self {
        case .poolExhausted(let apps):
            return "Renderer process pool is full. Running: \(apps.joined(separator: ", "))"
        case .helperNotFound(let path):
            return "ActriumRenderer helper was not found at \(path)"
        case .rendererExited(let reason):
            return "Renderer process exited: \(reason)"
        case .rendererReadyTimeout:
            return "Renderer process did not become ready in time"
        case .invalidConfiguration:
            return "Renderer configuration cannot be encoded"
        }
    }
}

extension Notification.Name {
    static let rendererSurfaceUpdated = Notification.Name("rendererSurfaceUpdated")
    static let rendererProcessCrashed = Notification.Name("rendererProcessCrashed")
    static let rendererViewportApplied = Notification.Name("rendererViewportApplied")
    static let rendererAccessibilityUpdated = Notification.Name("rendererAccessibilityUpdated")
    static let rendererCaretUpdated = Notification.Name("rendererCaretUpdated")
    static let rendererDocumentSelection = Notification.Name("rendererDocumentSelection")
    static let rendererCursor = Notification.Name("rendererCursor")
    static let rendererTextInputFocusChanged = Notification.Name("rendererTextInputFocusChanged")
}

enum RendererProcessState: String, Sendable, Equatable {
    case starting
    case ready
    case terminating
}

struct RendererProcessSnapshot: Identifiable, Sendable, Equatable {
    let id: String
    let pid: pid_t
    let fingerprint: String?
    let tabIDs: Set<UUID>
    let state: RendererProcessState
}

struct RendererProcessPoolSnapshot: Sendable, Equatable {
    let processes: [RendererProcessSnapshot]

    init(processes: [RendererProcessSnapshot] = []) {
        self.processes = processes.sorted { $0.id < $1.id }
    }

    func process(for tabID: UUID) -> RendererProcessSnapshot? {
        processes.first { $0.tabIDs.contains(tabID) }
    }

    func state(for tabID: UUID) -> RendererProcessState? {
        process(for: tabID)?.state
    }
}

/// Browser-style broker for a dynamically sized pool of homogeneous Renderer
/// processes. A worker is selected by application fingerprint, and one worker can
/// host multiple Tab roots while keeping different fingerprints process-isolated.
@MainActor
final class RendererProcessPool: ObservableObject {
    static let shared = RendererProcessPool(localStorage: RendererLocalStorage(access: .shared))
    private let guestMenu = RendererGuestMenu()

    nonisolated static let systemFingerprintPrefix = "system."

    /// The limit is a safety valve rather than a fixed set of copied bundles.
    /// It grows with the machine while remaining conservative on high-core Macs.
    nonisolated static var recommendedProcessLimit: Int {
        recommendedProcessLimit(
            activeProcessorCount: ProcessInfo.processInfo.activeProcessorCount
        )
    }

    nonisolated static func recommendedProcessLimit(
        activeProcessorCount: Int
    ) -> Int {
        min(12, max(4, (max(activeProcessorCount, 1) + 1) / 2))
    }

    nonisolated static func canonicalFingerprint(_ fingerprint: String) -> String {
        fingerprint.hasPrefix(systemFingerprintPrefix)
            ? systemFingerprintPrefix + "*"
            : fingerprint
    }

    nonisolated static func canShareRenderer(
        currentFingerprint: String?,
        requestedFingerprint: String
    ) -> Bool {
        guard let currentFingerprint else { return false }
        return currentFingerprint == canonicalFingerprint(requestedFingerprint)
    }

    /// A captured renderer operation is valid only while the tab remains on
    /// the same epoch. A tab that has never released a renderer has no stored
    /// epoch, which is the same as epoch 0; both reads must apply that default
    /// or the first attachment result or event subscription is dropped.
    nonisolated static func rendererEpochIsCurrent(
        capturedEpoch: Int,
        currentEpoch: Int?
    ) -> Bool {
        (currentEpoch ?? 0) == capturedEpoch
    }

    /// Gate for tab-scoped renderer messages (e.g. TitleChanged): the queued
    /// message applies only while the sending worker still owns the tab and
    /// the renderer generation is unchanged. A late message from a released
    /// renderer generation must not overwrite newer content's state.
    nonisolated static func acceptsTabRendererMessage(
        senderOwnsTab: Bool,
        capturedEpoch: Int,
        currentEpoch: Int?
    ) -> Bool {
        senderOwnsTab
            && rendererEpochIsCurrent(capturedEpoch: capturedEpoch, currentEpoch: currentEpoch)
    }

    private final class Worker {
        let id: String
        let connection: IPCConnection
        let process: Process
        var fingerprint: String?
        var tabs: Set<UUID> = []
        var surfaces: [UUID: UInt32] = [:]
        var isReady = false
        var isTerminating = false
        var lastUsedAt = Date()

        init(
            id: String,
            fingerprint: String?,
            connection: IPCConnection,
            process: Process
        ) {
            self.id = id
            self.fingerprint = fingerprint
            self.connection = connection
            self.process = process
        }
    }

    @Published private(set) var runningProcessCount = 0
    @Published private(set) var snapshot = RendererProcessPoolSnapshot()

    private var workers: [String: Worker] = [:]
    private var tabWorkerMap: [UUID: String] = [:]
    private let localStorage: RendererLocalStorage
    private let fileImport = RendererFileImport()
    private let fileExport = RendererFileExport()
    private lazy var attachmentStorage: (container: URL, lock: FileHandle)? = {
        guard let container = FileManager.default.containerURL(
            forSecurityApplicationGroupIdentifier: ActriumRuntimePaths.appGroupID
        ) else { return nil }
        do {
            return (container, try RendererAttachmentDirectory.acquireHostLock(in: container))
        } catch {
            NSLog("Could not prepare the Host attachment cache: %@", error.localizedDescription)
            return nil
        }
    }()
    var documentWindow: ((UUID, Int) -> NSWindow?)?
    var fileImportWindow: ((UUID, Int) -> NSWindow?)?
    var fileExportWindow: ((UUID, Int) -> NSWindow?)?
    private var fingerprintWorkerMap: [String: String] = [:]
    private var systemWorkerId: String?
    private var viewportRequestId: UInt32 = 1
    private var memoryPressureSource: DispatchSourceMemoryPressure?
    private var consecutivePrewarmFailures = 0
    private var prewarmRetryWorkItem: DispatchWorkItem?
    /// Per-tab renderer ownership epoch. Bumped whenever the tab's renderer is
    /// released or replaced, so late attachment results and queued Service
    /// event subscriptions can never belong to a replacement renderer.
    private var rendererEpoch: [UUID: Int] = [:]

    init(localStorage: RendererLocalStorage) {
        self.localStorage = localStorage
        RealmServiceBindings.shared.eventHandler = { tabId, event in
            RendererProcessPool.shared.forwardServiceEvent(event, tabId: tabId)
        }
        setupMemoryPressureHandling()
    }

    private func forwardServiceEvent(_ event: RealmServiceEvent, tabId: UUID) {
        guard let worker = worker(for: tabId),
              let data = try? JSONEncoder().encode(event) else { return }
        try? worker.connection.sendHostEvent(
            name: RealmServiceEventSubscription.eventEvent,
            userInfoJSON: String(decoding: data, as: UTF8.self),
            tabId: tabId.uuidString
        )
    }

    // MARK: - Allocation

    func allocateRenderer(for tabId: UUID, fingerprint: String) throws -> String {
        if let currentId = tabWorkerMap[tabId],
           let current = workers[currentId] {
            if fingerprintsMatch(current.fingerprint, fingerprint) {
                return currentId
            }
            releaseRenderer(for: tabId)
        }

        if isSystemFingerprint(fingerprint),
           let systemWorkerId,
           let worker = workers[systemWorkerId] {
            bind(tabId: tabId, to: worker)
            return worker.id
        }

        if let existingId = fingerprintWorkerMap[fingerprint],
           let worker = workers[existingId] {
            bind(tabId: tabId, to: worker)
            return worker.id
        }

        let worker: Worker
        if let prewarmed = workers.values.first(where: {
            $0.fingerprint == nil && $0.tabs.isEmpty && !$0.isTerminating
        }) {
            worker = prewarmed
            worker.fingerprint = Self.canonicalFingerprint(fingerprint)
        } else {
            try evictIdleWorkerIfNeeded(for: fingerprint)
            guard workers.count < Self.recommendedProcessLimit else {
                throw RendererProcessPoolError.poolExhausted(
                    runningApps: getRunningFingerprints()
                )
            }
            worker = try launchWorker(
                fingerprint: Self.canonicalFingerprint(fingerprint)
            )
        }

        if isSystemFingerprint(fingerprint) {
            systemWorkerId = worker.id
            worker.fingerprint = Self.systemFingerprintPrefix + "*"
            fingerprintWorkerMap[Self.systemFingerprintPrefix + "*"] = worker.id
        } else {
            fingerprintWorkerMap[fingerprint] = worker.id
        }
        bind(tabId: tabId, to: worker)
        ensureOnePrewarmedRenderer()
        return worker.id
    }

    func releaseRenderer(for tabId: UUID) {
        guestMenu.cancel(tabId: tabId)
        localStorage.remove(tabId)
        fileImport.cancel(tabId: tabId)
        fileExport.cancel(tabId: tabId)
        // The renderer is going away (close or fingerprint replacement):
        // invalidate late attachment results and cancel the tab's sessions
        // without touching any other tab.
        rendererEpoch[tabId, default: 0] &+= 1
        RealmServiceBindings.shared.cancelAttachmentUploads(tabId)
        RealmServiceBindings.shared.retireSubscriptions(for: tabId)
        guard let workerId = tabWorkerMap.removeValue(forKey: tabId),
              let worker = workers[workerId] else {
            return
        }

        worker.connection.destroyTab(tabId.uuidString)
        worker.tabs.remove(tabId)
        worker.surfaces.removeValue(forKey: tabId)
        worker.lastUsedAt = Date()
        publishSnapshot()

        // System applications deliberately keep their shared renderer. Ordinary
        // renderers retain the fingerprint for fast same-app reuse and are the
        // first eviction candidates when another fingerprint needs capacity.
        if worker.tabs.isEmpty {
            ensureOnePrewarmedRenderer()
        }
    }

    func rendererIdentifier(for tabId: UUID) -> String? {
        tabWorkerMap[tabId]
    }

    func isRendererReady(identifier: String) -> Bool {
        workers[identifier]?.isReady == true
    }

    func canAllocate(fingerprint: String) -> Bool {
        if isSystemFingerprint(fingerprint) || fingerprintWorkerMap[fingerprint] != nil {
            return true
        }
        return workers.count < Self.recommendedProcessLimit ||
            workers.values.contains { $0.tabs.isEmpty && $0.id != systemWorkerId }
    }

    // MARK: - Tab protocol

    func prepareLocalStorage(
        tabId: UUID, reloadKey: Int, origin: String?, publisherIdentity: String?,
        manifest: String?, permissions: [String]
    ) async throws -> PreparedLocalStorage? {
        var config: [String: Any] = ["permissions": permissions]
        config["localStorageOrigin"] = origin
        config["localStoragePublisherIdentity"] = publisherIdentity
        config["capabilityManifestJSON"] = manifest
        return try await localStorage.prepareForActivation(
            config: config, window: documentWindow?(tabId, reloadKey)
        )
    }

    @discardableResult
    func prepareTab(
        tabId: UUID,
        config: [String: Any]
    ) async throws -> UInt32 {
        guard let worker = worker(for: tabId) else {
            throw RendererProcessPoolError.rendererExited("Tab has no allocated renderer")
        }
        try await waitUntilReady(worker)
        try Task.checkCancellation()
        guard self.worker(for: tabId) === worker else {
            throw RendererProcessPoolError.rendererExited("Tab changed renderer")
        }
        let window = documentWindow?(tabId, config["reloadKey"] as? Int ?? 0)
        let resolvedConfig = try await localStorage.resolveOrigin(config: config, window: window)
        try Task.checkCancellation()
        guard self.worker(for: tabId) === worker else {
            throw RendererProcessPoolError.rendererExited("Tab changed renderer")
        }
        let reloadKey = resolvedConfig["reloadKey"] as? Int ?? 0
        guestMenu.update(tabId: tabId, reloadKey: reloadKey)
        fileImport.update(tabId: tabId, reloadKey: reloadKey)
        fileExport.update(tabId: tabId, reloadKey: reloadKey)

        let config = localStorage.prepare(tabId: tabId, config: resolvedConfig)
        guard JSONSerialization.isValidJSONObject(config),
              let data = try? JSONSerialization.data(withJSONObject: config),
              let json = String(data: data, encoding: .utf8) else {
            throw RendererProcessPoolError.invalidConfiguration
        }

        let content = config["unifiedScript"] as? String ?? ""
        _ = try await worker.connection.loadContent(
            type: content.isEmpty ? "configuration" : "javascript",
            content: content,
            backgroundColor: config["backgroundColor"] as? String ?? "#1f1f1f",
            tabId: tabId.uuidString,
            title: config[RuntimeBridgeContract.title] as? String ?? "",
            leftPanelVisible: config["leftPanelVisible"] as? Bool ?? true,
            rightPanelVisible: config["rightPanelVisible"] as? Bool ?? false,
            reloadKey: config["reloadKey"] as? Int ?? 0,
            configJSON: json
        )

        if let surface = worker.surfaces[tabId], surface != 0 {
            return surface
        }
        let surface = try await worker.connection.getSurfaceID(tabId: tabId.uuidString)
        worker.surfaces[tabId] = surface
        return surface
    }

    func updateViewport(
        tabId: UUID,
        size: CGSize,
        isOverviewMode: Bool
    ) {
        guard let worker = worker(for: tabId), worker.isReady,
              size.width > 0, size.height > 0 else {
            return
        }

        let requestId = nextViewportRequestId()
        try? worker.connection.updateViewport(
            tabId: tabId.uuidString,
            width: size.width,
            height: size.height,
            requestId: requestId,
            isOverviewMode: isOverviewMode
        )
    }

    func sendInput(_ event: Actrium_Ipc_InputEvent, to tabId: UUID) {
        guard let worker = worker(for: tabId), worker.isReady else { return }
        var tabEvent = event
        tabEvent.tabID = tabId.uuidString
        try? worker.connection.sendInputEvent(tabEvent)
    }

    func setKeyboardFocus(_ focused: Bool, for tabId: UUID) {
        guard let worker = worker(for: tabId), worker.isReady else { return }
        do {
            try worker.connection.sendHostEvent(
                name: "com.actrium.rendererKeyboardFocus",
                userInfoJSON: String(focused),
                tabId: tabId.uuidString
            )
        } catch {
            AppLogger.reactNative.warning("Renderer keyboard focus update failed: \(error.localizedDescription)")
        }
    }

    func sendHostEvent(name: String, userInfoJSON: String, to tabId: UUID) {
        guard let worker = worker(for: tabId), worker.isReady else { return }
        try? worker.connection.sendHostEvent(
            name: name,
            userInfoJSON: userInfoJSON,
            tabId: tabId.uuidString
        )
    }

    func requestAccessibilityTree(for tabId: UUID) {
        guard let worker = worker(for: tabId), worker.isReady else { return }
        Task { @MainActor in
            guard let tree = try? await worker.connection.getAccessibilityTree(
                tabId: tabId.uuidString
            ) else {
                return
            }
            self.publish(tree: tree)
        }
    }

    func performAccessibilityAction(
        tabId: UUID,
        nodeId: UInt64,
        action: String
    ) -> Bool {
        guard let worker = worker(for: tabId), worker.isReady else { return false }
        Task { @MainActor in
            _ = try? await worker.connection.sendAccessibilityAction(
                tabId: tabId.uuidString,
                nodeId: nodeId,
                action: action
            )
        }
        return true
    }

    private func publish(tree: Actrium_Ipc_AccessibilityTree) {
        guard let tabId = UUID(uuidString: tree.tabID) else { return }
        NotificationCenter.default.post(
            name: .rendererAccessibilityUpdated,
            object: nil,
            userInfo: ["tabId": tabId, "tree": tree]
        )
    }

    // MARK: - Startup and diagnostics

    @discardableResult
    func verifyRendererHelper() -> Bool {
        let url = rendererExecutableURL
        var isDirectory: ObjCBool = false
        let exists = FileManager.default.fileExists(
            atPath: url.path,
            isDirectory: &isDirectory
        )
        if !exists || isDirectory.boolValue {
            NSLog("ActriumRenderer helper missing: %@", url.path)
            return false
        }
        return FileManager.default.isExecutableFile(atPath: url.path)
    }

    func prewarmInitialRenderersAsync() async {
        guard verifyRendererHelper() else { return }
        if systemWorkerId == nil {
            do {
                let worker = try launchWorker(
                    fingerprint: Self.systemFingerprintPrefix + "*"
                )
                systemWorkerId = worker.id
                fingerprintWorkerMap[Self.systemFingerprintPrefix + "*"] = worker.id
            } catch {
                NSLog("Failed to prewarm system Renderer: %@", error.localizedDescription)
            }
        }
        ensureOnePrewarmedRenderer()
    }

    func getPoolStatus() -> String {
        let entries = workers.values
            .sorted { $0.id < $1.id }
            .map {
                let fingerprint = $0.fingerprint ?? "prewarm"
                return "[\($0.id.suffix(8)): \(fingerprint.prefix(12)) tabs=\($0.tabs.count) ready=\($0.isReady)]"
            }
            .joined(separator: " ")
        return "Renderer pool \(workers.count)/\(Self.recommendedProcessLimit): \(entries)"
    }

    func getRunningFingerprints() -> [String] {
        workers.values.compactMap(\.fingerprint).sorted()
    }

    func shutdown() {
        for tabId in tabWorkerMap.keys {
            guestMenu.cancel(tabId: tabId)
            fileImport.cancel(tabId: tabId)
            fileExport.cancel(tabId: tabId)
        }
        let activeWorkers = Array(workers.values)
        for worker in activeWorkers {
            terminate(worker, reason: "Actrium is terminating")
        }
    }

    // MARK: - Worker lifecycle

    private var rendererExecutableURL: URL {
        Bundle.main.bundleURL
            .appendingPathComponent("Contents/Helpers/ActriumRenderer.app")
            .appendingPathComponent("Contents/MacOS/ActriumRenderer")
    }

    private func launchWorker(fingerprint: String?) throws -> Worker {
        let executableURL = rendererExecutableURL
        guard FileManager.default.isExecutableFile(atPath: executableURL.path) else {
            throw RendererProcessPoolError.helperNotFound(executableURL.path)
        }

        let id = "renderer-\(UUID().uuidString)"
        var descriptors: [Int32] = [-1, -1]
        guard Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0 else {
            throw ActriumIPC.IPCError.socketCreationFailed(errno)
        }
        for descriptor in descriptors {
            guard fcntl(descriptor, F_SETFD, FD_CLOEXEC) != -1 else {
                let errorNumber = errno
                Darwin.close(descriptors[0])
                Darwin.close(descriptors[1])
                throw ActriumIPC.IPCError.socketCreationFailed(errorNumber)
            }
        }

        let connection = IPCConnection(
            tabId: id,
            connectedSocket: descriptors[0]
        )
        let childInput = FileHandle(
            fileDescriptor: descriptors[1],
            closeOnDealloc: true
        )
        try connection.start()

        let process = Process()
        process.executableURL = executableURL
        let attachmentDirectory = attachmentStorage.flatMap {
            RendererAttachmentDirectory.url(rendererID: id, in: $0.container)
        }
        var arguments = [
            "--renderer-id", id,
            "--fingerprint", fingerprint ?? "prewarm",
            "--ipc-stdin",
            "--host-pid", String(ProcessInfo.processInfo.processIdentifier)
        ]
        if let attachmentDirectory {
            arguments += ["--attachment-cache-directory", attachmentDirectory.path]
        }
        process.arguments = arguments
        process.standardInput = childInput

        let worker = Worker(
            id: id,
            fingerprint: fingerprint,
            connection: connection,
            process: process
        )
        configureCallbacks(for: worker)

        process.terminationHandler = { [weak self, weak worker] process in
            if let attachmentDirectory {
                do { try RendererAttachmentDirectory.remove(attachmentDirectory) }
                catch { NSLog("Could not remove the exited renderer's attachment cache: %@", error.localizedDescription) }
            }
            DispatchQueue.main.async {
                guard let self, let worker else { return }
                self.handleTermination(
                    of: worker,
                    status: process.terminationStatus
                )
            }
        }

        do {
            try process.run()
            try? childInput.close()
        } catch {
            try? childInput.close()
            connection.stop()
            throw error
        }

        workers[id] = worker
        runningProcessCount = workers.count
        publishSnapshot()
        NSLog(
            "Renderer launched: id=%@ pid=%d fingerprint=%@",
            id,
            process.processIdentifier,
            fingerprint ?? "prewarm"
        )
        return worker
    }

    private func configureCallbacks(for worker: Worker) {
        worker.connection.onRendererReady = { [weak self, weak worker] rendererId, pid in
            guard let self, let worker, rendererId == worker.id else { return }
            worker.isReady = true
            self.consecutivePrewarmFailures = 0
            self.prewarmRetryWorkItem?.cancel()
            self.prewarmRetryWorkItem = nil
            NSLog("Renderer ready: id=%@ pid=%d", rendererId, pid)
            #if DEBUG
            MetroManager.shared.reBroadcastConfig()
            #endif
            self.publishSnapshot()
        }

        worker.connection.onTabSurfaceUpdated = { [weak worker] tabIdString, contextId in
            guard let worker, let tabId = UUID(uuidString: tabIdString) else { return }
            worker.surfaces[tabId] = contextId
            NotificationCenter.default.post(
                name: .rendererSurfaceUpdated,
                object: nil,
                userInfo: ["tabId": tabId, "contextId": contextId]
            )
        }

        worker.connection.onAccessibilityTree = { [weak self] tree in
            self?.publish(tree: tree)
        }

        worker.connection.onTabTitleChanged = { [weak self, weak worker] tabIdString, title, reloadKey in
            guard let self, let worker, let tabId = UUID(uuidString: tabIdString),
                  self.worker(for: tabId) === worker else { return }
            // Re-check renderer ownership after queueing. The BrowserViewModel
            // separately compares reloadKey with the tab's current content
            // generation, which also changes during a same-worker reload.
            let capturedEpoch = self.rendererEpoch[tabId] ?? 0
            Task { [weak self, weak worker] in
                guard let self, let worker,
                      Self.acceptsTabRendererMessage(
                          senderOwnsTab: self.worker(for: tabId) === worker,
                          capturedEpoch: capturedEpoch,
                          currentEpoch: self.rendererEpoch[tabId]
                      ) else { return }
                NotificationCenter.default.post(
                    name: .tabTitleChanged,
                    object: nil,
                    userInfo: ["tabId": tabId, "title": title, "reloadKey": reloadKey]
                )
            }
        }

        worker.connection.onTabNavigationRequest = { tabIdString, url, newTab in
            guard let tabId = UUID(uuidString: tabIdString) else { return }
            NotificationCenter.default.post(
                name: .tabNavigationRequest,
                object: nil,
                userInfo: ["tabId": tabId, "url": url, "newTab": newTab]
            )
        }

        worker.connection.onTabConsoleLog = { tabIdString, level, message, source in
            if let tabId = UUID(uuidString: tabIdString) {
                DevToolsManager.shared.logFromRenderer(
                    tabId: tabId,
                    level: level,
                    message: message,
                    source: source
                )
            } else {
                let logLevel: DevToolsLogEntry.LogLevel
                switch level.lowercased() {
                case "debug": logLevel = .debug
                case "warn", "warning": logLevel = .warn
                case "error": logLevel = .error
                default: logLevel = .info
                }
                DevToolsManager.shared.log(
                    message,
                    level: logLevel,
                    source: "[Renderer] \(source)"
                )
            }
        }

        worker.connection.onHostEvent = { [weak self, weak worker] name, userInfoJSON, tab in
            if name == "com.actrium.rendererCursor" {
                guard let self, let worker, let id = UUID(uuidString: tab), self.worker(for: id) === worker,
                      let payload = try? JSONDecoder().decode([String: String].self, from: Data(userInfoJSON.utf8)),
                      let value = payload["cursor"] else { return }
                let cursor = RendererCursor(rawValue: value) ?? .arrow
                NotificationCenter.default.post(name: .rendererCursor, object: nil, userInfo: ["tabId": id, "cursor": cursor])
                return
            }
            if name == "com.actrium.documentSelection" {
                guard let self, let worker, let id = UUID(uuidString: tab), self.worker(for: id) === worker,
                      let flag = try? JSONDecoder().decode([String: Bool].self, from: Data(userInfoJSON.utf8)),
                      let active = flag["active"] else { return }
                NotificationCenter.default.post(
                    name: .rendererDocumentSelection,
                    object: nil, userInfo: ["tabId": id, "active": active]
                )
                return
            }
            if name == GuestLocalStorageCall.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker else { return }
                let connection = worker.connection
                Task {
                    guard self.worker(for: tabId) === worker else { return }
                    do {
                        if let reply = try await self.localStorage.reply(to: userInfoJSON, tabId: tabId),
                           self.worker(for: tabId) === worker {
                            try connection.sendHostEvent(
                                name: GuestLocalStorageCall.responseEvent, userInfoJSON: reply, tabId: tab
                            )
                        }
                    } catch {
                        AppLogger.reactNative.warning("Renderer local storage request could not be handled.")
                    }
                }
                return
            }
            if name == GuestFileImportCall.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker else { return }
                Task { await self.handleFileImport(userInfoJSON, tabId: tabId, worker: worker) }
                return
            }
            if name == GuestFileExportCall.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker else { return }
                Task { await self.handleFileExport(userInfoJSON, tabId: tabId, worker: worker) }
                return
            }
            if name == GuestMenuCall.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab) else { return }
                // Start tracking outside the main dispatch queue so renderer events can cancel the menu.
                RunLoop.main.perform {
                    MainActor.assumeIsolated {
                        self.handleGuestMenu(userInfoJSON, tabId: tabId, worker: worker)
                    }
                }
                return
            }
            if name == "com.actrium.textInputFocus" {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker,
                      let data = userInfoJSON.data(using: .utf8),
                      let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let focused = payload["focused"] as? Bool else { return }
                NotificationCenter.default.post(
                    name: .rendererTextInputFocusChanged,
                    object: nil,
                    userInfo: ["tabId": tabId, "focused": focused]
                )
                return
            }
            if name == "com.actrium.rendererCaret" {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker else { return }
                var userInfo: [String: Any] = ["tabId": tabId]
                if userInfoJSON != "null" {
                    guard let rect = try? JSONDecoder().decode(CGRect.self, from: Data(userInfoJSON.utf8)) else { return }
                    userInfo["rect"] = rect
                }
                NotificationCenter.default.post(
                    name: .rendererCaretUpdated, object: nil, userInfo: userInfo
                )
                return
            }
            if name == RealmServiceAttachmentAvailabilityCall.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker,
                      let request = try? JSONDecoder().decode(RealmServiceAttachmentAvailabilityCall.self, from: Data(userInfoJSON.utf8)) else {
                    return
                }
                Task { [weak self, weak worker] in
                    let available = await RealmServiceBindings.shared
                        .attachmentUploadAvailable(tabId)
                    let response = RealmServiceAttachmentAvailabilityReply(
                        id: request.id, available: available
                    )
                    guard let self, let worker, self.worker(for: tabId) === worker,
                          let data = try? JSONEncoder().encode(response) else { return }
                    try? worker.connection.sendHostEvent(
                        name: RealmServiceAttachmentAvailabilityCall.responseEvent,
                        userInfoJSON: String(decoding: data, as: UTF8.self),
                        tabId: tab
                    )
                }
                return
            }
            if name == RealmServiceAttachmentPrepareCall.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker,
                      let request = try? JSONDecoder().decode(RealmServiceAttachmentPrepareCall.self, from: Data(userInfoJSON.utf8)) else {
                    return
                }
                Task { [weak self, weak worker] in
                    let deliver = self?.makeAttachmentResultDeliver(tabId: tabId) ?? { _ in }
                    let response = await RealmServiceBindings.shared.prepareAttachmentUpload(
                        request, tabId: tabId, deliver: deliver
                    )
                    guard let self, let worker, self.worker(for: tabId) === worker,
                          let data = try? JSONEncoder().encode(response) else { return }
                    try? worker.connection.sendHostEvent(
                        name: RealmServiceAttachmentPrepareCall.responseEvent,
                        userInfoJSON: String(decoding: data, as: UTF8.self),
                        tabId: tab
                    )
                }
                return
            }
            if name == RealmServiceAttachmentChunkCall.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker,
                      let request = try? JSONDecoder().decode(RealmServiceAttachmentChunkCall.self, from: Data(userInfoJSON.utf8)) else {
                    return
                }
                Task { [weak self, weak worker] in
                    let response = await RealmServiceBindings.shared.sendAttachmentChunk(request, tabId: tabId)
                    guard let self, let worker, self.worker(for: tabId) === worker,
                          let data = try? JSONEncoder().encode(response) else { return }
                    try? worker.connection.sendHostEvent(
                        name: RealmServiceAttachmentChunkCall.responseEvent,
                        userInfoJSON: String(decoding: data, as: UTF8.self),
                        tabId: tab
                    )
                }
                return
            }
            if name == RealmServiceAttachmentUnregisterCall.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker,
                      let request = try? JSONDecoder().decode(RealmServiceAttachmentUnregisterCall.self, from: Data(userInfoJSON.utf8)) else {
                    return
                }
                Task { [weak self, weak worker] in
                    let response = await RealmServiceBindings.shared.unregisterAttachmentStream(request, tabId: tabId)
                    guard let self, let worker, self.worker(for: tabId) === worker,
                          let data = try? JSONEncoder().encode(response) else { return }
                    try? worker.connection.sendHostEvent(
                        name: RealmServiceAttachmentUnregisterCall.responseEvent,
                        userInfoJSON: String(decoding: data, as: UTF8.self),
                        tabId: tab
                    )
                }
                return
            }
            if name == RealmServiceCall.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker,
                      let request = try? JSONDecoder().decode(RealmServiceCall.self, from: Data(userInfoJSON.utf8)) else {
                    return
                }
                Task { [weak self, weak worker] in
                    let response: RealmServiceReply
                    do {
                        let payload = try await RealmServiceBindings.shared.call(request, tabId: tabId)
                        response = RealmServiceReply(id: request.id, payload: payload)
                    } catch {
                        AppLogger.reactNative.warning("Realm Service call failed: \(error.localizedDescription)")
                        response = RealmServiceReply(id: request.id, failure: RealmServiceFailure(serviceError: error))
                    }
                    guard let self, let worker, self.worker(for: tabId) === worker,
                          let data = try? JSONEncoder().encode(response) else { return }
                    try? worker.connection.sendHostEvent(
                        name: RealmServiceCall.responseEvent,
                        userInfoJSON: String(decoding: data, as: UTF8.self),
                        tabId: tab
                    )
                }
                return
            }
            if name == RealmServiceEventSubscription.requestEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker,
                      let request = try? JSONDecoder().decode(
                          RealmServiceEventSubscription.self, from: Data(userInfoJSON.utf8)
                      ) else { return }
                let requestEpoch = self.rendererEpoch[tabId] ?? 0
                Task { [weak self, weak worker] in
                    // The IPC ownership check happened before this task was
                    // queued. Re-check both worker and epoch before creating
                    // state; the same worker can later host a new renderer
                    // generation for this tab.
                    guard let self, let worker, self.worker(for: tabId) === worker,
                          Self.rendererEpochIsCurrent(
                              capturedEpoch: requestEpoch,
                              currentEpoch: self.rendererEpoch[tabId]
                          ) else { return }
                    let reply: RealmServiceEventReply
                    do {
                        try await RealmServiceBindings.shared.subscribe(request, tabId: tabId)
                        reply = RealmServiceEventReply(id: request.id)
                    } catch {
                        reply = RealmServiceEventReply(
                            id: request.id, failure: RealmServiceFailure(serviceError: error)
                        )
                    }
                    guard self.worker(for: tabId) === worker,
                          Self.rendererEpochIsCurrent(
                              capturedEpoch: requestEpoch,
                              currentEpoch: self.rendererEpoch[tabId]
                          ),
                          let data = try? JSONEncoder().encode(reply) else { return }
                    try? worker.connection.sendHostEvent(
                        name: RealmServiceEventSubscription.responseEvent,
                        userInfoJSON: String(decoding: data, as: UTF8.self),
                        tabId: tab
                    )
                }
                return
            }
            if name == RealmServiceEventSubscription.cancelEvent {
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      self.worker(for: tabId) === worker,
                      let request = try? JSONDecoder().decode(
                          RealmServiceEventSubscription.self, from: Data(userInfoJSON.utf8)
                      ) else { return }
                RealmServiceBindings.shared.unsubscribe(request.id, tabId: tabId)
                return
            }
            let localName: Notification.Name
            switch name {
            case "com.actro.openBookmark":
                localName = XPCNotificationBridge.openBookmark
            case "com.actro.openHistoryItem":
                localName = XPCNotificationBridge.openHistoryItem
            case "com.actro.deleteBookmark":
                localName = XPCNotificationBridge.deleteBookmark
            case "com.actro.deleteHistoryItem":
                localName = XPCNotificationBridge.deleteHistoryItem
            case "com.actro.restoreHistoryGroup":
                localName = XPCNotificationBridge.restoreHistoryGroup
            case "com.actro.clearHistory":
                localName = XPCNotificationBridge.clearHistory
            case "com.actrium.setHistoryPage":
                localName = XPCNotificationBridge.setHistoryPage
            case "com.actro.openURLFromStartPage":
                localName = XPCNotificationBridge.openURLFromStartPage
            case "com.actrium.capabilityPermissionRequest":
                localName = XPCNotificationBridge.capabilityPermissionRequest
            default:
                NSLog("Renderer host event rejected: %@", name)
                return
            }

            var userInfo: [AnyHashable: Any]?
            if let data = userInfoJSON.data(using: .utf8),
               let decoded = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                userInfo = decoded
            }
            switch localName {
            case XPCNotificationBridge.openBookmark, XPCNotificationBridge.deleteBookmark,
                 XPCNotificationBridge.openHistoryItem, XPCNotificationBridge.deleteHistoryItem,
                 XPCNotificationBridge.clearHistory, XPCNotificationBridge.restoreHistoryGroup,
                 XPCNotificationBridge.setHistoryPage:
                guard let self, let worker, let tabId = UUID(uuidString: tab),
                      worker.id == self.systemWorkerId,
                      self.worker(for: tabId) === worker,
                      var scopedInfo = userInfo else { return }
                scopedInfo["tabId"] = tabId
                userInfo = scopedInfo
            default:
                break
            }
            NotificationCenter.default.post(
                name: localName,
                object: nil,
                userInfo: userInfo?.isEmpty == false ? userInfo : nil
            )
        }

        worker.connection.onTabCrash = { tabIdString, reason in
            guard let tabId = UUID(uuidString: tabIdString) else { return }
            NotificationCenter.default.post(
                name: .rendererProcessCrashed,
                object: nil,
                userInfo: ["tabId": tabId, "reason": reason]
            )
        }

        worker.connection.onViewportApplied = { tabIdString, requestId, width, height in
            guard let tabId = UUID(uuidString: tabIdString) else { return }
            NotificationCenter.default.post(
                name: .rendererViewportApplied,
                object: nil,
                userInfo: [
                    "tabId": tabId,
                    "requestId": requestId,
                    "width": width,
                    "height": height
                ]
            )
        }

        worker.connection.onDisconnected = { [weak self, weak worker] in
            guard let self, let worker, !worker.isTerminating else { return }
            let status = worker.process.isRunning
                ? -1
                : worker.process.terminationStatus
            self.handleTermination(of: worker, status: status)
        }
    }

    /// Builds the result-delivery closure for one prepare call. The captured
    /// epoch invalidates the closure as soon as the tab's renderer is
    /// released or replaced, which stops in-flight or ghost results from
    /// reaching a renderer that no longer owns the upload session.
    private func makeAttachmentResultDeliver(tabId: UUID) -> @Sendable (RealmServiceAttachmentResultEvent) -> Void {
        let epoch = rendererEpoch[tabId] ?? 0
        return { [weak self] event in
            Task { @MainActor in
                guard let self,
                      Self.rendererEpochIsCurrent(
                          capturedEpoch: epoch,
                          currentEpoch: self.rendererEpoch[tabId]
                      ),
                      let worker = self.worker(for: tabId), worker.isReady,
                      let data = try? JSONEncoder().encode(event) else { return }
                try? worker.connection.sendHostEvent(
                    name: RealmServiceAttachmentResultEvent.event,
                    userInfoJSON: String(decoding: data, as: UTF8.self),
                    tabId: tabId.uuidString
                )
            }
        }
    }

    private func handleGuestMenu(_ json: String, tabId: UUID, worker: Worker) {
        do {
            let reply = try RendererGuestMenu.reply(to: json, isCurrent: { self.worker(for: tabId) === worker }) { call in
                guestMenu.show(call, tabId: tabId)
            }
            let data = try JSONEncoder().encode(reply)
            try worker.connection.sendHostEvent(
                name: GuestMenuCall.responseEvent, userInfoJSON: String(decoding: data, as: UTF8.self),
                tabId: tabId.uuidString
            )
        } catch {
            AppLogger.reactNative.error("Could not reply to guest menu request: \(error.localizedDescription)")
        }
    }

    private func handleFileImport(_ json: String, tabId: UUID, worker: Worker) async {
        do {
            let reply = try await fileImport.reply(to: json, tabId: tabId) { reloadKey in
                guard self.worker(for: tabId) === worker else { return nil }
                return self.fileImportWindow?(tabId, reloadKey)
            }
            guard let reply, self.worker(for: tabId) === worker else { return }
            try worker.connection.sendHostEvent(
                name: GuestFileImportCall.responseEvent, userInfoJSON: reply,
                tabId: tabId.uuidString
            )
        } catch {
            AppLogger.reactNative.error("Could not send file import result: \(error.localizedDescription)")
        }
    }

    private func handleFileExport(_ json: String, tabId: UUID, worker: Worker) async {
        do {
            guard worker === self.worker(for: tabId) else { return }
            let call = try JSONDecoder().decode(GuestFileExportCall.self, from: Data(json.utf8))
            let reply = await fileExport.reply(to: call, tabId: tabId) { reloadKey in
                guard self.worker(for: tabId) === worker else { return nil }
                return self.fileExportWindow?(tabId, reloadKey)
            }
            guard self.worker(for: tabId) === worker else { return }
            let response = try JSONEncoder().encode(reply)
            try worker.connection.sendHostEvent(
                name: GuestFileExportCall.responseEvent, userInfoJSON: String(decoding: response, as: UTF8.self),
                tabId: tabId.uuidString
            )
        } catch {
            AppLogger.reactNative.error("Could not handle the file export request: \(error.localizedDescription)")
        }
    }

    private func handleTermination(of worker: Worker, status: Int32) {
        guard workers[worker.id] != nil else { return }
        worker.isTerminating = true
        worker.connection.stop()

        let affectedTabs = worker.tabs
        for tabId in affectedTabs {
            guestMenu.cancel(tabId: tabId)
            fileImport.cancel(tabId: tabId)
            fileExport.cancel(tabId: tabId)
            rendererEpoch[tabId, default: 0] &+= 1
            RealmServiceBindings.shared.cancelAttachmentUploads(tabId)
            RealmServiceBindings.shared.retireSubscriptions(for: tabId)
            tabWorkerMap.removeValue(forKey: tabId)
            localStorage.remove(tabId)
        }
        if let fingerprint = worker.fingerprint {
            fingerprintWorkerMap.removeValue(forKey: fingerprint)
        }
        if systemWorkerId == worker.id {
            systemWorkerId = nil
        }
        workers.removeValue(forKey: worker.id)
        runningProcessCount = workers.count
        publishSnapshot()

        // Publish the crash only after every Broker lookup has stopped pointing
        // at the dead process. Observers may synchronously request a replacement.
        for tabId in affectedTabs {
            NotificationCenter.default.post(
                name: .rendererProcessCrashed,
                object: nil,
                userInfo: [
                    "tabId": tabId,
                    "reason": "Renderer exited with status \(status)"
                ]
            )
        }
        if worker.isReady {
            ensureOnePrewarmedRenderer()
        } else {
            schedulePrewarmRetry()
        }
    }

    private func terminate(_ worker: Worker, reason: String) {
        guard !worker.isTerminating else { return }
        worker.isTerminating = true
        publishSnapshot()
        worker.connection.terminate(reason: reason)
        if worker.process.isRunning {
            worker.process.terminate()
        } else {
            handleTermination(of: worker, status: worker.process.terminationStatus)
        }
    }

    private func bind(tabId: UUID, to worker: Worker) {
        worker.tabs.insert(tabId)
        worker.lastUsedAt = Date()
        tabWorkerMap[tabId] = worker.id
        publishSnapshot()
    }

    private func publishSnapshot() {
        snapshot = RendererProcessPoolSnapshot(
            processes: workers.values.map { worker in
                let state: RendererProcessState
                if worker.isTerminating {
                    state = .terminating
                } else if worker.isReady {
                    state = .ready
                } else {
                    state = .starting
                }
                return RendererProcessSnapshot(
                    id: worker.id,
                    pid: worker.process.processIdentifier,
                    fingerprint: worker.fingerprint,
                    tabIDs: worker.tabs,
                    state: state
                )
            }
        )
    }

    private func worker(for tabId: UUID) -> Worker? {
        guard let id = tabWorkerMap[tabId] else { return nil }
        return workers[id]
    }

    private func waitUntilReady(_ worker: Worker) async throws {
        let deadline = Date().addingTimeInterval(10)
        while !worker.isReady {
            guard worker.process.isRunning, !worker.isTerminating else {
                throw RendererProcessPoolError.rendererExited("process is not running")
            }
            guard Date() < deadline else {
                throw RendererProcessPoolError.rendererReadyTimeout
            }
            try await Task.sleep(for: .milliseconds(20))
        }
    }

    private func ensureOnePrewarmedRenderer() {
        guard workers.count < Self.recommendedProcessLimit,
              !workers.values.contains(where: {
                  $0.fingerprint == nil && $0.tabs.isEmpty && !$0.isTerminating
              }) else {
            return
        }

        do {
            _ = try launchWorker(fingerprint: nil)
        } catch {
            NSLog("Failed to prewarm Renderer: %@", error.localizedDescription)
            schedulePrewarmRetry()
        }
    }

    /// A broken Helper must not turn into a tight spawn/exit loop. Successful
    /// renderer_ready handshakes reset this backoff.
    private func schedulePrewarmRetry() {
        prewarmRetryWorkItem?.cancel()
        consecutivePrewarmFailures += 1
        let exponent = min(consecutivePrewarmFailures - 1, 5)
        let delay = min(30.0, pow(2.0, Double(exponent)))

        let workItem = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.prewarmRetryWorkItem = nil
            self.ensureOnePrewarmedRenderer()
        }
        prewarmRetryWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: workItem)
        NSLog(
            "Renderer prewarm retry scheduled in %.0f seconds (failure %d)",
            delay,
            consecutivePrewarmFailures
        )
    }

    private func evictIdleWorkerIfNeeded(for fingerprint: String) throws {
        guard workers.count >= Self.recommendedProcessLimit else { return }
        guard let victim = workers.values
            .filter({
                $0.tabs.isEmpty &&
                $0.id != systemWorkerId &&
                $0.fingerprint != fingerprint
            })
            .min(by: { $0.lastUsedAt < $1.lastUsedAt }) else {
            return
        }

        if let oldFingerprint = victim.fingerprint {
            fingerprintWorkerMap.removeValue(forKey: oldFingerprint)
        }
        workers.removeValue(forKey: victim.id)
        runningProcessCount = workers.count
        publishSnapshot()
        terminate(victim, reason: "Reclaimed idle Renderer capacity")
    }

    private func setupMemoryPressureHandling() {
        let source = DispatchSource.makeMemoryPressureSource(
            eventMask: [.warning, .critical],
            queue: .main
        )
        source.setEventHandler { [weak self, weak source] in
            guard let self, let source else { return }
            let keepPrewarm = !source.data.contains(.critical)
            let idleWorkers = self.workers.values.filter {
                $0.tabs.isEmpty && $0.id != self.systemWorkerId
            }
            for worker in idleWorkers where !keepPrewarm || worker.fingerprint != nil {
                if let fingerprint = worker.fingerprint {
                    self.fingerprintWorkerMap.removeValue(forKey: fingerprint)
                }
                self.workers.removeValue(forKey: worker.id)
                self.terminate(worker, reason: "Memory pressure")
            }
            self.runningProcessCount = self.workers.count
            self.publishSnapshot()
        }
        source.resume()
        memoryPressureSource = source
    }

    private func isSystemFingerprint(_ fingerprint: String) -> Bool {
        fingerprint.hasPrefix(Self.systemFingerprintPrefix)
    }

    private func fingerprintsMatch(_ current: String?, _ requested: String) -> Bool {
        Self.canShareRenderer(
            currentFingerprint: current,
            requestedFingerprint: requested
        )
    }

    private func nextViewportRequestId() -> UInt32 {
        defer {
            viewportRequestId = viewportRequestId == UInt32.max
                ? 1
                : viewportRequestId + 1
        }
        return viewportRequestId
    }
}
