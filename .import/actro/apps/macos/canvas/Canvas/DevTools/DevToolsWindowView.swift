import SwiftUI

/// View for the detached DevTools window
struct DevToolsWindowView: View {
    /// ViewModel passed from focused window (may become nil when focus changes)
    var viewModel: BrowserViewModel?
    @ObservedObject private var devTools = DevToolsManager.shared
    @Environment(\.dismissWindow) private var dismissWindow

    /// Effective viewModel: prefer passed viewModel, fallback to stored one
    private var effectiveViewModel: BrowserViewModel? {
        viewModel ?? devTools.activeBrowserViewModel
    }

    var body: some View {
        Group {
            if let vm = effectiveViewModel, vm.canvasMode.isInFocusMode {
                // Normal DevTools panel when in Focus mode
                DevToolsPanel(
                    devTools: devTools,
                    viewModel: vm,
                    isDetached: true
                )
            } else {
                // Blank state when in Overview mode or no viewModel
                DevToolsOverviewPlaceholder()
            }
        }
        .onAppear {
            devTools.isDetachedWindowOpen = true
            // Store window reference for programmatic control
            DispatchQueue.main.async {
                if let window = NSApp.windows.first(where: { $0.title == "DevTools" }) {
                    devTools.detachedWindow = window
                }
            }
        }
        .onDisappear {
            devTools.isDetachedWindowOpen = false
            devTools.detachedWindow = nil
            // If window is closed manually, switch back to hidden mode
            if devTools.displayMode == .detached {
                devTools.displayMode = .hidden
            }
        }
        .onChange(of: devTools.displayMode) { oldMode, newMode in
            // If mode changed to docked or hidden, close this window
            if newMode != .detached && devTools.isDetachedWindowOpen {
                dismissWindow(id: "devtools")
            }
        }
    }
}

/// Placeholder view shown in detached DevTools when in Overview mode
private struct DevToolsOverviewPlaceholder: View {
    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "square.grid.2x2")
                .font(.system(size: 48))
                .foregroundColor(.secondary)

            Text("DevTools Unavailable")
                .font(.headline)
                .foregroundColor(.primary)

            Text("Switch to a tab to inspect it")
                .font(.subheadline)
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

// MARK: - Preview

#Preview("DevTools Window - No ViewModel") {
    DevToolsWindowView(viewModel: nil)
        .frame(width: 900, height: 500)
}
