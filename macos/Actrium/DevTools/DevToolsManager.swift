import SwiftUI
import Combine
import os.log

private let devToolsLog = OSLog(subsystem: "com.actro.devtools", category: "DevTools")

/// DevTools display mode
enum DevToolsDisplayMode: String, CaseIterable {
    case docked = "docked"      // Docked at the bottom of the main window
    case detached = "detached"  // Separate window
    case hidden = "hidden"      // Not visible
}

/// DevTools dock position (when docked)
enum DevToolsDockPosition: String, CaseIterable {
    case bottom = "bottom"
    case right = "right"
}

/// DevTools tab/panel type
enum DevToolsTab: String, CaseIterable, Identifiable {
    case messages = "Messages"       // Host-Guest message inspector
    case elements = "Elements"       // Component tree inspector
    case sources = "Sources"         // Source code viewer
    case console = "Console"         // Console logs
    case network = "Network"         // Network requests
    case monitor = "Monitor"         // Engine monitor
    case metro = "Metro"             // Metro bundler management

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .messages: return "arrow.left.arrow.right"
        case .elements: return "square.stack.3d.up"
        case .sources: return "doc.text"
        case .console: return "terminal"
        case .network: return "network"
        case .monitor: return "waveform.path.ecg"
        case .metro: return "shippingbox"
        }
    }
}

/// Direction of Host-Guest message
enum MessageDirection: String {
    case guestToHost = "guestToHost"  // Guest → Host (↓ down to host)
    case hostToGuest = "hostToGuest"  // Host → Guest (↑ up to guest)
}

/// Category of Host-Guest message
enum MessageCategory: String, CaseIterable {
    case render = "render"  // Render operations (CREATE, UPDATE, DELETE, TEXT)
    case app = "app"        // Application messages (APP_FETCH_REQUEST, etc.)

    var icon: String {
        switch self {
        case .render: return "hammer"
        case .app: return "envelope"
        }
    }

    var label: String {
        switch self {
        case .render: return "Render"
        case .app: return "App"
        }
    }
}

/// Host-Guest message entry for Messages inspector
struct DevToolsMessage: Identifiable {
    let id = UUID()
    let timestamp: Date
    let category: MessageCategory
    let direction: MessageDirection
    let event: String
    let payload: String?  // JSON string representation
    let tabId: String?
    let nodeId: String?   // For render operations
    let componentType: String?  // For render operations

    /// Arrow indicator based on direction
    var directionArrow: String {
        direction == .guestToHost ? "↓" : "↑"
    }

    /// Color based on direction
    var directionColor: String {
        direction == .guestToHost ? "blue" : "green"
    }
}

/// Log entry for console/operations
struct DevToolsLogEntry: Identifiable {
    let id = UUID()
    let timestamp: Date
    let level: LogLevel
    let source: String
    let message: String
    let details: String?

    enum LogLevel: String {
        case debug = "debug"
        case info = "info"
        case warn = "warn"
        case error = "error"
    }
}

// MARK: - Network

/// Network request state
enum DevToolsNetworkRequestState {
    case pending
    case completed
    case failed
}

/// Network request type
enum DevToolsNetworkRequestType: String, CaseIterable, Identifiable {
    case http = "HTTP"
    case websocket = "WS"
    case webrtc = "WebRTC"
    case other = "Other"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .http: return "arrow.up.arrow.down"
        case .websocket: return "bolt.horizontal"
        case .webrtc: return "video"
        case .other: return "questionmark.circle"
        }
    }
}

/// Network request (APP_FETCH_* lifecycle)
struct DevToolsNetworkRequest: Identifiable {
    let id = UUID()
    let requestId: String
    var tabId: String?
    var url: String
    var method: String
    var requestType: DevToolsNetworkRequestType = .http
    var startTime: Date
    var endTime: Date?
    var status: Int?
    var ok: Bool?
    var error: String?
    var requestHeaders: [String: String]?
    var responseHeaders: [String: String]?
    var requestBody: String?
    var responseBody: String?
    var state: DevToolsNetworkRequestState = .pending

    var duration: TimeInterval? {
        guard let endTime else { return nil }
        return endTime.timeIntervalSince(startTime)
    }

    var statusText: String {
        if error != nil { return "Error" }
        if let status { return "\(status)" }
        return "Pending"
    }

    var statusColor: SwiftUI.Color {
        switch state {
        case .pending:
            return .gray
        case .failed:
            return .red
        case .completed:
            if let status, status >= 400 { return .red }
            return .green
        }
    }

    var displayHost: String {
        URL(string: url)?.host ?? ""
    }

    var displayPathWithQuery: String {
        guard let components = URLComponents(string: url) else { return url }
        var path = components.path.isEmpty ? "/" : components.path
        if let query = components.percentEncodedQuery, !query.isEmpty {
            path += "?\(query)"
        }
        return path
    }

    var responseSizeBytes: Int? {
        responseBody?.utf8.count
    }
}

// MARK: - Monitor

enum DevToolsEngineMonitorTabStatus: String, Decodable {
    case ok
    case compat
    case blocked
}

struct DevToolsEngineMonitorSnapshot: Decodable {
    let timestamp: Double?
    let activeTabId: String?
    let rows: [DevToolsEngineMonitorRow]

    var date: Date? {
        guard let timestamp else { return nil }
        // JS uses Date.now() (ms)
        return Date(timeIntervalSince1970: timestamp / 1000.0)
    }
}

struct DevToolsEngineMonitorRow: Identifiable, Decodable {
    var id: String { tabId }

    let tabId: String
    let title: String
    let isActive: Bool
    let engineId: String?
    let status: DevToolsEngineMonitorTabStatus?
    let blockedReason: String?
    let contract: DevToolsEngineMonitorContract?
    let permissions: [String]?
    let diagnostics: DevToolsEngineMonitorDiagnostics?
}

struct DevToolsEngineMonitorContract: Decodable {
    let name: String
    let version: Int
}

struct DevToolsEngineMonitorDiagnostics: Decodable {
    struct Health: Decodable {
        let loaded: Bool?
        let destroyed: Bool?
        let errorCount: Int?
        let lastErrorAt: Double?
        let receiverNodes: Int?
        let batching: Bool?
    }

    struct Resources: Decodable {
        let timers: Int?
        let nodes: Int?
        let callbacks: Int?
    }

    struct Contracts: Decodable {
        let violations: Int?
        let lastViolationAt: Double?
        let lastViolation: String?
    }

    struct Activity: Decodable {
        struct LastBatch: Decodable {
            let batchId: Int?
            let at: Double?
            let totalOps: Int?
            let applyDurationMs: Double?
        }

        struct Timeline: Decodable {
            struct Point: Decodable {
                let at: Double?
                let ops: Int?
                let batches: Int?
                let skippedOps: Int?
                let applyDurationMsAvg: Double?
                let applyDurationMsMax: Double?
            }

            let windowMs: Double?
            let bucketMs: Double?
            let points: [Point]?
        }

        let windowMs: Double?
        let opsPerSecond: Double?
        let batchesPerSecond: Double?
        let totalBatches: Int?
        let totalOps: Int?
        let lastBatch: LastBatch?
        let timeline: Timeline?
    }

    struct Receiver: Decodable {
        struct LastApply: Decodable {
            let batchId: Int?
            let at: Double?
            let total: Int?
            let applied: Int?
            let skipped: Int?
            let failed: Int?
            let durationMs: Double?
            let nodeDelta: Int?
            let opCounts: [String: Int]?
            let skippedOpCounts: [String: Int]?
            let topNodeTypes: [TopNodeType]?
            let topNodeTypesSkipped: [TopNodeType]?
        }

        struct TopNodeType: Decodable, Identifiable {
            var id: String { type }
            let type: String
            let ops: Int
        }

        struct Attribution: Decodable {
            struct WorstBatch: Decodable, Identifiable {
                var id: String { "\(kind ?? "unknown")-\(batchId ?? -1)-\(at ?? 0)" }
                let kind: String?
                let batchId: Int?
                let at: Double?
                let total: Int?
                let applied: Int?
                let skipped: Int?
                let failed: Int?
                let durationMs: Double?
                let nodeDelta: Int?
            }

            let windowMs: Double?
            let sampleCount: Int?
            let total: Int?
            let applied: Int?
            let skipped: Int?
            let failed: Int?
            let durationMs: Double?
            let nodeDelta: Int?
            let opCounts: [String: Int]?
            let skippedOpCounts: [String: Int]?
            let topNodeTypes: [TopNodeType]?
            let topNodeTypesSkipped: [TopNodeType]?
            let worstBatches: [WorstBatch]?
        }

        let nodeCount: Int?
        let lastRenderAt: Double?
        let lastRenderDurationMs: Double?
        let lastApply: LastApply?
        let attribution: Attribution?
    }

    struct Endpoint: Decodable {
        let lastEventName: String?
        let lastEventAt: Double?
        let lastPayloadBytes: Int?
    }

    struct GuestEndpoint: Decodable {
        let lastEventName: String?
        let lastEventAt: Double?
        let lastPayloadBytes: Int?
        let sleeping: Bool?
        let sleepingAt: Double?
    }

    let id: String?
    let health: Health?
    let resources: Resources?
    let contracts: Contracts?
    let activity: Activity?
    let receiver: Receiver?
    let host: Endpoint?
    let guest: GuestEndpoint?
}

/// Component tree node for Elements inspector
struct DevToolsComponentNode: Identifiable {
    let id: String
    let type: String
    let props: [String: Any]
    let children: [DevToolsComponentNode]
    var isExpanded: Bool = false
    var isSelected: Bool = false
}

/// Box model data for selected node (margin, border, padding, content)
struct DevToolsBoxModel {
    var margin: EdgeInsets
    var border: EdgeInsets
    var padding: EdgeInsets
    var width: CGFloat
    var height: CGFloat
    var x: CGFloat
    var y: CGFloat

    struct EdgeInsets {
        var top: CGFloat
        var right: CGFloat
        var bottom: CGFloat
        var left: CGFloat

        static let zero = EdgeInsets(top: 0, right: 0, bottom: 0, left: 0)
    }

    static let empty = DevToolsBoxModel(
        margin: .zero,
        border: .zero,
        padding: .zero,
        width: 0,
        height: 0,
        x: 0,
        y: 0
    )
}

/// Computed styles for selected node
struct DevToolsComputedStyles {
    var position: String?
    var backgroundColor: String?
    var top: CGFloat?
    var left: CGFloat?
    var right: CGFloat?
    var bottom: CGFloat?
    var display: String?
    var flexDirection: String?
    var alignItems: String?
    var justifyContent: String?
    var flex: CGFloat?
    var opacity: CGFloat?
    var overflow: String?
}

/// DevTools state manager - shared between main window and detached window
@MainActor
class DevToolsManager: ObservableObject {
    static let shared = DevToolsManager()

    // MARK: - Display State

    /// Current display mode
    @Published var displayMode: DevToolsDisplayMode = .hidden

    /// Dock position when in docked mode
    @Published var dockPosition: DevToolsDockPosition = .bottom

    /// Height of the docked panel (when docked at bottom)
    @Published var dockedHeight: CGFloat = 280

    /// Width of the docked panel (when docked at right)
    @Published var dockedWidth: CGFloat = 400

    /// Currently active tab
    @Published var activeTab: DevToolsTab = .messages

    /// Custom tab order (for drag-and-drop reordering)
    @Published var tabOrder: [DevToolsTab] = DevToolsTab.allCases

    // MARK: - Inspector State

    /// Currently inspected tab ID
    @Published var inspectedTabId: UUID?

    /// Component tree for the inspected tab
    @Published var componentTree: DevToolsComponentNode?

    /// Selected component node ID
    @Published var selectedNodeId: String?

    /// Box model data for selected node
    @Published var boxModel: DevToolsBoxModel?

    /// Computed styles for selected node
    @Published var computedStyles: DevToolsComputedStyles?

    /// Expanded node IDs (for tree collapse/expand state)
    @Published var expandedNodeIds: Set<String> = []

    // MARK: - Source Navigation

    /// Pending source navigation request (for jumping to source from function props)
    @Published var pendingSourceNavigation: SourceNavigation?

    /// Guest app source directory (from current tab's URL)
    @Published var guestAppSourcePath: String?

    /// Guest app name (for display in file tree)
    @Published var guestAppName: String?

    /// Host source directory.
    static var hostSourcePath: String {
        ActriumRuntimePaths.actrHostSourcePath
    }

    struct SourceNavigation: Equatable {
        let searchText: String  // Function name for search fallback
        let fileName: String?   // Source file path (from Babel plugin)
        let lineNumber: Int?    // Source line number (from Babel plugin)
    }

    /// Update guest app source info from current tab
    func updateGuestAppSource(path: String?, name: String?) {
        guestAppSourcePath = path
        guestAppName = name
    }

    /// Navigate to source location (switches to Sources tab)
    func navigateToSource(functionName: String, fileName: String? = nil, lineNumber: Int? = nil) {

        pendingSourceNavigation = SourceNavigation(
            searchText: functionName,
            fileName: fileName,
            lineNumber: lineNumber
        )
        activeTab = .sources

    }

    /// Clear pending navigation after it's been handled
    func clearPendingSourceNavigation() {
        pendingSourceNavigation = nil
    }

    /// Whether a node is expanded
    func isNodeExpanded(_ nodeId: String) -> Bool {
        expandedNodeIds.contains(nodeId)
    }

    /// Toggle node expand/collapse state
    func toggleNodeExpanded(_ nodeId: String) {
        if expandedNodeIds.contains(nodeId) {
            expandedNodeIds.remove(nodeId)
        } else {
            expandedNodeIds.insert(nodeId)
        }
    }

    /// Expand a node
    func expandNode(_ nodeId: String) {
        expandedNodeIds.insert(nodeId)
    }

    /// Collapse a node
    func collapseNode(_ nodeId: String) {
        expandedNodeIds.remove(nodeId)
    }

    /// Expand all nodes
    func expandAllNodes() {
        guard let tree = componentTree else { return }
        collectAllNodeIds(tree, into: &expandedNodeIds)
    }

    /// Collapse all nodes
    func collapseAllNodes() {
        expandedNodeIds.removeAll()
    }

    private func collectAllNodeIds(_ node: DevToolsComponentNode, into set: inout Set<String>) {
        if !node.children.isEmpty {
            set.insert(node.id)
            for child in node.children {
                collectAllNodeIds(child, into: &set)
            }
        }
    }

    // MARK: - Logs

    /// Host-Guest message entries
    @Published private(set) var messages: [DevToolsMessage] = []

    /// Console log entries
    @Published private(set) var consoleLogs: [DevToolsLogEntry] = []

    /// Network requests
    @Published private(set) var networkRequests: [DevToolsNetworkRequest] = []

    /// Version counter for network requests (incremented on any change)
    @Published private(set) var networkRequestsVersion: Int = 0

    /// Engine monitor snapshot (pushed from Host)
    @Published private(set) var engineMonitorSnapshot: DevToolsEngineMonitorSnapshot?

    /// Last decode error for engine monitor snapshot (debug aid)
    @Published private(set) var engineMonitorDecodeError: String?

    /// Maximum log entries to keep
    private let maxLogEntries = 65535
    private let maxNetworkEntries = 1000

    /// Message filter (event name pattern)
    @Published var messageFilter: String = ""

    /// Direction filter for messages
    @Published var messageDirectionFilter: MessageDirection? = nil

    /// Category filter for messages
    @Published var messageCategoryFilter: MessageCategory? = nil

    // MARK: - Engine State

    /// Whether the current engine is paused (controlled from DevTools)
    @Published var isEnginePaused: Bool = false

    /// Callback to toggle engine pause state
    var onTogglePause: (() -> Void)?

    // MARK: - Inspect Mode

    /// Whether inspect mode is active (click to select components)
    @Published var isInspectMode: Bool = false

    /// Currently highlighted node ID (during hover in inspect mode)
    @Published var highlightedNodeId: String?

    /// Frame of the highlighted node (for overlay rendering)
    @Published var highlightFrame: CGRect?

    /// Callback to toggle inspect mode in RN
    var onToggleInspect: (() -> Void)?

    /// Callback to notify RN when a node is selected from the tree
    var onSelectNodeFromTree: ((String) -> Void)?

    /// Callback to toggle RN's built-in Element Inspector
    var onToggleRNInspector: (() -> Void)?

    /// Whether RN's built-in Inspector is active
    @Published var isRNInspectorActive: Bool = false

    // MARK: - Window Management

    /// Whether the detached window is currently open
    @Published var isDetachedWindowOpen: Bool = false

    /// Reference to the detached window (for programmatic control)
    weak var detachedWindow: NSWindow?

    /// Active browser viewModel for detached DevTools window
    /// This is set when the browser window is active and used by detached DevTools
    weak var activeBrowserViewModel: BrowserViewModel?

    // MARK: - Constraints

    static let minDockedHeight: CGFloat = 150
    static let maxDockedHeight: CGFloat = 600
    static let minDockedWidth: CGFloat = 300
    static let maxDockedWidth: CGFloat = 800

    private init() {
        // Load persisted settings
        loadSettings()
    }

    // MARK: - Display Mode Control

    /// Toggle DevTools visibility
    func toggle() {
        switch displayMode {
        case .hidden:
            displayMode = .docked
        case .docked, .detached:
            displayMode = .hidden
        }
    }

    /// Show DevTools (docked mode)
    func show() {
        if displayMode == .hidden {
            displayMode = .docked
        }
    }

    /// Hide DevTools
    func hide() {
        displayMode = .hidden
    }

    /// Detach to separate window
    func detach() {
        displayMode = .detached
        isDetachedWindowOpen = true
    }

    /// Dock back to main window
    func dock() {
        displayMode = .docked
        isDetachedWindowOpen = false
        // Close the detached window if it exists
        detachedWindow?.close()
    }

    /// Toggle dock position (bottom <-> right)
    func toggleDockPosition() {
        dockPosition = dockPosition == .bottom ? .right : .bottom
    }

    /// Toggle engine pause state
    func togglePause() {
        onTogglePause?()
    }

    /// Toggle inspect mode
    func toggleInspect() {
        onToggleInspect?()
    }

    /// Toggle RN's built-in Element Inspector
    func toggleRNInspector() {
        onToggleRNInspector?()
    }

    // MARK: - Messages

    /// Add an application message entry (APP_FETCH_REQUEST, etc.)
    func logMessage(
        direction: MessageDirection,
        event: String,
        payload: Any? = nil,
        tabId: String? = nil
    ) {
        let payloadString = formatPayload(payload)

        let message = DevToolsMessage(
            timestamp: Date(),
            category: .app,
            direction: direction,
            event: event,
            payload: payloadString,
            tabId: tabId,
            nodeId: nil,
            componentType: nil
        )
        appendMessage(message)
    }

    /// Add a render operation entry (CREATE, UPDATE, DELETE, TEXT)
    func logRenderOperation(
        direction: MessageDirection = .guestToHost,
        operationType: String,
        componentType: String? = nil,
        nodeId: String? = nil,
        props: [String: Any]? = nil,
        tabId: String? = nil
    ) {
        let payloadString = formatPayload(props)

        let message = DevToolsMessage(
            timestamp: Date(),
            category: .render,
            direction: direction,
            event: operationType,
            payload: payloadString,
            tabId: tabId,
            nodeId: nodeId,
            componentType: componentType
        )
        appendMessage(message)
    }

    private func formatPayload(_ payload: Any?) -> String? {
        guard let payload = payload else { return nil }
        if let data = try? JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted, .sortedKeys]),
           let str = String(data: data, encoding: .utf8) {
            return str
        }
        return String(describing: payload)
    }

    private func appendMessage(_ message: DevToolsMessage) {
        // Use asyncAfter to avoid "Publishing changes from within view updates" warning
        DispatchQueue.main.asyncAfter(deadline: .now()) { [weak self] in
            guard let self = self else { return }
            // Don't record messages when engine is paused (freeze the log view)
            guard !self.isEnginePaused else { return }

            self.messages.append(message)
            // Trim if exceeds max
            if self.messages.count > self.maxLogEntries {
                self.messages.removeFirst(self.messages.count - self.maxLogEntries)
            }

            self.processNetworkMessage(message)
        }
    }

    func updateEngineMonitorSnapshot(_ snapshotJson: String?) {
        guard let snapshotJson, !snapshotJson.isEmpty else {
            engineMonitorSnapshot = nil
            engineMonitorDecodeError = nil
            return
        }

        guard let data = snapshotJson.data(using: .utf8) else {
            engineMonitorDecodeError = "Invalid UTF-8 data"
            return
        }

        do {
            let snapshot = try JSONDecoder().decode(DevToolsEngineMonitorSnapshot.self, from: data)
            engineMonitorSnapshot = snapshot
            engineMonitorDecodeError = nil
        } catch {
            engineMonitorDecodeError = error.localizedDescription
        }
    }

    private func processNetworkMessage(_ message: DevToolsMessage) {
        switch message.event {
        case "APP_FETCH_REQUEST":
            guard let payload = message.payload,
                  let dict = parseJSONDictionary(payload),
                  let requestId = dict["requestId"] as? String,
                  let url = dict["url"] as? String else { return }

            let options = dict["options"]
            let method = (options as? [String: Any])?["method"] as? String ?? "GET"
            let headers = normalizeHeaders((options as? [String: Any])?["headers"])
            let body = normalizeBody((options as? [String: Any])?["body"])

            upsertNetworkRequestStart(
                requestId: requestId,
                tabId: message.tabId,
                url: url,
                method: method,
                startTime: message.timestamp,
                requestHeaders: headers,
                requestBody: body
            )
        case "APP_FETCH_RESPONSE":
            guard let payload = message.payload,
                  let dict = parseJSONDictionary(payload),
                  let requestId = dict["requestId"] as? String else { return }

            let ok = dict["ok"] as? Bool
            let status = dict["status"] as? Int
            let error = dict["error"] as? String
            let responseHeaders = normalizeHeaders(dict["headers"])
            let responseBody = normalizeBody(dict["data"]) ?? error

            upsertNetworkRequestEnd(
                requestId: requestId,
                tabId: message.tabId,
                endTime: message.timestamp,
                ok: ok,
                status: status,
                error: error,
                responseHeaders: responseHeaders,
                responseBody: responseBody
            )
        default:
            return
        }
    }

    private func upsertNetworkRequestStart(
        requestId: String,
        tabId: String?,
        url: String,
        method: String,
        startTime: Date,
        requestHeaders: [String: String]?,
        requestBody: String?
    ) {
        if let index = networkRequests.firstIndex(where: { $0.requestId == requestId && ($0.tabId == tabId || tabId == nil || $0.tabId == nil) }) {
            var req = networkRequests[index]
            // Fill missing info for placeholders created from response.
            req.tabId = req.tabId ?? tabId
            if req.url == "unknown" { req.url = url }
            if req.method == "GET" { req.method = method }
            // Only move startTime earlier.
            if startTime < req.startTime { req.startTime = startTime }
            req.requestHeaders = requestHeaders ?? req.requestHeaders
            req.requestBody = requestBody ?? req.requestBody
            if req.endTime == nil { req.state = .pending }
            networkRequests[index] = req
            networkRequestsVersion += 1
            return
        }

        let req = DevToolsNetworkRequest(
            requestId: requestId,
            tabId: tabId,
            url: url,
            method: method,
            startTime: startTime,
            requestHeaders: requestHeaders,
            requestBody: requestBody
        )
        networkRequests.append(req)
        networkRequestsVersion += 1
        trimNetworkIfNeeded()
    }

    private func upsertNetworkRequestEnd(
        requestId: String,
        tabId: String?,
        endTime: Date,
        ok: Bool?,
        status: Int?,
        error: String?,
        responseHeaders: [String: String]?,
        responseBody: String?
    ) {
        if let index = networkRequests.firstIndex(where: { $0.requestId == requestId && ($0.tabId == tabId || tabId == nil || $0.tabId == nil) }) {
            var req = networkRequests[index]
            req.endTime = endTime
            req.ok = ok
            req.status = status
            req.error = error
            req.responseHeaders = responseHeaders ?? req.responseHeaders
            req.responseBody = responseBody ?? req.responseBody
            req.state = (error != nil || ok == false || (status ?? 0) >= 400) ? .failed : .completed
            networkRequests[index] = req
            networkRequestsVersion += 1
            return
        }

        // Response arrived before request start log (or request start missing) - create a placeholder.
        var req = DevToolsNetworkRequest(
            requestId: requestId,
            tabId: tabId,
            url: "unknown",
            method: "GET",
            startTime: endTime
        )
        req.endTime = endTime
        req.ok = ok
        req.status = status
        req.error = error
        req.responseHeaders = responseHeaders
        req.responseBody = responseBody
        req.state = (error != nil || ok == false || (status ?? 0) >= 400) ? .failed : .completed
        networkRequests.append(req)
        networkRequestsVersion += 1
        trimNetworkIfNeeded()
    }

    private func trimNetworkIfNeeded() {
        if networkRequests.count > maxNetworkEntries {
            networkRequests.removeFirst(networkRequests.count - maxNetworkEntries)
        }
    }

    private func parseJSONDictionary(_ json: String) -> [String: Any]? {
        guard let data = json.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    private func normalizeHeaders(_ headers: Any?) -> [String: String]? {
        guard let headers else { return nil }
        if let dict = headers as? [String: Any] {
            var result: [String: String] = [:]
            for (key, value) in dict {
                result[key] = normalizeScalarString(value) ?? String(describing: value)
            }
            return result.isEmpty ? nil : result
        }
        if let dict = headers as? [String: String] {
            return dict.isEmpty ? nil : dict
        }
        // React Native / fetch may pass headers as array of pairs.
        if let array = headers as? [[Any]] {
            var result: [String: String] = [:]
            for pair in array {
                guard pair.count >= 2 else { continue }
                let key = normalizeScalarString(pair[0]) ?? String(describing: pair[0])
                let value = normalizeScalarString(pair[1]) ?? String(describing: pair[1])
                result[key] = value
            }
            return result.isEmpty ? nil : result
        }
        return nil
    }

    private func normalizeBody(_ body: Any?) -> String? {
        guard let body else { return nil }
        if let str = body as? String {
            return prettyPrintedJSONStringIfPossible(str) ?? str
        }
        // JSON objects/arrays -> pretty JSON
        if JSONSerialization.isValidJSONObject(body),
           let data = try? JSONSerialization.data(withJSONObject: body, options: [.prettyPrinted, .sortedKeys]),
           let str = String(data: data, encoding: .utf8) {
            return str
        }
        return String(describing: body)
    }

    private func normalizeScalarString(_ value: Any?) -> String? {
        if let str = value as? String { return str }
        if let num = value as? NSNumber { return num.stringValue }
        if let bool = value as? Bool { return bool ? "true" : "false" }
        return nil
    }

    private func prettyPrintedJSONStringIfPossible(_ string: String) -> String? {
        guard let data = string.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data),
              JSONSerialization.isValidJSONObject(json),
              let outData = try? JSONSerialization.data(withJSONObject: json, options: [.prettyPrinted, .sortedKeys]),
              let outString = String(data: outData, encoding: .utf8) else {
            return nil
        }
        return outString
    }

    /// Get filtered messages
    var filteredMessages: [DevToolsMessage] {
        messages.filter { msg in
            // Category filter
            if let catFilter = messageCategoryFilter, msg.category != catFilter {
                return false
            }
            // Direction filter
            if let dirFilter = messageDirectionFilter, msg.direction != dirFilter {
                return false
            }
            // Event name filter
            if !messageFilter.isEmpty {
                let searchText = messageFilter.lowercased()
                let matchesEvent = msg.event.lowercased().contains(searchText)
                let matchesComponent = msg.componentType?.lowercased().contains(searchText) ?? false
                let matchesNode = msg.nodeId?.lowercased().contains(searchText) ?? false
                return matchesEvent || matchesComponent || matchesNode
            }
            return true
        }
    }

    /// Clear all messages
    func clearMessages() {
        messages.removeAll()
    }

    /// Evaluate JavaScript in the active tab (via XPC service)
    func evaluateScript(_ code: String) {
        guard let tabId = inspectedTabId else {
            log("No active tab to evaluate script", level: .error, source: "DevTools")
            return
        }
        // Send to XPC service via DistributedNotificationCenter
        let userInfo: [String: Any] = [
            "tabId": tabId.uuidString,
            "code": code
        ]
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.actro.evaluateScript"),
            object: nil,
            userInfo: userInfo,
            deliverImmediately: true
        )
    }

    // MARK: - Logging

    /// Add a console log entry
    func log(_ message: String, level: DevToolsLogEntry.LogLevel = .info, source: String = "System", details: String? = nil) {
        let entry = DevToolsLogEntry(
            timestamp: Date(),
            level: level,
            source: source,
            message: message,
            details: details
        )
        consoleLogs.append(entry)

        // Trim if exceeds max, but do it in chunks to avoid frequent array shifts
        if consoleLogs.count > maxLogEntries + 100 {
            consoleLogs.removeFirst(consoleLogs.count - maxLogEntries)
        }
    }

    /// Clear console logs
    func clearConsoleLogs() {
        consoleLogs.removeAll()
    }

    /// Log from a renderer process (multi-process architecture)
    func logFromRenderer(tabId: UUID, level: String, message: String, source: String) {
        let logLevel: DevToolsLogEntry.LogLevel
        switch level.lowercased() {
        case "debug": logLevel = .debug
        case "warn", "warning": logLevel = .warn
        case "error": logLevel = .error
        default: logLevel = .info
        }
        log(message, level: logLevel, source: "[\(tabId.uuidString.prefix(8))] \(source)")
    }

    /// Clear network requests
    func clearNetwork() {
        networkRequests.removeAll()
        networkRequestsVersion += 1
    }

    // MARK: - Component Tree

    /// Update the component tree for a tab
    func updateComponentTree(_ tree: DevToolsComponentNode?, for tabId: UUID) {
        if inspectedTabId == tabId {
            componentTree = tree
        }
    }

    /// Select a component node
    func selectNode(_ nodeId: String?) {
        selectedNodeId = nodeId
        // Clear box model when deselecting
        if nodeId == nil {
            boxModel = nil
            computedStyles = nil
        }
        // Notify RN to highlight the selected node
        if let nodeId = nodeId {
            onSelectNodeFromTree?(nodeId)
        }
    }

    /// Update box model data for the selected node
    func updateBoxModel(
        margin: (top: CGFloat, right: CGFloat, bottom: CGFloat, left: CGFloat),
        padding: (top: CGFloat, right: CGFloat, bottom: CGFloat, left: CGFloat),
        border: (top: CGFloat, right: CGFloat, bottom: CGFloat, left: CGFloat) = (0, 0, 0, 0),
        width: CGFloat,
        height: CGFloat,
        x: CGFloat,
        y: CGFloat
    ) {
        boxModel = DevToolsBoxModel(
            margin: DevToolsBoxModel.EdgeInsets(top: margin.top, right: margin.right, bottom: margin.bottom, left: margin.left),
            border: DevToolsBoxModel.EdgeInsets(top: border.top, right: border.right, bottom: border.bottom, left: border.left),
            padding: DevToolsBoxModel.EdgeInsets(top: padding.top, right: padding.right, bottom: padding.bottom, left: padding.left),
            width: width,
            height: height,
            x: x,
            y: y
        )
    }

    /// Update computed styles for the selected node
    func updateComputedStyles(
        position: String? = nil,
        backgroundColor: String? = nil,
        top: CGFloat? = nil,
        left: CGFloat? = nil,
        right: CGFloat? = nil,
        bottom: CGFloat? = nil,
        display: String? = nil,
        flexDirection: String? = nil,
        alignItems: String? = nil,
        justifyContent: String? = nil,
        flex: CGFloat? = nil,
        opacity: CGFloat? = nil,
        overflow: String? = nil
    ) {
        var styles = DevToolsComputedStyles()
        styles.position = position
        styles.backgroundColor = backgroundColor
        styles.top = top
        styles.left = left
        styles.right = right
        styles.bottom = bottom
        styles.display = display
        styles.flexDirection = flexDirection
        styles.alignItems = alignItems
        styles.justifyContent = justifyContent
        styles.flex = flex
        styles.opacity = opacity
        styles.overflow = overflow
        computedStyles = styles
    }

    // MARK: - Persistence

    private func loadSettings() {
        // DevTools always starts hidden - user must manually open it
        // We only restore position/size settings, not display mode
        // (Previously restored mode caused DevTools to open on every launch)

        if let positionRaw = UserDefaults.standard.string(forKey: "devtools.dockPosition"),
           let position = DevToolsDockPosition(rawValue: positionRaw) {
            dockPosition = position
        }

        let height = UserDefaults.standard.double(forKey: "devtools.dockedHeight")
        if height > 0 {
            // 🔑 & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & &
            dockedHeight = round(max(Self.minDockedHeight, min(Self.maxDockedHeight, height)))
        }

        let width = UserDefaults.standard.double(forKey: "devtools.dockedWidth")
        if width > 0 {
            // 🔑 & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & &
            dockedWidth = round(max(Self.minDockedWidth, min(Self.maxDockedWidth, width)))
        }

        if let tabRaw = UserDefaults.standard.string(forKey: "devtools.activeTab"),
           let tab = DevToolsTab(rawValue: tabRaw) {
            activeTab = tab
        }

        // Load custom tab order
        if let savedOrder = UserDefaults.standard.stringArray(forKey: "devtools.tabOrder") {
            let loadedTabs = savedOrder.compactMap { DevToolsTab(rawValue: $0) }
            // Ensure all tabs are present (in case new tabs were added)
            let existingTabs = Set(loadedTabs)
            let missingTabs = DevToolsTab.allCases.filter { !existingTabs.contains($0) }
            tabOrder = loadedTabs + missingTabs
        }
    }

    func saveSettings() {
        UserDefaults.standard.set(displayMode.rawValue, forKey: "devtools.displayMode")
        UserDefaults.standard.set(dockPosition.rawValue, forKey: "devtools.dockPosition")
        UserDefaults.standard.set(dockedHeight, forKey: "devtools.dockedHeight")
        UserDefaults.standard.set(dockedWidth, forKey: "devtools.dockedWidth")
        UserDefaults.standard.set(activeTab.rawValue, forKey: "devtools.activeTab")
        UserDefaults.standard.set(tabOrder.map { $0.rawValue }, forKey: "devtools.tabOrder")
    }

    /// Move a tab from one position to another
    func moveTab(from source: Int, to destination: Int) {
        guard source != destination,
              source >= 0 && source < tabOrder.count,
              destination >= 0 && destination <= tabOrder.count else { return }

        let tab = tabOrder.remove(at: source)
        let adjustedDestination = destination > source ? destination - 1 : destination
        tabOrder.insert(tab, at: adjustedDestination)
        saveSettings()
    }
}

// MARK: - Keyboard Shortcuts

extension DevToolsManager {
    /// Handle keyboard shortcut (Cmd+Option+I)
    static func handleToggleShortcut() {
        Task { @MainActor in
            DevToolsManager.shared.toggle()
        }
    }
}
