import SwiftUI
import Combine

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
    case console = "Console"         // Console logs
    case operations = "Operations"   // Render operations log
    case network = "Network"         // Network requests (future)
    case performance = "Performance" // Performance metrics (future)

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .messages: return "arrow.left.arrow.right"
        case .elements: return "square.stack.3d.up"
        case .console: return "terminal"
        case .operations: return "list.bullet.rectangle"
        case .network: return "network"
        case .performance: return "gauge.with.dots.needle.bottom.50percent"
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

/// Operation entry for render operations log
struct DevToolsOperation: Identifiable {
    let id = UUID()
    let timestamp: Date
    let tabId: UUID
    let operationType: String
    let componentType: String?
    let nodeId: String?
    let props: [String: Any]?
    let duration: TimeInterval?
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

    // MARK: - Inspector State

    /// Currently inspected tab ID
    @Published var inspectedTabId: UUID?

    /// Component tree for the inspected tab
    @Published var componentTree: DevToolsComponentNode?

    /// Selected component node ID
    @Published var selectedNodeId: String?

    // MARK: - Logs

    /// Host-Guest message entries
    @Published private(set) var messages: [DevToolsMessage] = []

    /// Console log entries
    @Published private(set) var consoleLogs: [DevToolsLogEntry] = []

    /// Render operation entries
    @Published private(set) var operations: [DevToolsOperation] = []

    /// Maximum log entries to keep
    private let maxLogEntries = 1000

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

    // MARK: - Window Management

    /// Whether the detached window is currently open
    @Published var isDetachedWindowOpen: Bool = false

    /// Reference to the detached window (for programmatic control)
    weak var detachedWindow: NSWindow?

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
        // Don't record messages when engine is paused (freeze the log view)
        guard !isEnginePaused else { return }

        messages.append(message)
        // Trim if exceeds max
        if messages.count > maxLogEntries {
            messages.removeFirst(messages.count - maxLogEntries)
        }
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

        // Trim if exceeds max
        if consoleLogs.count > maxLogEntries {
            consoleLogs.removeFirst(consoleLogs.count - maxLogEntries)
        }
    }

    /// Add a render operation entry
    func logOperation(
        tabId: UUID,
        operationType: String,
        componentType: String? = nil,
        nodeId: String? = nil,
        props: [String: Any]? = nil,
        duration: TimeInterval? = nil
    ) {
        let operation = DevToolsOperation(
            timestamp: Date(),
            tabId: tabId,
            operationType: operationType,
            componentType: componentType,
            nodeId: nodeId,
            props: props,
            duration: duration
        )
        operations.append(operation)

        // Trim if exceeds max
        if operations.count > maxLogEntries {
            operations.removeFirst(operations.count - maxLogEntries)
        }
    }

    /// Clear console logs
    func clearConsoleLogs() {
        consoleLogs.removeAll()
    }

    /// Clear operation logs
    func clearOperations() {
        operations.removeAll()
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
            dockedHeight = max(Self.minDockedHeight, min(Self.maxDockedHeight, height))
        }

        let width = UserDefaults.standard.double(forKey: "devtools.dockedWidth")
        if width > 0 {
            dockedWidth = max(Self.minDockedWidth, min(Self.maxDockedWidth, width))
        }

        if let tabRaw = UserDefaults.standard.string(forKey: "devtools.activeTab"),
           let tab = DevToolsTab(rawValue: tabRaw) {
            activeTab = tab
        }
    }

    func saveSettings() {
        UserDefaults.standard.set(displayMode.rawValue, forKey: "devtools.displayMode")
        UserDefaults.standard.set(dockPosition.rawValue, forKey: "devtools.dockPosition")
        UserDefaults.standard.set(dockedHeight, forKey: "devtools.dockedHeight")
        UserDefaults.standard.set(dockedWidth, forKey: "devtools.dockedWidth")
        UserDefaults.standard.set(activeTab.rawValue, forKey: "devtools.activeTab")
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
