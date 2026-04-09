import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var windowState: WindowStateManager = .shared
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    @State private var isACTRDropTarget: Bool = false
    @State private var showAllTabs: Bool = false
    
    @StateObject private var loginManager = LoginManager.shared

    var body: some View {
        Group {
            if UIDevice.current.userInterfaceIdiom == .phone {
                PhoneMainView(viewModel: viewModel, historyVM: historyVM)
            } else {
                ZStack {
                    // 浏览器风格的布局
                    VStack(spacing: 0) {
                        // 顶部导航栏
                        TabNavigationBar(viewModel: viewModel, showAllTabs: $showAllTabs)
                        
                        // 内容区域：根据 systemTabType 判断是否渲染系统页签
                        ZStack {
                            UnifiedCanvasView(viewModel: viewModel, historyVM: historyVM)
                            if let systemType = viewModel.currentSystemTabType {
                                SystemAppView(systemType: systemType, viewModel: viewModel, historyVM: historyVM)
                            }
                            // Search Tabs 搜索框 - 仅在 Overview 模式显示，固定在底部居中
                            if case .overview = viewModel.canvasMode, viewModel.currentSystemTabType != .globalSearch {
                                SearchBar(viewModel: viewModel)
                            }
                        }
                        .ignoresSafeArea()
                        .background(Color.black)
                        .onReceive(NotificationCenter.default.publisher(for: .exitSearchFromRN)) { _ in
                            withAnimation {
                                viewModel.currentSystemTabType = nil
                            }
                        }
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
    }
}

struct SearchBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    
    var body: some View {
        VStack {
            Spacer()
            
            HStack {
                Spacer()
                
                HStack(spacing: 12) {
                    Image(systemName: "magnifyingglass")
                        .foregroundColor(Color(hex: "#999"))
                        .font(.system(size: 18))
                    
                    Text("\(TM.active.search())...")
                        .foregroundColor(Color(hex: "#999"))
                    
                    Spacer()
                }
                .padding(.horizontal, 11)
                .frame(maxWidth: 500)
                .frame(height: 48)
                .background(Color.white)
                .cornerRadius(24)
                .onTapGesture {
                    viewModel.openSystemTab(.globalSearch, additionalProps: nil, replaceAdditionalProps: true)
                    viewModel.currentSystemTabType = .globalSearch
                }
                
                Spacer()
            }
            .padding(.bottom, 18)
        }
    }
}

#Preview {
    ContentView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel())
}
