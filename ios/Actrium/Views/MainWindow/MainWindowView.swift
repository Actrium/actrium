import SwiftUI

/// The iPad window root shared by windowed and full-screen presentations.
/// Keep its tab, system-page, search, link, and package-drop behavior together.
struct MainWindowView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @State private var isACTRDropTarget = false

    var body: some View {
        ZStack {
            ChromeStyleTabView(viewModel: viewModel, historyVM: historyVM)

            if let systemType = viewModel.currentSystemTabType {
                SystemAppView(systemType: systemType, viewModel: viewModel, historyVM: historyVM)
                    .padding(.top, 44)
            }

            if case .overview = viewModel.actriumMode,
               viewModel.currentSystemTabType != .globalSearch {
                SearchBar(viewModel: viewModel)
            }
        }
        .background(Color.black)
        .accessibilityIdentifier("actrium.main-window")
        .onReceive(NotificationCenter.default.publisher(for: .exitSearchFromRN)) { _ in
            withAnimation {
                if let previous = viewModel.previousSystemTabType {
                    viewModel.currentSystemTabType = previous
                    viewModel.previousSystemTabType = nil
                } else {
                    viewModel.currentSystemTabType = nil
                }
            }
        }
        .onOpenURL(perform: handleUniversalLink)
        .dropDestination(for: URL.self) { urls, _ in
            guard let url = urls.first else { return false }

            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
                return false
            }

            if isDirectory.boolValue {
                let manifestPath = url.appendingPathComponent("manifest.toml").path
                guard FileManager.default.fileExists(atPath: manifestPath) else { return false }
                Task { @MainActor in viewModel.loadFromDevDirectory(url) }
                return true
            }

            guard url.pathExtension.lowercased() == "actr" else { return false }
            Task { @MainActor in viewModel.importACTRPackage(from: url) }
            return true
        } isTargeted: { isTargeted in
            isACTRDropTarget = isTargeted
        }
    }

    private func handleUniversalLink(_ url: URL) {
        guard let appId = ActriumUniversalLink.appId(from: url) else { return }

        let newTab = Tab(
            title: TM.active.blackTab(),
            appId: appId,
            unifiedScript: "/* welcome page shell */"
        )
        viewModel.tabs.append(newTab)
        viewModel.tabSlotPositions[newTab.id] = viewModel.tabs.count - 1
        viewModel.currentSystemTabType = nil
        viewModel.switchToTab(id: newTab.id)
    }
}
