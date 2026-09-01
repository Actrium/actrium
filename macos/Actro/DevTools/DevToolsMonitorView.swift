import SwiftUI
import AppKit
import Combine

// MARK: - System Metrics Helper

@MainActor
private final class SystemMetrics: ObservableObject {
    static let shared = SystemMetrics()

    @Published var memoryMB: Double = 0

    // Historical data for sparklines (last 60 samples, 1 per second)
    @Published var memoryHistory: [Double] = []

    private let maxHistoryCount = 60
    private var timer: Timer?

    private init() {
        update()
        timer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.update()
        }
    }

    private func update() {
        // Memory usage
        var info = mach_task_basic_info()
        var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size) / 4
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
            }
        }
        if result == KERN_SUCCESS {
            memoryMB = Double(info.resident_size) / 1_048_576.0
            memoryHistory.append(memoryMB)
            if memoryHistory.count > maxHistoryCount {
                memoryHistory.removeFirst()
            }
        }
    }
}

// MARK: - Monitor View

struct DevToolsMonitorView: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject private var diagnosticsStore = RendererDiagnosticsStore.shared
    @StateObject private var systemMetrics = SystemMetrics.shared

    @State private var isFrozen: Bool = false
    @State private var frozenSnapshot: DevToolsEngineMonitorSnapshot?
    @State private var showDetails: Bool = false
    @State private var showMultiProcessTest: Bool = false
    @State private var testTab: Tab = Tab(title: "Test Tab")

    private var snapshot: DevToolsEngineMonitorSnapshot? {
        isFrozen ? frozenSnapshot : devTools.engineMonitorSnapshot
    }

    private var rows: [DevToolsEngineMonitorRow] {
        snapshot?.rows ?? []
    }

    private var currentRow: DevToolsEngineMonitorRow? {
        if let tabId = viewModel.currentTabId?.uuidString,
           let row = rows.first(where: { $0.tabId == tabId }) {
            return row
        }
        return rows.first
    }

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            Divider()

            if let error = devTools.engineMonitorDecodeError, !error.isEmpty {
                errorBanner(error)
                Divider()
            }

            if rows.isEmpty {
                emptyState
            } else if let row = currentRow {
                metricsContent(row: row)
            } else {
                noDataState
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
        .onChange(of: isFrozen) { _, frozen in
            frozenSnapshot = frozen ? devTools.engineMonitorSnapshot : nil
        }
    }

    // MARK: - Toolbar

    private var toolbar: some View {
        HStack(spacing: 8) {
            Text(viewModel.currentTab?.title ?? "Current Tab")
                .font(.system(size: 12, weight: .semibold))
                .lineLimit(1)

            if let row = currentRow {
                Text("TabId: \(row.tabId)")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(.secondary)
                    .textSelection(.enabled)
                if let engineId = row.engineId {
                    Text("EngineId: \(engineId)")
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundColor(.secondary)
                        .textSelection(.enabled)
                }
            }

            Spacer()

            if let date = snapshot?.date {
                Text("Updated: \(date, format: .dateTime.hour().minute().second())")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(Color(nsColor: .tertiaryLabelColor))
            } else {
                Text(isFrozen ? "Frozen" : "Waiting…")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(Color(nsColor: .tertiaryLabelColor))
            }

            Toggle("Freeze", isOn: $isFrozen)
                .toggleStyle(.switch)
                .font(.system(size: 10))

            Divider()
                .frame(height: 16)

            // Multi-process mode indicator
            HStack(spacing: 4) {
                Circle()
                    .fill(Color.green)
                    .frame(width: 6, height: 6)
                Text("Multi-Process")
                    .font(.system(size: 10))
                    .foregroundColor(.secondary)
            }

            // Test button for multi-process rendering
            Button(action: {
                testTab = Tab(title: "Test Tab")
                showMultiProcessTest = true
            }) {
                Label("Test", systemImage: "play.circle")
                    .font(.system(size: 10))
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(Color(nsColor: .controlBackgroundColor))
        .sheet(isPresented: $showMultiProcessTest) {
            multiProcessTestSheet
        }
    }

    // MARK: - Multi-Process Test Sheet

    private var multiProcessTestSheet: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("Multi-Process Renderer Test")
                    .font(.headline)
                Spacer()
                Button("Close") {
                    showMultiProcessTest = false
                }
                .buttonStyle(.bordered)
            }
            .padding()
            .background(Color(nsColor: .controlBackgroundColor))

            Divider()

            // Content - the IOSurface view
            MultiProcessTabView(
                tab: testTab,
                containerSize: CGSize(width: 600, height: 400)
            )
            .frame(minWidth: 600, minHeight: 400)
        }
        .frame(width: 700, height: 500)
    }

    // MARK: - Metrics Content

    private func metricsContent(row: DevToolsEngineMonitorRow) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                // Warnings banner
                let warnings = row.monitorWarnings
                if !warnings.isEmpty {
                    warningsBanner(warnings)
                }

                // Primary metrics cards (flow layout)
                primaryMetricsGrid(row: row)

                // Secondary metrics cards (flow layout)
                secondaryMetricsGrid(row: row)

                // Task Manager section
                taskManagerSection

                // Expandable details
                DisclosureGroup("Details", isExpanded: $showDetails) {
                    detailsContent(row: row)
                }
                .font(.system(size: 11, weight: .medium))
                .padding(8)
                .background(Color(nsColor: .controlBackgroundColor).opacity(0.3))
                .cornerRadius(6)
            }
            .padding(10)
        }
    }

    // MARK: - Primary Metrics Grid

    private func primaryMetricsGrid(row: DevToolsEngineMonitorRow) -> some View {
        let columns = [GridItem(.adaptive(minimum: 120, maximum: 160), spacing: 8)]

        return LazyVGrid(columns: columns, spacing: 8) {
            // FPS (placeholder - using ops/s as proxy for now)
            MonitorMetricCard(
                icon: "speedometer",
                title: "FPS",
                value: "—",
                subtitle: "Not available",
                status: .neutral
            )

            // Memory
            MonitorMetricCard(
                icon: "memorychip",
                title: "Memory",
                value: String(format: "%.0f", systemMetrics.memoryMB),
                subtitle: "MB",
                status: memoryStatus(systemMetrics.memoryMB),
                history: systemMetrics.memoryHistory,
                historyColor: .purple
            )

            // Apply Time (render performance for current tab)
            MonitorMetricCard(
                icon: "clock.arrow.circlepath",
                title: "Apply",
                value: row.monitorApplyMsString,
                subtitle: "ms",
                status: applyStatus(row.monitorApplyMs),
                historyColor: .cyan
            )

            // Network pending
            let pendingRequests = devTools.networkRequests.filter { $0.state == .pending }.count
            MonitorMetricCard(
                icon: "network",
                title: "Network",
                value: "\(pendingRequests)",
                subtitle: "pending",
                status: pendingRequests > 5 ? .warning : .good
            )
        }
    }

    // MARK: - Secondary Metrics Grid

    private func secondaryMetricsGrid(row: DevToolsEngineMonitorRow) -> some View {
        let columns = [GridItem(.adaptive(minimum: 90, maximum: 120), spacing: 8)]

        return LazyVGrid(columns: columns, spacing: 8) {
            // Nodes
            MonitorMetricCard(
                icon: "square.stack.3d.up",
                title: "Nodes",
                value: "\(row.monitorNodes)",
                subtitle: "/ 5000",
                status: row.monitorNodes > 5000 ? .error : (row.monitorNodes > 3000 ? .warning : .good),
                compact: true
            )

            // Timers
            MonitorMetricCard(
                icon: "timer",
                title: "Timers",
                value: "\(row.monitorTimers)",
                subtitle: "/ 50",
                status: row.monitorTimers > 50 ? .error : (row.monitorTimers > 30 ? .warning : .good),
                compact: true
            )

            // Callbacks
            MonitorMetricCard(
                icon: "arrow.turn.down.right",
                title: "Callbacks",
                value: "\(row.monitorCallbacks)",
                subtitle: "/ 2000",
                status: row.monitorCallbacks > 2000 ? .error : (row.monitorCallbacks > 1500 ? .warning : .good),
                compact: true
            )

            // Ops/sec
            MonitorMetricCard(
                icon: "bolt",
                title: "Ops/s",
                value: row.monitorOpsPerSecondString,
                subtitle: "",
                status: .neutral,
                compact: true
            )

            // Skipped
            let skipped = row.monitorSkipped
            MonitorMetricCard(
                icon: "exclamationmark.triangle",
                title: "Skipped",
                value: "\(skipped)",
                subtitle: "",
                status: skipped > 0 ? .warning : .good,
                compact: true
            )
        }
    }

    // MARK: - Task Manager Section

    @State private var showTaskManager: Bool = true

    private var taskManagerSection: some View {
        DisclosureGroup("Task Manager", isExpanded: $showTaskManager) {
            VStack(spacing: 8) {
                // Process Pool Status Bar
                processPoolStatusBar

                // Process table
                VStack(spacing: 0) {
                    // Header row
                    HStack(spacing: 0) {
                        Text("Tab")
                            .frame(width: 120, alignment: .leading)
                        Text("PID")
                            .frame(width: 60, alignment: .trailing)
                        Text("Memory")
                            .frame(width: 80, alignment: .trailing)
                        Text("CPU")
                            .frame(width: 60, alignment: .trailing)
                        Text("Status")
                            .frame(width: 80, alignment: .center)
                        Spacer()
                        Text("Action")
                            .frame(width: 60, alignment: .center)
                    }
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundColor(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(Color(nsColor: .controlBackgroundColor))

                    Divider()

                    // Process rows
                    let processes = diagnosticsStore.allLegacyProcesses()
                    if processes.isEmpty {
                        Text("No tab processes running")
                            .font(.system(size: 11))
                            .foregroundColor(.secondary)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 12)
                    } else {
                        ForEach(processes, id: \.tabId) { processInfo in
                            TaskManagerRow(processInfo: processInfo)
                            Divider()
                        }
                    }
                }
                .background(Color(nsColor: .textBackgroundColor))
                .cornerRadius(6)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(Color(nsColor: .separatorColor), lineWidth: 0.5)
                )
            }
        }
        .font(.system(size: 11, weight: .medium))
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.3))
        .cornerRadius(6)
    }

    private var processPoolStatusBar: some View {
        HStack(spacing: 12) {
            // XPC Service status
            HStack(spacing: 4) {
                Image(systemName: "network")
                    .font(.system(size: 10))
                Text("XPC Service")
                    .font(.system(size: 10, weight: .medium))
            }
            .foregroundColor(.secondary)

            Spacer()

            // Kill all button
            Button(action: {
                diagnosticsStore.terminateAllLegacyRenderers()
            }) {
                HStack(spacing: 4) {
                    Image(systemName: "xmark.circle")
                    Text("Kill All")
                }
                .font(.system(size: 10))
                .foregroundColor(.red)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
        .cornerRadius(6)
    }

    // MARK: - Details Content

    private func detailsContent(row: DevToolsEngineMonitorRow) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            detailSection(title: "Health", items: row.monitorHealthItems)
            detailSection(title: "Receiver", items: row.monitorReceiverItems)
            detailSection(title: "Host / Guest", items: row.monitorHostGuestItems)
            detailSection(title: "Compliance", items: row.monitorComplianceItems)
        }
        .padding(.top, 8)
    }

    private func detailSection(title: String, items: [(String, String)]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.system(size: 10, weight: .bold))
                .foregroundColor(.secondary)

            MonitorFlowLayout(spacing: 6) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(spacing: 2) {
                        Text(item.0 + ":")
                            .font(.system(size: 10))
                            .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                        Text(item.1)
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundColor(.secondary)
                            .textSelection(.enabled)
                    }
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
                    .cornerRadius(4)
                }
            }
        }
    }

    // MARK: - Status Helpers

    private func memoryStatus(_ mb: Double) -> MonitorMetricStatus {
        if mb > 500 { return .error }
        if mb > 200 { return .warning }
        return .good
    }

    private func applyStatus(_ ms: Double?) -> MonitorMetricStatus {
        guard let ms else { return .neutral }
        if ms > 50 { return .error }
        if ms > 30 { return .warning }
        return .good
    }

    // MARK: - Banners & States

    private func errorBanner(_ error: String) -> some View {
        Text("Monitor decode error: \(error)")
            .font(.system(size: 11, design: .monospaced))
            .foregroundColor(.red)
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.red.opacity(0.06))
    }

    private func warningsBanner(_ warnings: [String]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(warnings, id: \.self) { w in
                HStack(spacing: 4) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .font(.system(size: 10))
                        .foregroundColor(.orange)
                    Text(w)
                        .font(.system(size: 11))
                        .foregroundColor(.orange)
                }
            }
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.orange.opacity(0.08))
        .cornerRadius(6)
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Image(systemName: "waveform.path.ecg")
                .font(.system(size: 24))
                .foregroundColor(.secondary)
            Text("No monitor data")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
            Text("Host should call DevToolsBridge.updateEngineMonitor(...)")
                .font(.system(size: 10))
                .foregroundColor(Color(nsColor: .tertiaryLabelColor))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var noDataState: some View {
        VStack(spacing: 8) {
            Image(systemName: "waveform.path.ecg")
                .font(.system(size: 24))
                .foregroundColor(.secondary)
            Text("No data for current tab")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Metric Card Component

private enum MonitorMetricStatus {
    case good, warning, error, neutral

    var color: Color {
        switch self {
        case .good: return .green
        case .warning: return .orange
        case .error: return .red
        case .neutral: return .secondary
        }
    }

    var icon: String {
        switch self {
        case .good: return "checkmark.circle.fill"
        case .warning: return "exclamationmark.triangle.fill"
        case .error: return "xmark.circle.fill"
        case .neutral: return "minus.circle"
        }
    }
}

private struct MonitorMetricCard: View {
    let icon: String
    let title: String
    let value: String
    let subtitle: String
    let status: MonitorMetricStatus
    var compact: Bool = false
    var history: [Double]? = nil
    var historyColor: Color = .blue

    var body: some View {
        VStack(spacing: compact ? 2 : 4) {
            HStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.system(size: compact ? 10 : 12))
                    .foregroundColor(.secondary)
                Text(title)
                    .font(.system(size: compact ? 9 : 10, weight: .medium))
                    .foregroundColor(.secondary)
                Spacer()
                Circle()
                    .fill(status.color)
                    .frame(width: 6, height: 6)
            }

            HStack(alignment: .lastTextBaseline, spacing: 2) {
                Text(value)
                    .font(.system(size: compact ? 16 : 20, weight: .semibold, design: .rounded))
                    .foregroundColor(.primary)
                if !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.system(size: compact ? 9 : 10))
                        .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                }
                Spacer()
            }

            // Sparkline chart
            if let history, history.count > 1 {
                MonitorSparkline(data: history, color: historyColor)
                    .frame(height: compact ? 20 : 28)
            }
        }
        .padding(compact ? 8 : 10)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.6))
        .cornerRadius(8)
    }
}

// MARK: - Sparkline Chart

private struct MonitorSparkline: View {
    let data: [Double]
    var color: Color = .blue
    var showThreshold: Double? = nil

    var body: some View {
        GeometryReader { geo in
            if data.count > 1 {
                let size = geo.size
                let minVal = data.min() ?? 0
                let maxVal = max(data.max() ?? 1, minVal + 1)
                let range = maxVal - minVal
                let stepX = size.width / CGFloat(data.count - 1)
                let barWidth = max(2, stepX - 1)

                ZStack(alignment: .topLeading) {
                    if let threshold = showThreshold {
                        let y = size.height - CGFloat((threshold - minVal) / range) * size.height
                        Path { p in
                            p.move(to: CGPoint(x: 0, y: y))
                            p.addLine(to: CGPoint(x: size.width, y: y))
                        }
                        .stroke(.red.opacity(0.5), style: StrokeStyle(lineWidth: 1, dash: [4, 2]))
                    }

                    ForEach(Array(data.enumerated()), id: \.offset) { index, value in
                        let height = CGFloat((value - minVal) / range) * size.height
                        RoundedRectangle(cornerRadius: 1)
                            .fill(color.opacity(0.7))
                            .frame(width: barWidth, height: max(1, height))
                            .position(
                                x: CGFloat(index) * stepX + barWidth / 2,
                                y: size.height - max(1, height) / 2
                            )
                    }
                }
            }
        }
    }
}

// MARK: - Flow Layout

private struct MonitorFlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let result = arrangeSubviews(proposal: proposal, subviews: subviews)
        return result.size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let result = arrangeSubviews(proposal: proposal, subviews: subviews)
        for (index, position) in result.positions.enumerated() {
            subviews[index].place(at: CGPoint(x: bounds.minX + position.x, y: bounds.minY + position.y), proposal: .unspecified)
        }
    }

    private func arrangeSubviews(proposal: ProposedViewSize, subviews: Subviews) -> (size: CGSize, positions: [CGPoint]) {
        let maxWidth = proposal.width ?? .infinity
        var positions: [CGPoint] = []
        var currentX: CGFloat = 0
        var currentY: CGFloat = 0
        var lineHeight: CGFloat = 0
        var totalHeight: CGFloat = 0

        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)

            if currentX + size.width > maxWidth && currentX > 0 {
                currentX = 0
                currentY += lineHeight + spacing
                lineHeight = 0
            }

            positions.append(CGPoint(x: currentX, y: currentY))
            currentX += size.width + spacing
            lineHeight = max(lineHeight, size.height)
            totalHeight = currentY + lineHeight
        }

        return (CGSize(width: maxWidth, height: totalHeight), positions)
    }
}

// MARK: - Task Manager Row

private struct TaskManagerRow: View {
    @ObservedObject private var diagnosticsStore = RendererDiagnosticsStore.shared
    @ObservedObject var processInfo: TabProcessInfo
    @State private var metrics: (memory: UInt64, cpuTime: UInt64)?

    private var statusText: String {
        switch processInfo.state {
        case .idle: return "Idle"
        case .launching: return "Launching..."
        case .running(let pid): return "Running"
        case .crashed(let reason): return "Crashed"
        case .terminated: return "Terminated"
        }
    }

    private var statusColor: Color {
        switch processInfo.state {
        case .idle: return .secondary
        case .launching: return .orange
        case .running: return .green
        case .crashed: return .red
        case .terminated: return .secondary
        }
    }

    var body: some View {
        HStack(spacing: 0) {
            // Tab ID (shortened)
            Text(processInfo.tabId.uuidString.prefix(8) + "...")
                .font(.system(size: 10, design: .monospaced))
                .frame(width: 120, alignment: .leading)

            // PID
            if let pid = processInfo.pid {
                Text("\(pid)")
                    .font(.system(size: 10, design: .monospaced))
                    .frame(width: 60, alignment: .trailing)
            } else {
                Text("—")
                    .foregroundColor(.secondary)
                    .frame(width: 60, alignment: .trailing)
            }

            // Memory
            if let mem = metrics?.memory {
                let mb = Double(mem) / 1_048_576.0
                Text(String(format: "%.1f MB", mb))
                    .font(.system(size: 10, design: .monospaced))
                    .frame(width: 80, alignment: .trailing)
            } else {
                Text("—")
                    .foregroundColor(.secondary)
                    .frame(width: 80, alignment: .trailing)
            }

            // CPU Time
            if let cpu = metrics?.cpuTime {
                let seconds = Double(cpu) / 1_000_000_000.0
                Text(String(format: "%.1fs", seconds))
                    .font(.system(size: 10, design: .monospaced))
                    .frame(width: 60, alignment: .trailing)
            } else {
                Text("—")
                    .foregroundColor(.secondary)
                    .frame(width: 60, alignment: .trailing)
            }

            // Status
            HStack(spacing: 4) {
                Circle()
                    .fill(statusColor)
                    .frame(width: 6, height: 6)
                Text(statusText)
                    .font(.system(size: 10))
            }
            .frame(width: 80, alignment: .center)

            Spacer()

            // Kill button
            if processInfo.isRunning {
                Button(action: {
                    diagnosticsStore.terminateLegacyRenderer(for: processInfo.tabId)
                }) {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundColor(.red)
                }
                .buttonStyle(.plain)
                .frame(width: 60, alignment: .center)
            } else {
                Color.clear
                    .frame(width: 60)
            }
        }
        .font(.system(size: 10))
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .onAppear {
            updateMetrics()
        }
        .onReceive(Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()) { _ in
            updateMetrics()
        }
    }

    private func updateMetrics() {
        guard let pid = processInfo.pid else {
            metrics = nil
            return
        }
        metrics = diagnosticsStore.getProcessMetrics(pid: pid)
    }
}

// MARK: - Row Extensions

private extension DevToolsEngineMonitorRow {
    var monitorOpsPerSecond: Double {
        diagnostics?.activity?.opsPerSecond ?? 0
    }

    var monitorOpsPerSecondString: String {
        String(format: "%.1f", monitorOpsPerSecond)
    }

    var monitorTimers: Int { diagnostics?.resources?.timers ?? 0 }
    var monitorNodes: Int { diagnostics?.resources?.nodes ?? (diagnostics?.receiver?.nodeCount ?? 0) }
    var monitorCallbacks: Int { diagnostics?.resources?.callbacks ?? 0 }

    var monitorApplyMs: Double? {
        diagnostics?.activity?.lastBatch?.applyDurationMs ?? diagnostics?.receiver?.lastApply?.durationMs
    }

    var monitorApplyMsString: String {
        guard let ms = monitorApplyMs else { return "—" }
        return String(format: "%.0f", ms) + "ms"
    }

    var monitorSkipped: Int {
        diagnostics?.receiver?.lastApply?.skipped ?? 0
    }

    var monitorWarnings: [String] {
        var out: [String] = []

        if status == .blocked {
            out.append("Blocked: \(blockedReason ?? "Unknown")")
        }
        if monitorTimers > 50 {
            out.append("Timers over budget: \(monitorTimers) > 50")
        }
        if monitorNodes > 5000 {
            out.append("Nodes over budget: \(monitorNodes) > 5000")
        }
        if monitorCallbacks > 2000 {
            out.append("Callbacks over budget: \(monitorCallbacks) > 2000")
        }
        if let ms = monitorApplyMs, ms > 50 {
            out.append(String(format: "Apply too slow: %.0fms > 50ms", ms))
        }
        if monitorSkipped > 0 {
            out.append("Backpressure: skipped \(monitorSkipped) ops")
        }

        let violations = diagnostics?.contracts?.violations ?? 0
        if violations > 0 {
            out.append("Contract violations: \(violations)")
        }

        return out
    }

    var monitorHealthItems: [(String, String)] {
        let loaded = diagnostics?.health?.loaded.map { $0 ? "true" : "false" } ?? "—"
        let destroyed = diagnostics?.health?.destroyed.map { $0 ? "true" : "false" } ?? "—"
        let batching = diagnostics?.health?.batching.map { $0 ? "true" : "false" } ?? "—"
        let errorCount = diagnostics?.health?.errorCount.map(String.init) ?? "—"
        return [
            ("status", status?.rawValue ?? "—"),
            ("loaded", loaded),
            ("destroyed", destroyed),
            ("batching", batching),
            ("errors", errorCount),
        ]
    }

    var monitorReceiverItems: [(String, String)] {
        let nodeCount = diagnostics?.receiver?.nodeCount
        let lastRenderMs = diagnostics?.receiver?.lastRenderDurationMs.map { String(format: "%.0fms", $0) } ?? "—"
        let lastApplyMs = diagnostics?.receiver?.lastApply?.durationMs.map { String(format: "%.0fms", $0) } ?? "—"
        let nodeDelta = diagnostics?.receiver?.lastApply?.nodeDelta.map(String.init) ?? "—"

        return [
            ("nodeCount", nodeCount.map(String.init) ?? "—"),
            ("renderMs", lastRenderMs),
            ("applyMs", lastApplyMs),
            ("nodeDelta", nodeDelta),
        ]
    }

    var monitorHostGuestItems: [(String, String)] {
        [
            ("hostEvent", diagnostics?.host?.lastEventName ?? "—"),
            ("hostBytes", diagnostics?.host?.lastPayloadBytes.map(String.init) ?? "—"),
            ("guestEvent", diagnostics?.guest?.lastEventName ?? "—"),
            ("guestBytes", diagnostics?.guest?.lastPayloadBytes.map(String.init) ?? "—"),
            ("sleeping", diagnostics?.guest?.sleeping.map { $0 ? "true" : "false" } ?? "—"),
        ]
    }

    var monitorComplianceItems: [(String, String)] {
        let contractText: String
        if let c = contract {
            contractText = "\(c.name)@v\(c.version)"
        } else {
            contractText = "(none)"
        }
        let violations = diagnostics?.contracts?.violations ?? 0
        return [
            ("contract", contractText),
            ("violations", "\(violations)"),
        ]
    }
}
