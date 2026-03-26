import SwiftUI

struct PhoneMainView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    
    @State private var selectedTab: PhoneTab = .tabs
    @State private var fullscreenTab: Tab?
    @State private var skipManualSettingsResetOnce = false
    
    // 自定义转场动画状态
    @State private var transitioningTab: Tab?
    @State private var showBrowser = false
    @State private var transitionRect: CGRect = .zero
    @State private var isAnimating = false
    
    @Namespace private var animation
    
    enum PhoneTab {
        case bookmarks
        case tabs
        case settings
        case search
    }
    
    var body: some View {
        ZStack {
            TabView(selection: $selectedTab) {
                // 书签页：使用 RN 渲染
                SwiftUI.Tab("书签", systemImage: "book.fill", value: PhoneTab.bookmarks) {
                    PhoneSystemAppView(
                        systemType: .bookmarks,
                        viewModel: viewModel,
                        historyVM: historyVM,
                        bookmarkVM: bookmarkVM,
                        onSelect: { tab in
                            startTransition(to: tab)
                        }
                    )
                }
                
                SwiftUI.Tab("页签", systemImage: "rectangle.grid.1x2.fill", value: PhoneTab.tabs) {
                    PhoneTabListView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM, animation: animation, onSelect: { tab in
                        startTransition(to: tab)
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
                            startTransition(to: tab)
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
                            startTransition(to: tab)
                        }
                    )
                    .toolbar(.hidden, for: .tabBar)
                }
            }
            .accentColor(.blue)
            .opacity(showBrowser ? 0 : 1)
            
            transitionReceiver
            
            // 自定义浏览器层
            if let tab = transitioningTab {
                GeometryReader { geometry in
                    let screenWidth = geometry.size.width
                    let topInset = geometry.safeAreaInsets.top

                    let browserHeight = showBrowser
                    ? (UIScreen.main.bounds.height - topInset)
                        : (transitionRect.width * 0.54)

                    PhoneBrowserView(
                        tab: tab,
                        viewModel: viewModel,
                        historyVM: historyVM,
                        bookmarkVM: bookmarkVM
                    )
                    .frame(
                        width: showBrowser ? screenWidth : transitionRect.width,
                        height: browserHeight
                    )
                    .cornerRadius(showBrowser ? 16 : 22)
                    .position(
                        x: showBrowser ? screenWidth / 2 : transitionRect.midX,
                        y: showBrowser
                            ? (browserHeight / 2)
                            : (transitionRect.midY - topInset)
                    )
                    .zIndex(100)
                }
            }
        }
        .background(Color.black)
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
        .onReceive(NotificationCenter.default.publisher(for: .exitFullscreenFromRN)) { _ in
            // RN 请求退出全屏。存在标题时先完成截图，再执行退场，避免快照拿到半退场画面。
            if let tab = transitioningTab, !tab.title.isEmpty {
                takeSnapshot(for: tab) {
                    dismissBrowser(exitingTab: tab)
                }
            } else {
                dismissBrowser()
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
                startTransition(to: tab)
            }
        }
        .onAppear {
            // 初始进入时，如果有选中的 tab，延迟进入全屏模式，确保应用完全启动及布局完成
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                if let currentId = viewModel.currentTabId,
                   let tab = viewModel.tabs.first(where: { $0.id == currentId }) {
                    startTransition(to: tab)
                }
            }
        }
        .onChange(of: viewModel.currentTabId) { oldValue, newValue in
            guard let newValue = newValue else {
                dismissBrowser()
                return
            }
            
            // 确保全屏视图显示的标签页与 currentTabId 保持同步
            if transitioningTab?.id != newValue {
                if let tab = viewModel.tabs.first(where: { $0.id == newValue }) {
                    startTransition(to: tab)
                }
            }
        }
        .onChange(of: transitioningTab?.id) { oldValue, newValue in
            viewModel.fullscreenTabId = newValue
        }
    }
    
    private func startTransition(to tab: Tab) {
        // 如果当前是 Tabs 页签列表，则尝试从通知获取位置，否则直接中心放大
        if selectedTab == .tabs {
            // 发送通知获取卡片位置
            NotificationCenter.default.post(name: NSNotification.Name("GetTabCardRect"), object: nil, userInfo: ["tabId": tab.id])
            
            // 延迟一帧，等待 UpdateTabCardRect 通知更新 transitionRect
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.01) {
                executeTransition(to: tab)
            }
        } else {
            // 从其他页签（书签/设置等）打开，直接从底部弹出
            let width = UIScreen.main.bounds.width
            transitionRect = CGRect(x: 0, y: UIScreen.main.bounds.height, width: width, height: width * 0.54)
            executeTransition(to: tab)
        }
    }
    
    private func executeTransition(to tab: Tab) {
        transitioningTab = tab
        
        withAnimation(.spring(response: 0.45, dampingFraction: 0.85)) {
            showBrowser = true
        }
    }
    
    private func dismissBrowser(exitingTab: Tab? = nil) {
        let resolvedExitingTab = exitingTab ?? transitioningTab

        // 退出时，如果是从 Tabs 进来的，尝试刷新位置
        if selectedTab == .tabs, let tab = resolvedExitingTab {
            NotificationCenter.default.post(name: NSNotification.Name("GetTabCardRect"), object: nil, userInfo: ["tabId": tab.id])
        }

        withAnimation(.spring(response: 0.35, dampingFraction: 0.9)) {
            showBrowser = false
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
            // 退出完成事件必须跟退场动画绑定，避免截图分支与无截图分支行为不一致。
            if let tab = resolvedExitingTab {
                NotificationCenter.default.post(
                    name: .tabExitedFullscreenFromNative,
                    object: nil,
                    userInfo: ["tabId": tab.id.uuidString]
                )
            }
            transitioningTab = nil
        }
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
                
            }
        }
    }
}

extension PhoneMainView {
    /// 这里的 View 用于接收来自卡片的位置更新通知
    var transitionReceiver: some View {
        Color.clear
            .frame(width: 0, height: 0)
            .onReceive(NotificationCenter.default.publisher(for: NSNotification.Name("UpdateTabCardRect"))) { notification in
                if let rect = notification.userInfo?["rect"] as? CGRect {
                    if self.transitionRect == .zero {
                        self.transitionRect = CGRect(x: rect.origin.x, y: rect.origin.y, width: rect.size.width+32, height: rect.size.height)
                    } else {
                        self.transitionRect = rect
                    }
                }
            }
    }
}
