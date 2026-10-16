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

    /// Called when a tab is closed.
    ///
    /// This currently performs legacy helper cleanup as a compatibility fallback.
    func tabDidClose(_ tabId: UUID) {
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
