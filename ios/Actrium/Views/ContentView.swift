import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var windowState: WindowStateManager = .shared
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    @State private var isACTRDropTarget: Bool = false
    
    @StateObject private var loginManager = LoginManager.shared

    var body: some View {
        Group {
            if UIDevice.current.userInterfaceIdiom == .phone {
                PhoneMainView(viewModel: viewModel, historyVM: historyVM)
            } else {
                // Content Area
                ZStack {
                    ChromeStyleTabView(viewModel: viewModel, historyVM: historyVM)
                    if let systemType = viewModel.currentSystemTabType {
                        SystemAppView(systemType: systemType, viewModel: viewModel, historyVM: historyVM)
                            .padding(.top, 44)
                    }
                    // Search Tabs Search Box - Only Overview Mode shows, centered at bottom
                    if case .overview = viewModel.actriumMode, viewModel.currentSystemTabType != .globalSearch {
                        SearchBar(viewModel: viewModel)
                    }
                }
                .background(Color.black)
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
                .onOpenURL { url in
                    handleUniversalLink(url)
                }
                // Top pocket: Drag .actr Creates a new tab when a directory is created
                .dropDestination(for: URL.self) { urls, location in
                    guard let url = urls.first else {
                        return false
                    }
                    
                    // Check if it's a .actr file or a directory with manifest.toml
                    var isDirectory: ObjCBool = false
                    guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
                        return false
                    }
                    
                    if isDirectory.boolValue {
                        // Directory - check if it has manifest.toml
                        let manifestPath = url.appendingPathComponent("manifest.toml").path
                        guard FileManager.default.fileExists(atPath: manifestPath) else {
                            return false
                        }
                        Task { @MainActor in
                            viewModel.loadFromDevDirectory(url)
                        }
                        return true
                    } else if url.pathExtension.lowercased() == "actr" {
                        // .actr file
                        Task { @MainActor in
                            viewModel.importACTRPackage(from: url)
                        }
                        return true
                    }
                    
                    return false
                } isTargeted: { isTargeted in
                    isACTRDropTarget = isTargeted
                }
            }
        }
    }

    /// Processing generic links: matching Actrium Create a new page sign and enter when a domain name is created Focus Mode
    private func handleUniversalLink(_ url: URL) {
        guard let appId = ActriumUniversalLink.appId(from: url) else { return }

        // Create a new page sign, which will appId Import RN Side
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

struct SearchBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    
    var body: some View {
        VStack {
            Spacer()
            
            HStack {
                Spacer()
                
                HStack(spacing: 12) {
                    Image(systemName: "magnifyingglass")
                        .foregroundColor(Color(hex: "#999"))
                        .font(.system(size: 18))
                    
                    Text("\(TM.active.search())...")
                        .foregroundColor(Color(hex: "#999"))
                    
                    Spacer()
                }
                .padding(.horizontal, 11)
                .frame(maxWidth: 500)
                .frame(height: 48)
                .background(Color.white)
                .cornerRadius(24)
                .onTapGesture {
                    viewModel.previousSystemTabType = viewModel.currentSystemTabType
                    viewModel.openSystemTab(.globalSearch, additionalProps: nil, replaceAdditionalProps: true)
                    viewModel.currentSystemTabType = .globalSearch
                }
                
                Spacer()
            }
            .padding(.bottom, 18)
        }
    }
}

#Preview {
    ContentView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel())
}
