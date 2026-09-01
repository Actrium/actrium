import Foundation
import Combine

/// Centralized diagnostics state for renderer processes used by DevTools/App Manager.
final class RendererDiagnosticsStore: ObservableObject {
    static let shared = RendererDiagnosticsStore()

    @Published private(set) var legacyProcesses: [UUID: TabProcessInfo] = [:]

    private let legacyManager = LegacyRendererProcessManager.shared
    private var cancellables = Set<AnyCancellable>()

    private init() {
        legacyProcesses = legacyManager.processes

        legacyManager.$processes
            .receive(on: DispatchQueue.main)
            .sink { [weak self] processes in
                self?.legacyProcesses = processes
            }
            .store(in: &cancellables)
    }

    func allLegacyProcesses() -> [TabProcessInfo] {
        Array(legacyProcesses.values)
    }

    func processInfo(for tabId: UUID) -> TabProcessInfo? {
        legacyProcesses[tabId]
    }

    func terminateLegacyRenderer(for tabId: UUID) {
        RendererLifecycleManager.shared.terminateRenderer(for: tabId)
    }

    func terminateAllLegacyRenderers() {
        RendererLifecycleManager.shared.terminateAllLegacyRenderers()
    }

    func getProcessMetrics(pid: pid_t) -> (memory: UInt64, cpuTime: UInt64)? {
        legacyManager.getProcessMetrics(pid: pid)
    }
}
