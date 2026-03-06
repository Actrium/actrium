import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var windowState: WindowStateManager = .shared
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    @State private var isACTRDropTarget: Bool = false
    @State private var showAllTabs: Bool = false
    
    @StateObject private var loginManager = LoginManager.shared

    var body: some View {
        Group {
            if UIDevice.current.userInterfaceIdiom == .phone {
                PhoneMainView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM)
                    .preferredColorScheme(.dark)
            } else {
                ZStack {
                    // 浏览器风格的布局
                    VStack(spacing: 0) {
                        // 顶部导航栏
                        TabNavigationBar(viewModel: viewModel, showAllTabs: $showAllTabs)
                        
                        // 内容区域
                        UnifiedCanvasView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM)
                    }
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
        .sheet(isPresented: $loginManager.showLoginSheet) {
            HStack {
                switch loginManager.currentLoginSheetType {
                case .login:
                    LoginSheet()
                        .presentationDetents([.height(341)])
                        .interactiveDismissDisabled()
                case .loginAuth:
                    LoginAuthorSheet()
                        .presentationDetents([.height(367)])
                        .interactiveDismissDisabled()
                case .loginView:
                    LoginView(detent: .constant(.large))
                        .presentationDetents([.large])
                        .interactiveDismissDisabled()
                        .ignoresSafeArea()
                case .none:
                    EmptyView()
                }
            }
        }
    }
}


#Preview {
    ContentView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel(), bookmarkVM: BookmarkViewModel())
}
