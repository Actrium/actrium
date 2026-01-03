import SwiftUI
import UniformTypeIdentifiers
import AppKit

struct ContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var devTools: DevToolsManager = .shared
    @ObservedObject var windowState: WindowStateManager = .shared
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    @State private var isACTRDropTarget: Bool = false

    var body: some View {
        ZStack {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                SidebarView(viewModel: viewModel, bookmarkVM: bookmarkVM, historyVM: historyVM)
            } detail: {
                // Workspace: main canvas + optional docked DevTools panel
                WorkspaceView(devTools: devTools, viewModel: viewModel) {
                    WorkspaceMainView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM)
                }
            }
            // 让 toolbar 背景透明，内容区域背景通栏
            .toolbarBackground(.hidden, for: .windowToolbar)
            // 顶层兜底：拖入 .actr 或目录时创建新标签
            .dropDestination(for: URL.self) { urls, location in
                guard let url = urls.first else {
                    return false
                }

                // Check if it's a .actr file or a directory with manifest.json
                var isDirectory: ObjCBool = false
                guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
                    return false
                }

                if isDirectory.boolValue {
                    // Directory - check if it has manifest.json
                    let manifestPath = url.appendingPathComponent("manifest.json").path
                    guard FileManager.default.fileExists(atPath: manifestPath) else {
                        return false
                    }
                    Task { @MainActor in
                        viewModel.loadFromDevDirectory(url)
                    }
                    return true
                } else if url.pathExtension.lowercased() == "actr" {
                    // .actr file
                    Task { @MainActor in
                        viewModel.importACTRPackage(from: url)
                    }
                    return true
                }

                return false
            } isTargeted: { isTargeted in
                isACTRDropTarget = isTargeted
            }
            // 配置窗口：透明标题栏，让背景通栏
            .background(WindowAccessor())
            // 强制清空窗口标题
            .navigationTitle("")
        }
    }
}

// MARK: - Window Accessor (配置 NSWindow 透明标题栏)

private struct WindowAccessor: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = WindowObserverView()
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        if let observer = nsView as? WindowObserverView {
            observer.configureWindow()
        }
    }
}

/// 监听窗口并持续隐藏标题
private class WindowObserverView: NSView {
    private var titleObserver: NSKeyValueObservation?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        configureWindow()
        setupTitleObserver()
    }

    func configureWindow() {
        guard let window = window else { return }
        window.backgroundColor = NSColor(white: 0.12, alpha: 1.0)
        window.titlebarSeparatorStyle = .line
        window.titleVisibility = .hidden
        window.title = ""
    }

    private func setupTitleObserver() {
        guard let window = window else { return }
        // 监听标题变化，立即重置为空
        titleObserver = window.observe(\.title, options: [.new]) { [weak self] window, _ in
            if !window.title.isEmpty {
                DispatchQueue.main.async {
                    window.title = ""
                    window.titleVisibility = .hidden
                }
            }
        }
    }

    deinit {
        titleObserver?.invalidate()
    }
}

#Preview {
    ContentView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel(), bookmarkVM: BookmarkViewModel())
}
