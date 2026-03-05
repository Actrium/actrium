import SwiftUI

struct PhoneMainView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    
    @State private var selectedTab: PhoneTab = .tabs
    @State private var fullscreenTab: Tab?
    @State private var skipManualSettingsResetOnce = false
    
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
            SwiftUI.Tab("书签", systemImage: "book.fill", value: PhoneTab.bookmarks) {
                PhoneSystemAppView(
                    systemType: .bookmarks,
                    viewModel: viewModel,
                    historyVM: historyVM,
                    bookmarkVM: bookmarkVM,
                    onSelect: { tab in
                        fullscreenTab = tab
                    }
                )
            }
            
            SwiftUI.Tab("页签", systemImage: "rectangle.grid.1x2.fill", value: PhoneTab.tabs) {
                PhoneTabListView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM, onSelect: { tab in
                    fullscreenTab = tab
                })
            }
            
            // 设置页：使用 RN 渲染
            SwiftUI.Tab("偏好设置", systemImage: "gearshape.fill", value: PhoneTab.settings) {
                PhoneSystemAppView(
                    systemType: .settings,
                    viewModel: viewModel,
                    historyVM: historyVM,
                    bookmarkVM: bookmarkVM,
                    onSelect: { tab in
                        fullscreenTab = tab
                    }
                )
            }

            // 搜索页：独立显示（不占用固定 tab 位置）
            SwiftUI.Tab("搜索", systemImage: "magnifyingglass", value: PhoneTab.search, role: .search) {
                PhoneSystemAppView(
                    systemType: .globalSearch,
                    viewModel: viewModel,
                    historyVM: historyVM,
                    bookmarkVM: bookmarkVM,
                    onSelect: { tab in
                        fullscreenTab = tab
                    }
                )
                .toolbar(.hidden, for: .tabBar)
            }
        }
        .accentColor(.blue)
        .onChange(of: selectedTab) { oldValue, newValue in
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
        .fullScreenCover(item: $fullscreenTab) { tab in
            PhoneBrowserView(tab: tab, viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM)
                .interactiveDismissDisabled(true)
//                .presentationDetents([.large])
//                .presentationDragIndicator(.hidden)
                .asNavigationTransition(id: tab.id, in: animation)
        }
        .onReceive(NotificationCenter.default.publisher(for: .exitFullscreenFromRN)) { _ in
            // RN 请求退出全屏 - 先截图再关闭
            if let tab = fullscreenTab {
                takeSnapshot(for: tab)
            }
            fullscreenTab = nil
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
            }
        }
        .onAppear {
            // 初始进入时，如果有选中的 tab，延迟进入全屏模式，确保应用完全启动及布局完成
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                if let currentId = viewModel.currentTabId,
                   let tab = viewModel.tabs.first(where: { $0.id == currentId }) {
                    fullscreenTab = tab
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
    }
    
    /// 截取当前全屏 tab 的快照
    private func takeSnapshot(for tab: Tab) {
        // 延迟一帧，确保捕获正确的内容
        DispatchQueue.main.async {
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
                let cropHeight = screenWidth * 0.54
                
                // 垂直居中
                let cropY = (screenHeight - cropHeight) / 2
                
                let cropRect = CGRect(
                    x: 0,
                    y: cropY,
                    width: screenWidth,
                    height: cropHeight
                )
                
                if let croppedImage = fullImage.cropped(to: cropRect) {
                    viewModel.tabSnapshots[tab.id] = croppedImage
                } else {
                    // 如果裁剪失败，保留全屏图作为 fallback
                    viewModel.tabSnapshots[tab.id] = fullImage
                }
                
                // 发送退出全屏通知到 RN
                NotificationCenter.default.post(
                    name: .tabExitedFullscreenFromNative,
                    object: nil,
                    userInfo: ["tabId": tab.id.uuidString]
                )
            }
        }
    }
}
