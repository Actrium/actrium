import SwiftUI
import UniformTypeIdentifiers
import AppKit

// MARK: - Sidebar Width Preference Key

private struct SidebarWidthKey: PreferenceKey {
    static var defaultValue: CGFloat = 250
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

struct ContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var devTools: DevToolsManager = .shared
    @ObservedObject var windowState: WindowStateManager = .shared
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    @State private var isACTRDropTarget: Bool = false
    @State private var useNativeToolbar: Bool = true  // 使用原生 NSToolbar

    var body: some View {
        ZStack {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                SidebarView(viewModel: viewModel, bookmarkVM: bookmarkVM, historyVM: historyVM)
                    .toolbar(removing: .sidebarToggle)
                    .background(
                        GeometryReader { geo in
                            Color.clear.preference(key: SidebarWidthKey.self, value: geo.size.width)
                        }
                    )
            } detail: {
                // Workspace: main canvas + optional docked DevTools panel
                WorkspaceView(devTools: devTools, viewModel: viewModel) {
                    WorkspaceMainView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM, useNativeToolbar: useNativeToolbar)
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
            .background(WindowAccessor(viewModel: viewModel, bookmarkVM: bookmarkVM, useNativeToolbar: useNativeToolbar))
            // 强制清空窗口标题
            .navigationTitle("")
            // 监听 sidebar 宽度变化
            .onPreferenceChange(SidebarWidthKey.self) { width in
                TitlebarManager.shared.updateSidebarWidth(width)
            }
            // 监听 sidebar 显示/隐藏
            .onChange(of: columnVisibility) { _, newValue in
                let isVisible = newValue != .detailOnly
                TitlebarManager.shared.updateSidebarVisibility(isVisible)
            }
        }
    }
}

// MARK: - Window Accessor (配置 NSWindow 透明标题栏)

private struct WindowAccessor: NSViewRepresentable {
    let viewModel: BrowserViewModel
    let bookmarkVM: BookmarkViewModel
    let useNativeToolbar: Bool

    func makeNSView(context: Context) -> NSView {
        let view = WindowObserverView()
        view.viewModel = viewModel
        view.bookmarkVM = bookmarkVM
        view.useNativeToolbar = useNativeToolbar
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
    weak var viewModel: BrowserViewModel?
    weak var bookmarkVM: BookmarkViewModel?
    var useNativeToolbar: Bool = false
    private var toolbarConfigured: Bool = false

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
        // 设置窗口最小尺寸
        window.minSize = NSSize(width: 500, height: 400)

        // 配置自定义 AppKit 标题栏（只执行一次）
        if useNativeToolbar && !toolbarConfigured,
           let viewModel = viewModel,
           let bookmarkVM = bookmarkVM {
            toolbarConfigured = true
            TitlebarManager.shared.setupTitlebar(for: window, viewModel: viewModel, bookmarkVM: bookmarkVM)
        }
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
