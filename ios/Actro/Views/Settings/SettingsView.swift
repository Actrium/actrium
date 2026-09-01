import SwiftUI

struct SettingsView: View {
    let systemType: SystemTabType
    @ObservedObject var viewModel: BrowserViewModel

    @State private var systemTab: Tab?

    var body: some View {
        GeometryReader { geometry in
            if let tab = systemTab {
                ZStack {
                    SingleTabRootView(
                        tab: tab,
                        viewModel: viewModel,
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
                .background(Color.black)
            }
        }
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
