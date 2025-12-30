import SwiftUI
import AppKit

private struct DevToolsMonitorBudgets: Equatable {
    var timers: Int
    var nodes: Int
    var callbacks: Int
    var applyDurationMsMax: Double
    var skippedOps: Int

    static let `default` = DevToolsMonitorBudgets(
        timers: 50,
        nodes: 5000,
        callbacks: 2000,
        applyDurationMsMax: 50,
        skippedOps: 0
    )
}

struct DevToolsMonitorView: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel

    @AppStorage("devtools.monitor.budget.timers") private var budgetTimers: Int = DevToolsMonitorBudgets.default.timers
    @AppStorage("devtools.monitor.budget.nodes") private var budgetNodes: Int = DevToolsMonitorBudgets.default.nodes
    @AppStorage("devtools.monitor.budget.callbacks") private var budgetCallbacks: Int = DevToolsMonitorBudgets.default.callbacks
    @AppStorage("devtools.monitor.budget.applyDurationMsMax") private var budgetApplyDurationMsMax: Double = DevToolsMonitorBudgets.default.applyDurationMsMax
    @AppStorage("devtools.monitor.budget.skippedOps") private var budgetSkippedOps: Int = DevToolsMonitorBudgets.default.skippedOps

    @State private var isFrozen: Bool = false
    @State private var frozenSnapshot: DevToolsEngineMonitorSnapshot?

    @State private var showBudgets: Bool = false

    private var budgets: DevToolsMonitorBudgets {
        DevToolsMonitorBudgets(
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

    private var currentTabIdString: String? {
        viewModel.currentTabId?.uuidString
    }

    private var currentRow: DevToolsEngineMonitorRow? {
        if let currentTabIdString, let row = rows.first(where: { $0.tabId == currentTabIdString }) {
            return row
        }
        // Host 侧默认只推送当前 Tab 的 row，兜底取第一个
        return rows.first
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
                if let row = currentRow {
                    DevToolsMonitorDetailView(row: row, budgets: budgets)
                } else {
                    VStack(spacing: 8) {
                        Image(systemName: "waveform.path.ecg")
                            .font(.system(size: 24))
                            .foregroundColor(.secondary)
                        Text("No monitor data for current tab")
                            .font(.system(size: 12))
                            .foregroundColor(.secondary)
                        if let currentTabIdString {
                            Text("currentTabId: \(currentTabIdString)")
                                .font(.system(size: 10, design: .monospaced))
                                .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
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
        .sheet(isPresented: $showBudgets) {
            DevToolsMonitorBudgetsSheet(
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
            Text(viewModel.currentTab?.title ?? "Current Tab")
                .font(.system(size: 12, weight: .semibold))
                .lineLimit(1)

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
                            DevToolsDisclosure(title: "Last Apply (top types)") {
                                DevToolsTopTypesView(types: lastApply.topNodeTypes)
                            }
                            DevToolsDisclosure(title: "Last Apply (top types skipped)") {
                                DevToolsTopTypesView(types: lastApply.topNodeTypesSkipped)
                            }
                        }
                        if let attribution = row.diagnostics?.receiver?.attribution {
                            DevToolsDisclosure(title: "Attribution (worst batches)") {
                                DevToolsWorstBatchesView(batches: attribution.worstBatches)
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

private struct DevToolsTopTypesView: View {
    let types: [DevToolsEngineMonitorDiagnostics.Receiver.TopNodeType]?

    var body: some View {
        if let types, !types.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(types.prefix(12)) { t in
                    Text("\(t.type): \(t.ops)")
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

private struct DevToolsWorstBatchesView: View {
    let batches: [DevToolsEngineMonitorDiagnostics.Receiver.Attribution.WorstBatch]?

    var body: some View {
        if let batches, !batches.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(batches.prefix(8)) { b in
                    Text(format(b))
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

    private func format(_ b: DevToolsEngineMonitorDiagnostics.Receiver.Attribution.WorstBatch) -> String {
        let kind = b.kind ?? "unknown"
        let total = b.total.map(String.init) ?? "—"
        let skipped = b.skipped.map(String.init) ?? "—"
        let ms = b.durationMs.map { String(format: "%.0f", $0) } ?? "—"
        let delta = b.nodeDelta.map(String.init) ?? "—"
        return "\(kind) total=\(total) skipped=\(skipped) ms=\(ms) Δ=\(delta)"
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
        let loaded = diagnostics?.health?.loaded.map { $0 ? "true" : "false" } ?? "—"
        let destroyed = diagnostics?.health?.destroyed.map { $0 ? "true" : "false" } ?? "—"
        let batching = diagnostics?.health?.batching.map { $0 ? "true" : "false" } ?? "—"
        let errorCount = diagnostics?.health?.errorCount.map(String.init) ?? "—"
        return [
            ("Status", status?.rawValue ?? "—"),
            ("loaded", loaded),
            ("destroyed", destroyed),
            ("batching", batching),
            ("errors", errorCount),
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
