import Foundation
import Combine

/// Centralized diagnostics state for renderer processes used by DevTools/App Manager.
@MainActor
final class RendererDiagnosticsStore: ObservableObject {
    static let shared = RendererDiagnosticsStore()

    @Published private(set) var legacyProcesses: [UUID: TabProcessInfo] = [:]
    @Published private(set) var poolSnapshot = RendererProcessPoolSnapshot()
    @Published private(set) var metricsByPID: [pid_t: PIDMetrics] = [:]

    private let legacyManager = LegacyRendererProcessManager.shared
    private let processPool = RendererProcessPool.shared
    private let sampler = PIDMetricsSampler()
    private var metricsTimer: Timer?
    private var cancellables = Set<AnyCancellable>()

    private init() {
        legacyProcesses = legacyManager.processes
        poolSnapshot = processPool.snapshot

        legacyManager.$processes
            .receive(on: DispatchQueue.main)
            .sink { [weak self] processes in
                Task { @MainActor in
                    guard let self else { return }
                    self.legacyProcesses = processes
                    self.sampleMetrics()
                }
            }
            .store(in: &cancellables)

        processPool.$snapshot
            .receive(on: DispatchQueue.main)
            .sink { [weak self] snapshot in
                Task { @MainActor in
                    guard let self else { return }
                    self.poolSnapshot = snapshot
                    self.sampleMetrics()
                }
            }
            .store(in: &cancellables)

        metricsTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.sampleMetrics()
            }
        }
        sampleMetrics()
    }

    func allLegacyProcesses() -> [TabProcessInfo] {
        legacyProcesses.values.sorted { $0.tabId.uuidString < $1.tabId.uuidString }
    }

    func processInfo(for tabId: UUID) -> TabProcessInfo? {
        legacyProcesses[tabId]
    }

    func poolProcess(for tabId: UUID) -> RendererProcessSnapshot? {
        poolSnapshot.process(for: tabId)
    }

    func metrics(for pid: pid_t) -> PIDMetrics? {
        metricsByPID[pid]
    }

    func terminateLegacyRenderer(for tabId: UUID) {
        RendererLifecycleManager.shared.terminateRenderer(for: tabId)
    }

    func terminateAllLegacyRenderers() {
        RendererLifecycleManager.shared.terminateAllLegacyRenderers()
    }

    private func sampleMetrics() {
        let legacyPIDs = legacyProcesses.values.compactMap(\.pid)
        let poolPIDs = poolSnapshot.processes.map(\.pid)
        metricsByPID = sampler.sample(pids: legacyPIDs + poolPIDs)
    }
}
