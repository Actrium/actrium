import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    @State private var isACTRDropTarget: Bool = false

    var body: some View {
        NavigationSplitView(columnVisibility: $columnVisibility) {
            SidebarView(viewModel: viewModel)
        } detail: {
            UnifiedCanvasView(viewModel: viewModel)
        }
        // 顶层兜底：拖入 .actr 时创建新标签（使用 dropDestination API）
        .dropDestination(for: URL.self) { urls, location in
            guard let url = urls.first else {
                return false
            }

            guard url.pathExtension.lowercased() == "actr" else {
                return false
            }

            Task { @MainActor in
                viewModel.importACTRPackage(from: url)
            }

            return true
        } isTargeted: { isTargeted in
            isACTRDropTarget = isTargeted
        }
    }
}

#Preview {
    ContentView(viewModel: BrowserViewModel())
}
