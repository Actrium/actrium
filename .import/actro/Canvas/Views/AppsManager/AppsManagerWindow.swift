import SwiftUI
import AppKit

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

        window.title = "Apps Manager"
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
    @State private var selectedProcessId: UUID?
    @State private var sortOrder: [KeyPathComparator<ProcessRowData>] = [
        .init(\.memoryMB, order: .reverse)
    ]

    var body: some View {
        VStack(spacing: 0) {
            // Header
            headerView

            Divider()

            // Process table
            if processManager.isMultiProcessEnabled {
                processTable
            } else {
                singleProcessView
            }

            Divider()

            // Footer with actions
            footerView
        }
        .frame(minWidth: 500, minHeight: 300)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    // MARK: - Header

    private var headerView: some View {
        HStack {
            Image(systemName: "cpu")
                .font(.system(size: 16))
                .foregroundColor(.secondary)

            Text("Apps Manager")
                .font(.system(size: 13, weight: .semibold))

            Spacer()

            // Refresh button
            Button(action: { /* Force refresh */ }) {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 12))
            }
            .buttonStyle(.borderless)
            .help("Refresh")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    // MARK: - Process Table

    private var processTable: some View {
        let rows = buildProcessRows()

        return Table(rows, selection: $selectedProcessId, sortOrder: $sortOrder) {
            TableColumn("Process", value: \.name) { row in
                HStack(spacing: 8) {
                    processIcon(for: row)
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
            }
            .width(min: 150, ideal: 200)

            TableColumn("PID", value: \.pid) { row in
                Text(row.pid > 0 ? "\(row.pid)" : "—")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(.secondary)
            }
            .width(60)

            TableColumn("Memory", value: \.memoryMB) { row in
                Text(formatMemory(row.memoryMB))
                    .font(.system(size: 11, design: .monospaced))
            }
            .width(80)

            TableColumn("CPU", value: \.cpuPercent) { row in
                Text(String(format: "%.1f%%", row.cpuPercent))
                    .font(.system(size: 11, design: .monospaced))
            }
            .width(60)

            TableColumn("Status", value: \.status) { row in
                HStack(spacing: 4) {
                    Circle()
                        .fill(statusColor(for: row.status))
                        .frame(width: 6, height: 6)
                    Text(row.status)
                        .font(.system(size: 11))
                }
            }
            .width(80)
        }
        .tableStyle(.inset(alternatesRowBackgrounds: true))
    }

    // MARK: - Single Process View (fallback)

    private var singleProcessView: some View {
        VStack(spacing: 16) {
            Image(systemName: "app.dashed")
                .font(.system(size: 40))
                .foregroundColor(.secondary)

            Text("Multi-Process Mode Disabled")
                .font(.headline)

            Text("All tabs run in the main process.\nEnable multi-process mode to see individual tab processes.")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)

            // Show main process info
            mainProcessInfo
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }

    private var mainProcessInfo: some View {
        let pid = getpid()
        let memory = getMainProcessMemory()

        return VStack(spacing: 8) {
            Divider()
                .padding(.vertical, 8)

            HStack(spacing: 24) {
                VStack {
                    Text("Main Process")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text("PID \(pid)")
                        .font(.system(size: 12, design: .monospaced))
                }

                VStack {
                    Text("Memory")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text(formatMemory(memory))
                        .font(.system(size: 12, design: .monospaced))
                }
            }
        }
    }

    // MARK: - Footer

    private var footerView: some View {
        HStack {
            // Process count
            let count = processManager.processes.count
            Text("\(count) process\(count == 1 ? "" : "es")")
                .font(.system(size: 11))
                .foregroundColor(.secondary)

            Spacer()

            // End process button
            Button(action: endSelectedProcess) {
                Text("End Process")
            }
            .disabled(selectedProcessId == nil)
            .keyboardShortcut(.delete, modifiers: [])
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    // MARK: - Helpers

    private func buildProcessRows() -> [ProcessRowData] {
        var rows: [ProcessRowData] = []

        // Main browser process
        let mainPid = getpid()
        let mainMemory = getMainProcessMemory()
        rows.append(ProcessRowData(
            id: UUID(),
            name: "Browser (Main)",
            subtitle: "Canvas",
            pid: mainPid,
            memoryMB: mainMemory,
            cpuPercent: 0,
            status: "Running",
            isMainProcess: true
        ))

        // Tab processes
        for (tabId, processInfo) in processManager.processes {
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
            case .crashed(let reason): statusText = "Crashed"
            case .terminated: statusText = "Ended"
            }

            rows.append(ProcessRowData(
                id: tabId,
                name: "Tab: \(tabId.uuidString.prefix(8))...",
                subtitle: "Renderer Process",
                pid: processInfo.pid ?? 0,
                memoryMB: memoryMB,
                cpuPercent: cpuTime, // Using CPU time as proxy
                status: statusText,
                isMainProcess: false
            ))
        }

        // Sort
        return rows.sorted(using: sortOrder)
    }

    private func processIcon(for row: ProcessRowData) -> some View {
        Group {
            if row.isMainProcess {
                Image(systemName: "app.fill")
                    .foregroundColor(.blue)
            } else {
                Image(systemName: "square.split.2x1")
                    .foregroundColor(.green)
            }
        }
        .font(.system(size: 14))
        .frame(width: 20)
    }

    private func statusColor(for status: String) -> Color {
        switch status.lowercased() {
        case "running": return .green
        case "starting", "launching": return .orange
        case "crashed": return .red
        case "ended", "terminated": return .gray
        default: return .secondary
        }
    }

    private func formatMemory(_ mb: Double) -> String {
        if mb >= 1024 {
            return String(format: "%.1f GB", mb / 1024)
        } else if mb >= 1 {
            return String(format: "%.1f MB", mb)
        } else {
            return String(format: "%.0f KB", mb * 1024)
        }
    }

    private func getMainProcessMemory() -> Double {
        var info = mach_task_basic_info()
        var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size) / 4
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
            }
        }
        if result == KERN_SUCCESS {
            return Double(info.resident_size) / 1_048_576.0
        }
        return 0
    }

    private func endSelectedProcess() {
        guard let processId = selectedProcessId else { return }

        // Don't allow ending main process
        if processManager.processes[processId] != nil {
            processManager.terminateProcess(for: processId)
            selectedProcessId = nil
        }
    }
}

// MARK: - Process Row Data

struct ProcessRowData: Identifiable {
    let id: UUID
    let name: String
    let subtitle: String?
    let pid: pid_t
    let memoryMB: Double
    let cpuPercent: Double
    let status: String
    let isMainProcess: Bool
}

// MARK: - Preview

#Preview {
    AppsManagerView()
        .frame(width: 650, height: 450)
}
