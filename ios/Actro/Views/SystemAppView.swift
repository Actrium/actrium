import SwiftUI

/// iPad End-system application view (e.g. bookmarks)/Set Page)
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
                // Placeview
                VStack {
                    ProgressView()
                        .progressViewStyle(CircularProgressViewStyle())
                    Text("Loading...")
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
            // When? reloadNonce Retake when Change tab
            loadSystemTab()
        }
    }

    private func loadSystemTab() {
        // Get System Tabs (do not add to tabs array)
        let tab = viewModel.openSystemTab(systemType)
        systemTab = tab
    }
}
