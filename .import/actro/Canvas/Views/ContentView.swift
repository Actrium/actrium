import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var windowState: WindowStateManager = .shared
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    @State private var isACTRDropTarget: Bool = false
    
    @StateObject private var loginManager = LoginManager.shared

    var body: some View {
        Group {
            if UIDevice.current.userInterfaceIdiom == .phone {
                PhoneMainView(viewModel: viewModel, historyVM: historyVM)
            } else {
                // 内容区域
                ZStack {
                    ChromeStyleTabView(viewModel: viewModel, historyVM: historyVM)
                    if let systemType = viewModel.currentSystemTabType {
                        SystemAppView(systemType: systemType, viewModel: viewModel, historyVM: historyVM)
                            .padding(.top, 44)
                    }
                    // Search Tabs 搜索框 - 仅在 Overview 模式显示，固定在底部居中
                    if case .overview = viewModel.canvasMode, viewModel.currentSystemTabType != .globalSearch {
                        SearchBar(viewModel: viewModel)
                    }
                }
                .background(Color.black)
                .onReceive(NotificationCenter.default.publisher(for: .exitSearchFromRN)) { _ in
                    withAnimation {
                        if let previous = viewModel.previousSystemTabType {
                            viewModel.currentSystemTabType = previous
                            viewModel.previousSystemTabType = nil
                        } else {
                            viewModel.currentSystemTabType = nil
                        }
                    }
                }
                .onOpenURL { url in
                    handleUniversalLink(url)
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

    // MARK: - Universal Link 处理

    /// AskAway 通用链接的 host 列表
    private static let askawayHosts: Set<String> = [
        "go.askaway.chat",
        "go-askaway.develenv.com"
    ]

    /// 处理通用链接：匹配 AskAway 域名时创建新页签并进入 Focus 模式
    private func handleUniversalLink(_ url: URL) {
        guard let host = url.host, Self.askawayHosts.contains(host) else {
            return
        }

        // 从 URL query 参数中解析 appId
        // 格式：https://go.askaway.chat/appstore/index.html?appid=xxx&env=production
        let appId = URLComponents(url: url, resolvingAgainstBaseURL: true)?.queryItems?
            .first(where: { $0.name == "appid" })?.value

        // appId 为空时不打开新页签
        guard let appId = appId, !appId.isEmpty else {
            return
        }

        // 创建新页签，将 appId 传入 RN 侧
        let newTab = Tab(
            title: TM.active.blackTab(),
            appId: appId,
            unifiedScript: "/* welcome page shell */"
        )
        viewModel.tabs.append(newTab)
        viewModel.tabSlotPositions[newTab.id] = viewModel.tabs.count - 1
        viewModel.currentSystemTabType = nil
        viewModel.switchToTab(id: newTab.id)
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
                    viewModel.previousSystemTabType = viewModel.currentSystemTabType
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
