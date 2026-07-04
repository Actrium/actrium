import SwiftUI

/// iPad 端系统应用视图（如书签/设置页）
struct SystemAppView: View {
    let systemType: SystemTabType
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel

    @State private var systemTab: Tab?

    var body: some View {
        GeometryReader { geometry in
            if let tab = systemTab {
                ZStack {
                    Color.black.ignoresSafeArea()
                    SingleTabRootView(
                        tab: tab,
                        viewModel: viewModel,
                        historyVM: historyVM,
                        containerSize: geometry.size
                    )
                }
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
        .ignoresSafeArea(.container)
        .onAppear {
            loadSystemTab()
        }
        .onChange(of: systemType) { _, _ in
            loadSystemTab()
        }
        .onChange(of: viewModel.systemTabs[systemType]?.reloadNonce) { _, _ in
            // 当 reloadNonce 变化时，重新获取 tab
            loadSystemTab()
        }
    }

    private func loadSystemTab() {
        // 获取系统标签页（不会加入到 tabs 数组）
        let tab = viewModel.openSystemTab(systemType)
        systemTab = tab
    }
}
