import SwiftUI

struct PhoneMainView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    
    @State private var selectedTab: PhoneTab = .tabs
    @State private var fullscreenTab: Tab?
    
    enum PhoneTab {
        case bookmarks
        case tabs
        case settings
    }
    
    var body: some View {
        TabView(selection: $selectedTab) {
            // 书签页：使用 RN 渲染
            PhoneSystemAppView(
                systemType: .bookmarks,
                viewModel: viewModel,
                historyVM: historyVM,
                bookmarkVM: bookmarkVM,
                onSelect: { tab in
                    fullscreenTab = tab
                }
            )
            .tabItem {
                Label("书签", systemImage: "bookmark.fill")
            }
            .tag(PhoneTab.bookmarks)
            
            PhoneTabListView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM, onSelect: { tab in
                fullscreenTab = tab
            })
            .tabItem {
                Label("页签", systemImage: "square.grid.2x2.fill")
            }
            .tag(PhoneTab.tabs)
            
            // 设置页：使用 RN 渲染
            PhoneSystemAppView(
                systemType: .settings,
                viewModel: viewModel,
                historyVM: historyVM,
                bookmarkVM: bookmarkVM,
                onSelect: { tab in
                    fullscreenTab = tab
                }
            )
            .tabItem {
                Label("偏好设置", systemImage: "gearshape.fill")
            }
            .tag(PhoneTab.settings)
        }
        .accentColor(.blue)
        .sheet(item: $fullscreenTab) { tab in
            PhoneBrowserView(tab: tab, viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM)
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
