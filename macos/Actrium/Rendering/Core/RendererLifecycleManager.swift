import Foundation

/// Unified entry point for renderer lifecycle operations.
///
/// Current state:
/// - RN tabs are managed by the ViewBridge/XPC pipeline.
/// - Legacy helper renderers are managed by `LegacyRendererProcessManager`.
final class RendererLifecycleManager {
    static let shared = RendererLifecycleManager()

    private let legacyManager = LegacyRendererProcessManager.shared

    private init() {}

    /// Release the tab's renderer and any legacy helper, including when the
    /// tab will stay open to show a notice or load replacement content.
    @MainActor
    func tabDidClose(_ tabId: UUID) {
        ViewBridgeCoordinatorManager.shared.removeCoordinator(for: tabId)
        terminateRenderer(for: tabId)
    }

    /// Unified terminate API.
    ///
    /// For now it maps to legacy helper teardown.
    func terminateRenderer(for tabId: UUID) {
        legacyManager.terminateProcess(for: tabId)
    }

    func terminateAllLegacyRenderers() {
        legacyManager.terminateAllProcesses()
    }
}
