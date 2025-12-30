import SwiftUI
import AppKit
import Combine

// MARK: - Apps Manager Window Controller

/// Manages the Apps Manager window lifecycle
final class AppsManagerWindowController {
    static let shared = AppsManagerWindowController()

    private var window: NSWindow?
    private var hostingView: NSHostingView<AppsManagerView>?

    private init() {}

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

        window.title = "Task Manager"
        window.contentView = hostingView
        window.center()
        window.setFrameAutosaveName("AppsManagerWindow")
        window.minSize = NSSize(width: 500, height: 300)
        window.isReleasedWhenClosed = false

        // Use utility style for floating window
        window.level = .floating
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        self.window = window
        window.makeKeyAndOrderFront(nil)
    }

    func closeWindow() {
        window?.close()
    }
}

// MARK: - Apps Manager View

struct AppsManagerView: View {
    @ObservedObject private var processManager = TabProcessManager.shared
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
            Image(systemName: "cpu")
                .font(.system(size: 16))
                .foregroundColor(.secondary)

            Text("Task Manager")
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
                Text("Process")
                    .frame(width: 200, alignment: .leading)
                Text("PID")
                    .frame(width: 60, alignment: .trailing)
                Text("Memory")
                    .frame(width: 80, alignment: .trailing)
                Text("CPU")
                    .frame(width: 60, alignment: .trailing)
                Text("Status")
                    .frame(width: 80, alignment: .leading)
                    .padding(.leading, 8)
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

            // Check if we have process info for this tab
            if processManager.isMultiProcessEnabled,
               let processInfo = processManager.processes[tabId] {
                // Multi-process mode with actual process data
                let metrics = processInfo.pid.flatMap {
                    processManager.getProcessMetrics(pid: $0)
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
                // Single-process mode or no process data yet
                rows.append(ProcessRowData(
                    id: tabId,
                    name: tab.title.isEmpty ? "New Tab" : tab.title,
                    subtitle: nil,
                    pid: 0,
                    memoryMB: 0,
                    cpuPercent: 0,
                    status: "Running"
                ))
            }
        }

        return rows
    }

    private func endSelectedProcess() {
        guard let processId = selectedRowId else { return }

        // Close the tab through BrowserViewModel
        if let viewModel = devTools.activeBrowserViewModel {
            viewModel.closeTab(id: processId)
        }

        // Also terminate process if in multi-process mode
        if processManager.isMultiProcessEnabled,
           processManager.processes[processId] != nil {
            processManager.terminateProcess(for: processId)
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
            // Process name & icon
            HStack(spacing: 8) {
                processIcon
                VStack(alignment: .leading, spacing: 1) {
                    Text(row.name)
                        .font(.system(size: 12))
                        .lineLimit(1)
                    if let subtitle = row.subtitle {
                        Text(subtitle)
                            .font(.system(size: 10))
                            .foregroundColor(.secondary)
                            .lineLimit(1)
                    }
                }
            }
            .frame(width: 200, alignment: .leading)

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
            .frame(width: 80, alignment: .leading)
            .padding(.leading, 8)
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 4)
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
