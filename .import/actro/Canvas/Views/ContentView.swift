import SwiftUI
import UniformTypeIdentifiers

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
            // 让 toolbar 背景使用深色（与 sidebar 一致）
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
        }
    }
}


#Preview {
    ContentView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel(), bookmarkVM: BookmarkViewModel())
}
