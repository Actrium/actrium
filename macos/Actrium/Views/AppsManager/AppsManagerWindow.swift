import SwiftUI
import AppKit
import Combine

// MARK: - Apps Manager Window Controller

/// Manages the Apps Manager window lifecycle
final class AppsManagerWindowController: NSObject, NSWindowDelegate {
    static let shared = AppsManagerWindowController()

    private var window: NSWindow?
    private var hostingView: NSHostingView<AppsManagerView>?

    private override init() {
        super.init()
    }

    func showWindow() {
        if let existingWindow = window, existingWindow.isVisible {
            existingWindow.makeKeyAndOrderFront(nil)
            return
        }

        let contentView = AppsManagerView()
        let hostingView = NSHostingView(rootView: contentView)
        self.hostingView = hostingView

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 650, height: 450),
            styleMask: [.titled, .closable, .resizable, .miniaturizable],
            backing: .buffered,
            defer: false
        )

        window.title = "App Manager"
        window.contentView = hostingView
        window.center()
        window.setFrameAutosaveName("AppsManagerWindow")
        window.minSize = NSSize(width: 400, height: 250)
        window.isReleasedWhenClosed = false

        // Use floating window style
        window.level = .floating
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        // Set delegate for Cmd-W support
        window.delegate = self

        self.window = window
        window.makeKeyAndOrderFront(nil)
    }

    func closeWindow() {
        window?.close()
    }

    // MARK: - NSWindowDelegate

    func windowWillClose(_ notification: Notification) {
        // Window is closing, clean up reference
        window = nil
        hostingView = nil
    }
}

// MARK: - Apps Manager View

struct AppsManagerView: View {
    @ObservedObject private var diagnosticsStore = RendererDiagnosticsStore.shared
    @ObservedObject private var devTools = DevToolsManager.shared
    @State private var selectedRowId: UUID?
    @State private var refreshTrigger: Bool = false

    // Timer for periodic refresh
    private let timer = Timer.publish(every: 2, on: .main, in: .common).autoconnect()

    var body: some View {
        VStack(spacing: 0) {
            // Header
            headerView

            Divider()

            // Process list
            processList

            Divider()

            // Footer with actions
            footerView
        }
        .frame(minWidth: 500, minHeight: 300)
        .background(Color(nsColor: .windowBackgroundColor))
        .onReceive(timer) { _ in
            refreshTrigger.toggle()
        }
    }

    // MARK: - Header

    private var headerView: some View {
        HStack {
            Image(systemName: "square.grid.2x2")
                .font(.system(size: 16))
                .foregroundColor(.secondary)

            Text("App Manager")
                .font(.system(size: 13, weight: .semibold))

            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    // MARK: - Process List

    private var processList: some View {
        let rows = buildProcessRows()

        return List(selection: $selectedRowId) {
            // Header row
            HStack(spacing: 0) {
                Text("App")
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text("PID")
                    .frame(width: 60, alignment: .trailing)
                Text("Memory")
                    .frame(width: 80, alignment: .trailing)
                Text("CPU")
                    .frame(width: 60, alignment: .trailing)
                Text("Status")
                    .frame(width: 80, alignment: .center)
            }
            .font(.system(size: 10, weight: .medium))
            .foregroundColor(.secondary)
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets(top: 4, leading: 12, bottom: 4, trailing: 12))

            ForEach(rows) { row in
                ProcessRowView(row: row, isSelected: selectedRowId == row.id)
                    .tag(row.id)
                    .listRowInsets(EdgeInsets(top: 2, leading: 8, bottom: 2, trailing: 8))
            }
        }
        .listStyle(.plain)
        .id(refreshTrigger) // Force refresh
    }

    // MARK: - Footer

    private var footerView: some View {
        let rows = buildProcessRows()

        return HStack {
            // Tab count
            Text("\(rows.count) app\(rows.count == 1 ? "" : "s")")
                .font(.system(size: 11))
                .foregroundColor(.secondary)

            Spacer()

            // End task button
            Button(action: endSelectedProcess) {
                Text("End Task")
            }
            .disabled(selectedRowId == nil)
            .keyboardShortcut(.delete, modifiers: [])
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    // MARK: - Helpers

    private func buildProcessRows() -> [ProcessRowData] {
        var rows: [ProcessRowData] = []

        // Get tabs from active BrowserViewModel
        guard let viewModel = devTools.activeBrowserViewModel else {
            return rows
        }

        // Add all tabs (Apps)
        for tab in viewModel.tabs {
            let tabId = tab.id

            // Get process info for this tab
            if let processInfo = diagnosticsStore.processInfo(for: tabId) {
                let metrics = processInfo.pid.flatMap {
                    diagnosticsStore.getProcessMetrics(pid: $0)
                }

                let memoryMB = Double(metrics?.memory ?? 0) / 1_048_576.0
                let cpuTime = Double(metrics?.cpuTime ?? 0) / 1_000_000_000.0

                let statusText: String
                switch processInfo.state {
                case .idle: statusText = "Idle"
                case .launching: statusText = "Starting"
                case .running: statusText = "Running"
                case .crashed: statusText = "Crashed"
                case .terminated: statusText = "Ended"
                }

                rows.append(ProcessRowData(
                    id: tabId,
                    name: tab.title.isEmpty ? "New Tab" : tab.title,
                    subtitle: processInfo.pid.map { "PID \($0)" } ?? "Renderer",
                    pid: processInfo.pid ?? 0,
                    memoryMB: memoryMB,
                    cpuPercent: cpuTime,
                    status: statusText
                ))
            } else {
                // No process data yet (tab hasn't spawned renderer)
                rows.append(ProcessRowData(
                    id: tabId,
                    name: tab.title.isEmpty ? "New Tab" : tab.title,
                    subtitle: nil,
                    pid: 0,
                    memoryMB: 0,
                    cpuPercent: 0,
                    status: "Pending"
                ))
            }
        }

        return rows
    }

    private func endSelectedProcess() {
        guard let processId = selectedRowId else { return }

        // Terminate the renderer process
        if diagnosticsStore.processInfo(for: processId) != nil {
            diagnosticsStore.terminateLegacyRenderer(for: processId)
        }

        // Close the tab through BrowserViewModel
        if let viewModel = devTools.activeBrowserViewModel {
            viewModel.closeTab(id: processId)
        }

        selectedRowId = nil
    }
}

// MARK: - Process Row View

private struct ProcessRowView: View {
    let row: ProcessRowData
    let isSelected: Bool

    var body: some View {
        HStack(spacing: 0) {
            // App name & icon
            HStack(spacing: 8) {
                processIcon
                Text(row.name)
                    .font(.system(size: 12))
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            // PID
            Text(row.pid > 0 ? "\(row.pid)" : "—")
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(.secondary)
                .frame(width: 60, alignment: .trailing)

            // Memory
            Text(formatMemory(row.memoryMB))
                .font(.system(size: 11, design: .monospaced))
                .frame(width: 80, alignment: .trailing)

            // CPU
            Text(row.cpuPercent > 0 ? String(format: "%.1f%%", row.cpuPercent) : "—")
                .font(.system(size: 11, design: .monospaced))
                .frame(width: 60, alignment: .trailing)

            // Status
            HStack(spacing: 4) {
                Circle()
                    .fill(statusColor(for: row.status))
                    .frame(width: 6, height: 6)
                Text(row.status)
                    .font(.system(size: 11))
            }
            .frame(width: 80, alignment: .center)
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .background(
            RoundedRectangle(cornerRadius: 4)
                .fill(isSelected ? Color.accentColor.opacity(0.2) : Color.clear)
        )
        .contentShape(Rectangle())
    }

    private var processIcon: some View {
        Image(systemName: "app.fill")
            .foregroundColor(.blue)
            .font(.system(size: 14))
            .frame(width: 20)
    }

    private func statusColor(for status: String) -> Color {
        switch status.lowercased() {
        case "running": return .green
        case "starting", "launching": return .orange
        case "crashed": return .red
        case "ended", "terminated": return .gray
        case "idle": return .yellow
        default: return .secondary
        }
    }

    private func formatMemory(_ mb: Double) -> String {
        if mb >= 1024 {
            return String(format: "%.1f GB", mb / 1024)
        } else if mb >= 1 {
            return String(format: "%.1f MB", mb)
        } else if mb > 0 {
            return String(format: "%.0f KB", mb * 1024)
        } else {
            return "—"
        }
    }
}

// MARK: - Process Row Data

struct ProcessRowData: Identifiable, Hashable {
    let id: UUID
    let name: String
    let subtitle: String?
    let pid: pid_t
    let memoryMB: Double
    let cpuPercent: Double
    let status: String

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }

    static func == (lhs: ProcessRowData, rhs: ProcessRowData) -> Bool {
        lhs.id == rhs.id
    }
}

// MARK: - Preview

#Preview {
    AppsManagerView()
        .frame(width: 650, height: 450)
}
