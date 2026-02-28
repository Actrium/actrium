import SwiftUI

struct PhoneMainView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    
    @State private var selectedTab: PhoneTab = .tabs
    @State private var fullscreenTab: Tab?
    @State private var tabRefreshKeys: [PhoneTab: Int] = [:]
    
    enum PhoneTab: Hashable {
        case bookmarks
        case tabs
        case settings
        case search
    }
    
    var body: some View {
        TabView(selection: $selectedTab) {
            // 书签页：使用 RN 渲染
            SwiftUI.Tab("书签", systemImage: "bookmark.fill", value: PhoneTab.bookmarks) {
                PhoneSystemAppView(
                    systemType: .bookmarks,
                    viewModel: viewModel,
                    historyVM: historyVM,
                    bookmarkVM: bookmarkVM,
                    onSelect: { tab in
                        fullscreenTab = tab
                    }
                )
                .id("bookmarks-\(tabRefreshKeys[.bookmarks, default: 0])")
            }
            
            SwiftUI.Tab("页签", systemImage: "square.grid.2x2.fill", value: PhoneTab.tabs) {
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
                .id("settings-\(tabRefreshKeys[.settings, default: 0])")
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
                .id("search-\(tabRefreshKeys[.search, default: 0])")
            }
        }
        .accentColor(.blue)
        .onChange(of: selectedTab) { oldValue, newValue in
            // 切换到系统页签时，增加刷新计数以触发重新加载
            if [.bookmarks, .settings, .search].contains(newValue) {
                tabRefreshKeys[newValue, default: 0] += 1
            }
        }
        .sheet(item: $fullscreenTab) { tab in
            PhoneBrowserView(tab: tab, viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM)
                .interactiveDismissDisabled(true)
        }
        .onReceive(NotificationCenter.default.publisher(for: .exitFullscreenFromRN)) { _ in
            // RN 请求退出全屏
            fullscreenTab = nil
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
            // 初始进入时，如果有选中的 tab，直接进入全屏模式
            if let currentId = viewModel.currentTabId,
               let tab = viewModel.tabs.first(where: { $0.id == currentId }) {
                fullscreenTab = tab
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
    }
}
