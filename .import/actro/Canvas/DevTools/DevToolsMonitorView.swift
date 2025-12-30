import SwiftUI
import AppKit

private struct DevToolsMonitorBudgets: Equatable {
    var backgroundOpsPerSecond: Double
    var timers: Int
    var nodes: Int
    var callbacks: Int
    var applyDurationMsMax: Double
    var skippedOps: Int

    static let `default` = DevToolsMonitorBudgets(
        backgroundOpsPerSecond: 0.2,
        timers: 50,
        nodes: 5000,
        callbacks: 2000,
        applyDurationMsMax: 50,
        skippedOps: 0
    )
}

private enum DevToolsMonitorFilter: String, CaseIterable, Identifiable {
    case all = "All"
    case warnings = "Warnings"
    case background = "Background"
    case blocked = "Blocked"

    var id: String { rawValue }
}

struct DevToolsMonitorView: View {
    @ObservedObject var devTools: DevToolsManager

    @AppStorage("devtools.monitor.budget.backgroundOpsPerSecond") private var budgetBackgroundOpsPerSecond: Double = DevToolsMonitorBudgets.default.backgroundOpsPerSecond
    @AppStorage("devtools.monitor.budget.timers") private var budgetTimers: Int = DevToolsMonitorBudgets.default.timers
    @AppStorage("devtools.monitor.budget.nodes") private var budgetNodes: Int = DevToolsMonitorBudgets.default.nodes
    @AppStorage("devtools.monitor.budget.callbacks") private var budgetCallbacks: Int = DevToolsMonitorBudgets.default.callbacks
    @AppStorage("devtools.monitor.budget.applyDurationMsMax") private var budgetApplyDurationMsMax: Double = DevToolsMonitorBudgets.default.applyDurationMsMax
    @AppStorage("devtools.monitor.budget.skippedOps") private var budgetSkippedOps: Int = DevToolsMonitorBudgets.default.skippedOps

    @State private var filter: DevToolsMonitorFilter = .all
    @State private var searchText: String = ""
    @State private var selectedTabId: String?

    @State private var isFrozen: Bool = false
    @State private var frozenSnapshot: DevToolsEngineMonitorSnapshot?

    @State private var showBudgets: Bool = false

    private var budgets: DevToolsMonitorBudgets {
        DevToolsMonitorBudgets(
            backgroundOpsPerSecond: budgetBackgroundOpsPerSecond,
            timers: budgetTimers,
            nodes: budgetNodes,
            callbacks: budgetCallbacks,
            applyDurationMsMax: budgetApplyDurationMsMax,
            skippedOps: budgetSkippedOps
        )
    }

    private var snapshot: DevToolsEngineMonitorSnapshot? {
        isFrozen ? frozenSnapshot : devTools.engineMonitorSnapshot
    }

    private var rows: [DevToolsEngineMonitorRow] {
        snapshot?.rows ?? []
    }

    private var filteredRows: [DevToolsEngineMonitorRow] {
        rows
            .filter { row in
                switch filter {
                case .all:
                    break
                case .warnings:
                    if !row.monitorHasWarning(budgets: budgets) { return false }
                case .background:
                    if !(row.isActive == false && row.monitorOpsPerSecond > budgets.backgroundOpsPerSecond) { return false }
                case .blocked:
                    if row.status != .blocked { return false }
                }

                if !searchText.isEmpty {
                    let q = searchText.lowercased()
                    let matchesTitle = row.title.lowercased().contains(q)
                    let matchesTabId = row.tabId.lowercased().contains(q)
                    let matchesEngineId = row.engineId?.lowercased().contains(q) ?? false
                    if !(matchesTitle || matchesTabId || matchesEngineId) { return false }
                }

                return true
            }
            .sorted { lhs, rhs in
                let l = lhs.monitorSeverityRank(budgets: budgets)
                let r = rhs.monitorSeverityRank(budgets: budgets)
                if l != r { return l < r }
                if lhs.isActive != rhs.isActive { return lhs.isActive }
                return lhs.title < rhs.title
            }
    }

    private var selectedRow: DevToolsEngineMonitorRow? {
        guard let selectedTabId else { return nil }
        return rows.first(where: { $0.tabId == selectedTabId })
    }

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            Divider()

            if let error = devTools.engineMonitorDecodeError, !error.isEmpty {
                Text("Monitor decode error: \(error)")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(.red)
                    .padding(8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.red.opacity(0.06))
                Divider()
            }

            if rows.isEmpty {
                emptyState
            } else {
                HSplitView {
                    List(selection: $selectedTabId) {
                        ForEach(filteredRows) { row in
                            DevToolsMonitorRowView(row: row, budgets: budgets)
                                .tag(row.tabId)
                        }
                    }
                    .listStyle(.plain)
                    .frame(minWidth: 460)

                    if let row = selectedRow {
                        DevToolsMonitorDetailView(row: row, budgets: budgets)
                            .frame(minWidth: 320)
                    } else {
                        VStack {
                            Text("Select a tab to inspect")
                                .font(.system(size: 12))
                                .foregroundColor(.secondary)
                        }
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
        .onChange(of: isFrozen) { _, next in
            if next {
                frozenSnapshot = devTools.engineMonitorSnapshot
            } else {
                frozenSnapshot = nil
            }
        }
        .onChange(of: devTools.engineMonitorSnapshot?.timestamp) { _, _ in
            // Auto-select active tab when first data arrives.
            if selectedTabId == nil, let active = devTools.engineMonitorSnapshot?.activeTabId {
                selectedTabId = active
            }
        }
        .sheet(isPresented: $showBudgets) {
            DevToolsMonitorBudgetsSheet(
                backgroundOpsPerSecond: $budgetBackgroundOpsPerSecond,
                timers: $budgetTimers,
                nodes: $budgetNodes,
                callbacks: $budgetCallbacks,
                applyDurationMsMax: $budgetApplyDurationMsMax,
                skippedOps: $budgetSkippedOps
            )
        }
    }

    private var toolbar: some View {
        HStack(spacing: 8) {
            Picker("", selection: $filter) {
                ForEach(DevToolsMonitorFilter.allCases) { option in
                    Text(option.rawValue).tag(option)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 260)

            HStack(spacing: 4) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 10))
                    .foregroundColor(.secondary)
                TextField("Filter", text: $searchText)
                    .textFieldStyle(.plain)
                    .font(.system(size: 11))
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(Color(nsColor: .textBackgroundColor))
            .cornerRadius(4)
            .frame(maxWidth: 220)

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

            Button {
                showBudgets = true
            } label: {
                Image(systemName: "slider.horizontal.3")
                    .font(.system(size: 11))
            }
            .buttonStyle(.borderless)
            .help("Budgets")
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Image(systemName: "waveform.path.ecg")
                .font(.system(size: 24))
                .foregroundColor(.secondary)
            Text("No monitor data")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
            Text("Host should call DevToolsBridge.updateEngineMonitor(...) periodically")
                .font(.system(size: 11))
                .foregroundColor(Color(nsColor: .tertiaryLabelColor))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct DevToolsMonitorRowView: View {
    let row: DevToolsEngineMonitorRow
    let budgets: DevToolsMonitorBudgets

    private var badge: (text: String, color: Color)? {
        if row.status == .blocked { return ("BLOCKED", .red) }
        if row.monitorHasWarning(budgets: budgets) { return ("WARN", .orange) }
        return nil
    }

    var body: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(row.isActive ? "▶" : " ")
                        .font(.system(size: 10, weight: .bold, design: .monospaced))
                        .foregroundColor(row.isActive ? .accentColor : .secondary)
                        .frame(width: 10, alignment: .leading)

                    Text(row.title)
                        .font(.system(size: 12, weight: .medium))
                        .lineLimit(1)

                    if let badge {
                        Text(badge.text)
                            .font(.system(size: 9, weight: .semibold, design: .monospaced))
                            .foregroundColor(.white)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(badge.color)
                            .cornerRadius(3)
                    }
                }

                HStack(spacing: 10) {
                    DevToolsMonitorMetric(label: "ops/s", value: row.monitorOpsPerSecondString)
                    DevToolsMonitorMetric(label: "batch/s", value: row.monitorBatchesPerSecondString)
                    DevToolsMonitorMetric(label: "nodes", value: row.monitorNodeCountString)
                    DevToolsMonitorMetric(label: "timers", value: row.monitorTimersString)
                    DevToolsMonitorMetric(label: "cb", value: row.monitorCallbacksString)
                    DevToolsMonitorMetric(label: "apply", value: row.monitorApplyMsString)
                    DevToolsMonitorMetric(label: "skip", value: row.monitorSkippedString)
                }
            }

            Spacer(minLength: 0)

            DevToolsMonitorSparklineView(points: row.monitorTimelinePoints)
                .frame(width: 120, height: 32)
        }
        .padding(.vertical, 4)
        .contextMenu {
            Button("Copy tabId") { DevToolsClipboard.copy(row.tabId) }
            if let engineId = row.engineId { Button("Copy engineId") { DevToolsClipboard.copy(engineId) } }
        }
    }
}

private struct DevToolsMonitorMetric: View {
    let label: String
    let value: String

    var body: some View {
        HStack(spacing: 2) {
            Text(label + ":")
                .font(.system(size: 9))
                .foregroundColor(Color(nsColor: .tertiaryLabelColor))
            Text(value)
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(.secondary)
        }
    }
}

private struct DevToolsMonitorDetailView: View {
    let row: DevToolsEngineMonitorRow
    let budgets: DevToolsMonitorBudgets

    private var warnings: [String] {
        row.monitorWarnings(budgets: budgets)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if !warnings.isEmpty {
                        DevToolsMonitorSection(title: "Warnings") {
                            VStack(alignment: .leading, spacing: 6) {
                                ForEach(warnings, id: \.self) { w in
                                    Text("• " + w)
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundColor(.orange)
                                }
                            }
                            .padding(8)
                            .background(Color.orange.opacity(0.08))
                            .cornerRadius(6)
                        }
                    }

                    DevToolsMonitorSection(title: "Trend (last buckets)") {
                        DevToolsMonitorSparklineView(points: row.monitorTimelinePoints, style: .large)
                            .frame(maxWidth: .infinity)
                            .frame(height: 90)
                    }

                    DevToolsMonitorSection(title: "Overview") {
                        DevToolsKeyValueGrid(items: row.monitorOverviewItems(budgets: budgets))
                    }

                    DevToolsMonitorSection(title: "Receiver") {
                        DevToolsKeyValueGrid(items: row.monitorReceiverItems)
                        if let lastApply = row.diagnostics?.receiver?.lastApply {
                            DevToolsDisclosure(title: "Last Apply (op counts)") {
                                DevToolsCountsView(counts: lastApply.opCounts)
                            }
                            DevToolsDisclosure(title: "Last Apply (skipped counts)") {
                                DevToolsCountsView(counts: lastApply.skippedOpCounts)
                            }
                        }
                    }

                    DevToolsMonitorSection(title: "Host / Guest") {
                        DevToolsKeyValueGrid(items: row.monitorHostGuestItems)
                    }

                    DevToolsMonitorSection(title: "Compliance") {
                        DevToolsKeyValueGrid(items: row.monitorComplianceItems)
                    }
                }
                .padding(12)
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text(row.title)
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(1)

                Spacer()

                Button {
                    DevToolsClipboard.copy(row.tabId)
                } label: {
                    Image(systemName: "doc.on.doc")
                        .font(.system(size: 11))
                }
                .buttonStyle(.borderless)
                .help("Copy tabId")
            }

            HStack(spacing: 8) {
                Text("tabId: \(row.tabId)")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(.secondary)
                    .textSelection(.enabled)
                if let engineId = row.engineId {
                    Text("engineId: \(engineId)")
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundColor(.secondary)
                        .textSelection(.enabled)
                }
            }
        }
        .padding(12)
        .background(Color(nsColor: .controlBackgroundColor))
    }
}

private struct DevToolsMonitorSection<Content: View>: View {
    let title: String
    let content: Content

    init(title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(size: 11, weight: .bold))
                .foregroundColor(.secondary)
            content
        }
    }
}

private struct DevToolsKeyValueGrid: View {
    let items: [(String, String)]

    private let columns = [
        GridItem(.fixed(120), alignment: .trailing),
        GridItem(.flexible(minimum: 120), alignment: .leading),
    ]

    var body: some View {
        LazyVGrid(columns: columns, alignment: .leading, spacing: 6) {
            ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                Text(item.0 + ":")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, alignment: .trailing)
                Text(item.1)
                    .font(.system(size: 11, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
        .cornerRadius(6)
    }
}

private struct DevToolsDisclosure<Content: View>: View {
    let title: String
    let content: Content

    init(title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        DisclosureGroup(title) {
            content
                .padding(.top, 6)
        }
        .font(.system(size: 11))
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.35))
        .cornerRadius(6)
    }
}

private struct DevToolsCountsView: View {
    let counts: [String: Int]?

    var body: some View {
        if let counts, !counts.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(counts.sorted(by: { $0.value > $1.value }).prefix(12), id: \.key) { key, value in
                    Text("\(key): \(value)")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(.secondary)
                }
            }
        } else {
            Text("—")
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(.secondary)
        }
    }
}

private struct DevToolsMonitorSparklineView: View {
    enum Style {
        case compact
        case large
    }

    let points: [DevToolsEngineMonitorDiagnostics.Activity.Timeline.Point]
    var style: Style = .compact

    private var barGap: CGFloat { style == .compact ? 1 : 2 }
    private var opsColor: Color { .secondary.opacity(0.75) }
    private var applyColor: Color { .blue.opacity(0.75) }
    private var skippedColor: Color { .orange.opacity(0.9) }

    var body: some View {
        GeometryReader { geo in
            Canvas { context, size in
                guard !points.isEmpty else { return }

                let count = points.count
                let totalGap = CGFloat(max(0, count - 1)) * barGap
                let barW = max(1, (size.width - totalGap) / CGFloat(count))
                let maxOps = max(1, points.map { $0.ops ?? 0 }.max() ?? 0)
                let maxApply = max(1, Int(points.map { $0.applyDurationMsMax ?? 0 }.max() ?? 0))

                for (i, p) in points.enumerated() {
                    let ops = CGFloat(p.ops ?? 0)
                    let apply = CGFloat(p.applyDurationMsMax ?? 0)
                    let skipped = (p.skippedOps ?? 0) > 0

                    let opsH = (ops / CGFloat(maxOps)) * size.height
                    let applyH = (apply / CGFloat(maxApply)) * size.height

                    let x = CGFloat(i) * (barW + barGap)
                    let opsRect = CGRect(x: x, y: size.height - opsH, width: barW, height: opsH)
                    let applyRect = CGRect(x: x, y: size.height - applyH, width: barW, height: applyH)

                    context.fill(
                        Path(roundedRect: opsRect, cornerRadius: 1),
                        with: .color(skipped ? skippedColor : opsColor)
                    )

                    // Draw apply on top with narrower width for readability
                    let applyInset = max(0, barW * 0.25)
                    let applyTopRect = applyRect.insetBy(dx: applyInset, dy: 0)
                    context.fill(
                        Path(roundedRect: applyTopRect, cornerRadius: 1),
                        with: .color(applyColor)
                    )
                }
            }
            .frame(width: geo.size.width, height: geo.size.height)
        }
    }
}

private struct DevToolsMonitorBudgetsSheet: View {
    @Binding var backgroundOpsPerSecond: Double
    @Binding var timers: Int
    @Binding var nodes: Int
    @Binding var callbacks: Int
    @Binding var applyDurationMsMax: Double
    @Binding var skippedOps: Int

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Monitor Budgets")
                    .font(.system(size: 14, weight: .semibold))
                Spacer()
                Button("Done") { dismiss() }
            }

            Form {
                LabeledContent("Background ops/s") {
                    TextField("", value: $backgroundOpsPerSecond, format: .number)
                        .frame(width: 120)
                }
                LabeledContent("Timers") {
                    TextField("", value: $timers, format: .number)
                        .frame(width: 120)
                }
                LabeledContent("Nodes") {
                    TextField("", value: $nodes, format: .number)
                        .frame(width: 120)
                }
                LabeledContent("Callbacks") {
                    TextField("", value: $callbacks, format: .number)
                        .frame(width: 120)
                }
                LabeledContent("Apply max (ms)") {
                    TextField("", value: $applyDurationMsMax, format: .number)
                        .frame(width: 120)
                }
                LabeledContent("Skipped ops") {
                    TextField("", value: $skippedOps, format: .number)
                        .frame(width: 120)
                }
            }
        }
        .padding(16)
        .frame(minWidth: 420, minHeight: 320)
    }
}

private enum DevToolsClipboard {
    static func copy(_ string: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(string, forType: .string)
    }
}

private extension DevToolsEngineMonitorRow {
    var monitorOpsPerSecond: Double {
        diagnostics?.activity?.opsPerSecond ?? 0
    }

    var monitorBatchesPerSecond: Double {
        diagnostics?.activity?.batchesPerSecond ?? 0
    }

    var monitorOpsPerSecondString: String { String(format: "%.2f", monitorOpsPerSecond) }
    var monitorBatchesPerSecondString: String { String(format: "%.2f", monitorBatchesPerSecond) }

    var monitorTimers: Int { diagnostics?.resources?.timers ?? 0 }
    var monitorNodes: Int { diagnostics?.resources?.nodes ?? (diagnostics?.receiver?.nodeCount ?? 0) }
    var monitorCallbacks: Int { diagnostics?.resources?.callbacks ?? 0 }

    var monitorTimersString: String { "\(monitorTimers)" }
    var monitorNodeCountString: String { "\(monitorNodes)" }
    var monitorCallbacksString: String { "\(monitorCallbacks)" }

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

    var monitorSkippedString: String { "\(monitorSkipped)" }

    var monitorTimelinePoints: [DevToolsEngineMonitorDiagnostics.Activity.Timeline.Point] {
        diagnostics?.activity?.timeline?.points ?? []
    }

    func monitorHasWarning(budgets: DevToolsMonitorBudgets) -> Bool {
        if status == .blocked { return true }
        if !isActive && monitorOpsPerSecond > budgets.backgroundOpsPerSecond { return true }
        if monitorTimers > budgets.timers { return true }
        if monitorNodes > budgets.nodes { return true }
        if monitorCallbacks > budgets.callbacks { return true }

        let applyMax = monitorApplyMaxMs
        if let applyMax, applyMax > budgets.applyDurationMsMax { return true }

        if monitorSkipped > budgets.skippedOps { return true }
        if monitorTimelineSkippedSum > budgets.skippedOps { return true }

        let violations = diagnostics?.contracts?.violations ?? 0
        if violations > 0 { return true }

        return false
    }

    func monitorSeverityRank(budgets: DevToolsMonitorBudgets) -> Int {
        if status == .blocked { return 0 }
        if monitorHasWarning(budgets: budgets) { return 1 }
        if isActive { return 2 }
        return 3
    }

    func monitorWarnings(budgets: DevToolsMonitorBudgets) -> [String] {
        var out: [String] = []

        if status == .blocked {
            out.append("Blocked: \(blockedReason ?? "Unknown")")
        }

        if !isActive && monitorOpsPerSecond > budgets.backgroundOpsPerSecond {
            out.append(String(format: "Background busy: ops/s %.2f > %.2f", monitorOpsPerSecond, budgets.backgroundOpsPerSecond))
        }

        if monitorTimers > budgets.timers {
            out.append("Timers over budget: \(monitorTimers) > \(budgets.timers)")
        }
        if monitorNodes > budgets.nodes {
            out.append("Nodes over budget: \(monitorNodes) > \(budgets.nodes)")
        }
        if monitorCallbacks > budgets.callbacks {
            out.append("Callbacks over budget: \(monitorCallbacks) > \(budgets.callbacks)")
        }

        if let applyMax = monitorApplyMaxMs, applyMax > budgets.applyDurationMsMax {
            out.append(String(format: "Apply too slow: %.0fms > %.0fms", applyMax, budgets.applyDurationMsMax))
        }

        if monitorSkipped > budgets.skippedOps {
            out.append("Backpressure: skipped \(monitorSkipped) > \(budgets.skippedOps)")
        } else if monitorTimelineSkippedSum > budgets.skippedOps {
            out.append("Backpressure (timeline): skipped \(monitorTimelineSkippedSum) > \(budgets.skippedOps)")
        }

        let violations = diagnostics?.contracts?.violations ?? 0
        if violations > 0 {
            out.append("Contracts: violations=\(violations)")
        }

        let guestSleeping = diagnostics?.guest?.sleeping
        if !isActive, guestSleeping == false {
            out.append("Guest still active while tab is background (check HOST_VISIBILITY / sleep reporting)")
        }

        return out
    }

    private var monitorTimelineSkippedSum: Int {
        monitorTimelinePoints.reduce(0) { $0 + ($1.skippedOps ?? 0) }
    }

    private var monitorApplyMaxMs: Double? {
        if let ms = diagnostics?.activity?.lastBatch?.applyDurationMs { return ms }
        let max = monitorTimelinePoints.map { $0.applyDurationMsMax ?? 0 }.max() ?? 0
        return max > 0 ? max : nil
    }

    func monitorOverviewItems(budgets: DevToolsMonitorBudgets) -> [(String, String)] {
        [
            ("Status", status?.rawValue ?? "—"),
            ("ops/s", monitorOpsPerSecondString),
            ("batch/s", monitorBatchesPerSecondString),
            ("Nodes", "\(monitorNodes) / \(budgets.nodes)"),
            ("Timers", "\(monitorTimers) / \(budgets.timers)"),
            ("Callbacks", "\(monitorCallbacks) / \(budgets.callbacks)"),
            ("Apply", monitorApplyMsString + " / " + String(format: "%.0fms", budgets.applyDurationMsMax)),
            ("Skipped", "\(monitorSkipped) / \(budgets.skippedOps)"),
        ]
    }

    var monitorReceiverItems: [(String, String)] {
        let nodeCount = diagnostics?.receiver?.nodeCount
        let lastRenderAt = formatAgeMs(diagnostics?.receiver?.lastRenderAt)
        let lastRenderMs = diagnostics?.receiver?.lastRenderDurationMs.map { String(format: "%.0fms", $0) } ?? "—"
        let lastApplyMs = diagnostics?.receiver?.lastApply?.durationMs.map { String(format: "%.0fms", $0) } ?? "—"
        let nodeDelta = diagnostics?.receiver?.lastApply?.nodeDelta.map(String.init) ?? "—"

        return [
            ("nodeCount", nodeCount.map(String.init) ?? "—"),
            ("lastRender", lastRenderAt),
            ("renderMs", lastRenderMs),
            ("lastApplyMs", lastApplyMs),
            ("nodeΔ", nodeDelta),
        ]
    }

    var monitorHostGuestItems: [(String, String)] {
        [
            ("Host event", diagnostics?.host?.lastEventName ?? "—"),
            ("Host age", formatAgeMs(diagnostics?.host?.lastEventAt)),
            ("Host bytes", diagnostics?.host?.lastPayloadBytes.map(String.init) ?? "—"),
            ("Guest event", diagnostics?.guest?.lastEventName ?? "—"),
            ("Guest age", formatAgeMs(diagnostics?.guest?.lastEventAt)),
            ("Guest bytes", diagnostics?.guest?.lastPayloadBytes.map(String.init) ?? "—"),
            ("Guest sleeping", diagnostics?.guest?.sleeping.map { $0 ? "true" : "false" } ?? "—"),
        ]
    }

    var monitorComplianceItems: [(String, String)] {
        let contractText: String
        if let c = contract {
            contractText = "\(c.name)@v\(c.version)"
        } else {
            contractText = "(Undeclared/Compatibility Mode)"
        }
        let permText: String
        if let permissions, !permissions.isEmpty {
            permText = permissions.joined(separator: ", ")
        } else {
            permText = "(None)"
        }
        let violations = diagnostics?.contracts?.violations ?? 0
        let lastViolation = diagnostics?.contracts?.lastViolation ?? "—"
        return [
            ("Contract", contractText),
            ("Permissions", permText),
            ("Violations", "\(violations)"),
            ("Last violation", lastViolation),
        ]
    }

    private func formatAgeMs(_ atMs: Double?) -> String {
        guard let atMs else { return "—" }
        let age = Date().timeIntervalSince1970 * 1000.0 - atMs
        if age < 0 { return "0.0s" }
        return String(format: "%.1fs", age / 1000.0)
    }
}

