import SwiftUI

struct PhoneMainView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel

    @State private var selectedTab: PhoneTab = .tabs
    @State private var fullscreenTab: Tab?
    @State private var skipManualSettingsResetOnce = false
    @State private var searchText: String = ""
    @FocusState private var isSearchFocused: Bool
    
    @Namespace private var animation
    
    enum PhoneTab {
        case bookmarks
        case tabs
        case settings
        case search
    }
    
    var body: some View {
        TabView(selection: $selectedTab) {
            // 书签页：使用 RN 渲染
            SwiftUI.Tab(TM.active.bookmarks(), systemImage: "book.fill", value: PhoneTab.bookmarks) {
                PhoneSystemAppView(
                    systemType: .bookmarks,
                    viewModel: viewModel,
                    historyVM: historyVM
                )
            }
            
            SwiftUI.Tab(TM.active.activity(), systemImage: "rectangle.grid.1x2.fill", value: PhoneTab.tabs) {
                PhoneTabListView(viewModel: viewModel, historyVM: historyVM, animation: animation, onSelect: { tab in
                    fullscreenTab = tab
                })
            }
            
            // 设置页：使用 RN 渲染
            SwiftUI.Tab(TM.active.preferences(), systemImage: "gearshape.fill", value: PhoneTab.settings) {
                PhoneSystemAppView(
                    systemType: .settings,
                    viewModel: viewModel,
                    historyVM: historyVM
                )
            }
            
            // 搜索页：独立显示（不占用固定 tab 位置）
            SwiftUI.Tab(TM.active.search(), systemImage: "magnifyingglass", value: PhoneTab.search, role: .search) {
                NavigationStack {
                    PhoneSystemAppView(
                        systemType: .globalSearch,
                        viewModel: viewModel,
                        historyVM: historyVM
                    )
                }
                .searchable(text: $searchText)
                .searchFocused($isSearchFocused)
                .task {
                    isSearchFocused = true
                }
            }
        }
        .accentColor(.blue)
        .background(Color.black)
        .onChange(of: selectedTab) { oldValue, newValue in
            // 离开搜索页时，清空搜索框文本
            if oldValue == .search {
                searchText = ""
                isSearchFocused = false
            }
            // 切换到系统页签时，刷新系统页签
            switch newValue {
            case .bookmarks:
                viewModel.reloadSystemTab(.bookmarks)
            case .settings:
                if skipManualSettingsResetOnce {
                    skipManualSettingsResetOnce = false
                } else {
                    let _ = viewModel.openSystemTab(
                        .settings,
                        additionalProps: nil,
                        replaceAdditionalProps: true
                    )
                }
            case .search:
                viewModel.reloadSystemTab(.globalSearch)
            case .tabs:
                break // 页签列表不需要刷新
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .exitFullscreenFromRN)) { _ in
            // RN 请求退出全屏。存在标题时先完成截图，再执行退场，避免快照拿到半退场画面。
            if let tab = viewModel.tabs.first(where: { $0.id == fullscreenTab?.id}), !tab.title.isEmpty {
                viewModel.skipNextDisappearSnapshot = true
                takeSnapshot(for: tab) {
                    fullscreenTab = nil
                }
            } else {
                fullscreenTab = nil
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .exitSearchFromRN)) { _ in
            // RN 请求退出搜索页 - 切换回页签列表
            withAnimation {
                selectedTab = .tabs
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .switchPhoneTabFromRN)) { notification in
            guard let systemAppType = notification.userInfo?["systemAppType"] as? String else {
                return
            }
            withAnimation {
                switch systemAppType {
                case SystemTabType.bookmarks.rawValue:
                    selectedTab = .bookmarks
                case SystemTabType.settings.rawValue:
                    skipManualSettingsResetOnce = true
                    selectedTab = .settings
                case SystemTabType.globalSearch.rawValue:
                    selectedTab = .search
                default:
                    selectedTab = .tabs
                }
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .openTabFullscreenFromRN)) { notification in
            // RN 请求打开指定 tab
            if let tabIdString = notification.userInfo?["tabId"] as? String,
               let tabId = UUID(uuidString: tabIdString),
               let tab = viewModel.tabs.first(where: { $0.id == tabId }) {
                fullscreenTab = tab
                selectedTab = .tabs
            }
        }
        .onAppear {
            // 初始进入时，如果有选中的 tab，延迟进入全屏模式，确保应用完全启动及布局完成
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                if let currentId = viewModel.currentTabId,
                   let tab = viewModel.tabs.first(where: { $0.id == currentId }) {
                    fullscreenTab = tab
                }
            }
        }
        .sheet(item: $fullscreenTab) { tab in
            PhoneBrowserView(tab: tab, viewModel: viewModel, historyVM: historyVM)
                .ignoresSafeArea()
                .asNavigationTransition(id: tab.id, in: animation)
                .onAppear {
                    // sheet 弹出动画完成后，将 pending 的新标签页正式加入列表显示
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                        viewModel.commitPendingNewTab()
                    }
                }
        }
        .onChange(of: viewModel.currentTabId) { oldValue, newValue in
            guard let newValue = newValue else {
                fullscreenTab = nil
                return
            }
            
            // 确保全屏视图显示的标签页与 currentTabId 保持同步
            if fullscreenTab?.id != newValue {
                if let tab = viewModel.tabs.first(where: { $0.id == newValue }) {
                    fullscreenTab = tab
                }
            }
        }
        .onChange(of: fullscreenTab?.id) { oldValue, newValue in
            viewModel.fullscreenTabId = newValue
        }
        .onChange(of: searchText) { _, newText in
            // 搜索框内容变化时，实时发送通知给 RN 侧
            NotificationCenter.default.post(
                name: .searchInputChangedFromNative,
                object: nil,
                userInfo: [
                    "inputText": newText
                ]
            )
        }
        .onOpenURL { url in
            handleUniversalLink(url)
        }
    }

    // MARK: - Universal Link 处理

    /// AskAway 通用链接的 host 列表
    private static let askawayHosts: Set<String> = [
        "go.askaway.chat",
        "go-askaway.develenv.com"
    ]

    /// 处理通用链接：匹配 AskAway 域名时创建新页签并全屏打开
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
        )
        viewModel.tabs.append(newTab)
        viewModel.tabSlotPositions[newTab.id] = viewModel.tabs.count - 1
        viewModel.currentTabId = newTab.id
        selectedTab = .tabs
        fullscreenTab = newTab
    }

    /// 截取当前全屏 tab 的快照
    private func takeSnapshot(for tab: Tab, completion: @escaping () -> Void) {
        // 延迟一帧，确保捕获正确的内容
        DispatchQueue.main.async {
            defer { completion() }

            let window = UIApplication.shared.connectedScenes
                .filter { $0.activationState == .foregroundActive }
                .compactMap { $0 as? UIWindowScene }
                .first?.windows
                .filter { $0.isKeyWindow }.first
            
            if let window = window {
                let renderer = UIGraphicsImageRenderer(bounds: window.bounds)
                let fullImage = renderer.image { context in
                    window.drawHierarchy(in: window.bounds, afterScreenUpdates: false)
                }
                
                // 截取页面中部区域：宽度 = 页面宽度，高度 = 宽度 * 0.54
                let screenWidth = window.bounds.width
                let screenHeight = window.bounds.height
                let cropHeight = screenWidth * 0.54 - 56
                
                // 垂直居中
                let cropY = (screenHeight - cropHeight) / 2
                
                let cropRect = CGRect(
                    x: 0,
                    y: cropY,
                    width: screenWidth,
                    height: cropHeight
                )
                
                if let croppedImage = fullImage.cropped(to: cropRect) {
                    viewModel.setTabSnapshot(croppedImage, for: tab.id)
                } else {
                    // 如果裁剪失败，保留全屏图作为 fallback
                    viewModel.setTabSnapshot(fullImage, for: tab.id)
                }
                
            }
        }
    }
}
