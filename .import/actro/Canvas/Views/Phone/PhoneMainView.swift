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
            PhoneBookmarkListView(viewModel: viewModel, bookmarkVM: bookmarkVM, onSelect: { tab in
                fullscreenTab = tab
            })
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
            
            SettingsView()
                .tabItem {
                    Label("偏好设置", systemImage: "gearshape.fill")
                }
                .tag(PhoneTab.settings)
        }
        .accentColor(.blue)
        .fullScreenCover(item: $fullscreenTab) { tab in
            PhoneBrowserView(tab: tab, viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM)
        }
        .onChange(of: viewModel.currentTabId) { newValue in
            if let newValue = newValue, fullscreenTab == nil {
                if let tab = viewModel.tabs.first(where: { $0.id == newValue }) {
                    fullscreenTab = tab
                }
            }
        }
    }
}
