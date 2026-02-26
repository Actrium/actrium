import SwiftUI

/// Phone 端系统应用视图（History/Bookmarks）
/// 使用 RN 渲染，并传入 systemAppType
struct PhoneSystemAppView: View {
    let systemType: SystemTabType
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel
    var bookmarkVM: BookmarkViewModel
    var onSelect: (Tab) -> Void
    
    @State private var systemTab: Tab?
    
    var body: some View {
        GeometryReader { geometry in
            if let tab = systemTab {
                SingleTabRootView(
                    tab: tab,
                    viewModel: viewModel,
                    historyVM: historyVM,
                    bookmarkVM: bookmarkVM,
                    containerSize: geometry.size
                )
                .id(tab.id)
            } else {
                // 占位视图
                VStack {
                    ProgressView()
                        .progressViewStyle(CircularProgressViewStyle())
                    Text("加载中...")
                        .foregroundColor(.gray)
                        .padding(.top, 8)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.black)
            }
        }
        .onAppear {
            loadSystemTab()
        }
        .onChange(of: systemType) { _, _ in
            loadSystemTab()
        }
    }
    
    private func loadSystemTab() {
        // 获取系统标签页（不会加入到 tabs 数组）
        let tab = viewModel.openSystemTab(systemType)
        systemTab = tab
        
        // 确保 bookmarkVM 数据已加载
        if systemType == .bookmarks {
            bookmarkVM.loadAll()
        }
    }
}
