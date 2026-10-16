import SwiftUI

private enum PhoneTabNotificationContract {
    static let systemAppType = RuntimeBridgeContract.systemAppType
    static let tabId = RuntimeBridgeContract.tabId
}

struct PhoneMainView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel

    @State private var selectedTab: PhoneTab = .tabs
    @State private var fullscreenTab: Tab?
    @State private var skipManualSettingsResetOnce = false
    @State private var searchText: String = ""
    @FocusState private var isSearchFocused: Bool
    
    @Namespace private var animation
    
    enum PhoneTab {
        case bookmarks
        case tabs
        case settings
        case search
    }
    
    var body: some View {
        TabView(selection: $selectedTab) {
            // Bookmark Page: Use RN Render
            SwiftUI.Tab(TM.active.bookmarks(), systemImage: "book.fill", value: PhoneTab.bookmarks) {
                PhoneSystemAppView(
                    systemType: .bookmarks,
                    viewModel: viewModel,
                    historyVM: historyVM
                )
            }
            
            SwiftUI.Tab(TM.active.activity(), systemImage: "rectangle.grid.1x2.fill", value: PhoneTab.tabs) {
                PhoneTabListView(viewModel: viewModel, historyVM: historyVM, animation: animation, onSelect: { tab in
                    fullscreenTab = tab
                })
            }
            
            // Settings Page: Use RN Render
            SwiftUI.Tab(TM.active.preferences(), systemImage: "gearshape.fill", value: PhoneTab.settings) {
                PhoneSystemAppView(
                    systemType: .settings,
                    viewModel: viewModel,
                    historyVM: historyVM
                )
            }
            
            // Search Page: Independent display (no fixed occupation) tab Location)
            SwiftUI.Tab(TM.active.search(), systemImage: "magnifyingglass", value: PhoneTab.search, role: .search) {
                NavigationStack {
                    PhoneSystemAppView(
                        systemType: .globalSearch,
                        viewModel: viewModel,
                        historyVM: historyVM
                    )
                }
                .searchable(text: $searchText)
                .searchFocused($isSearchFocused)
                .task {
                    isSearchFocused = true
                }
            }
        }
        .accentColor(.blue)
        .background(Color.black)
        .onChange(of: selectedTab) { oldValue, newValue in
            // Empty search box text when leaving search page
            if oldValue == .search {
                searchText = ""
                isSearchFocused = false
            }
            // Refresh system pagemarks when switching to system page signers
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
                break // Pagemark List does not need to be refreshed
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .exitFullscreenFromRN)) { _ in
            // RN Request to exit the full screen. Finish the screenshot when the title exists and then execute the exit to avoid a semi-release.
            if let tab = viewModel.tabs.first(where: { $0.id == fullscreenTab?.id}), !tab.title.isEmpty {
                viewModel.skipNextDisappearSnapshot = true
                takeSnapshot(for: tab) {
                    fullscreenTab = nil
                }
            } else {
                fullscreenTab = nil
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .exitSearchFromRN)) { _ in
            // RN Request to exit the search page - Toggle Back Page Signing List
            withAnimation {
                selectedTab = .tabs
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .switchPhoneTabFromRN)) { notification in
            guard let systemAppType = notification.userInfo?[PhoneTabNotificationContract.systemAppType] as? String else {
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
            // RN Request to open the designation tab
            if let tabIdString = notification.userInfo?[PhoneTabNotificationContract.tabId] as? String,
               let tabId = UUID(uuidString: tabIdString),
               let tab = viewModel.tabs.first(where: { $0.id == tabId }) {
                fullscreenTab = tab
                selectedTab = .tabs
            }
        }
        .onAppear {
            // When entering at the beginning, if selected tabDelay in fullscreen mode to ensure full start and layout of applications
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                if let currentId = viewModel.currentTabId,
                   let tab = viewModel.tabs.first(where: { $0.id == currentId }) {
                    fullscreenTab = tab
                }
            }
        }
        .sheet(item: $fullscreenTab) { tab in
            PhoneBrowserView(tab: tab, viewModel: viewModel, historyVM: historyVM)
                .ignoresSafeArea()
                .asNavigationTransition(id: tab.id, in: animation)
                .onAppear {
                    // sheet After the popup animation is finished, pending Add a new tab to the list
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                        viewModel.commitPendingNewTab()
                    }
                }
        }
        .onChange(of: viewModel.currentTabId) { oldValue, newValue in
            guard let newValue = newValue else {
                fullscreenTab = nil
                return
            }
            
            // Ensure that tabs displayed in fullscreen view are compared to currentTabId Keep Sync
            if fullscreenTab?.id != newValue {
                if let tab = viewModel.tabs.first(where: { $0.id == newValue }) {
                    fullscreenTab = tab
                }
            }
        }
        .onChange(of: fullscreenTab?.id) { oldValue, newValue in
            viewModel.fullscreenTabId = newValue
        }
        .onChange(of: searchText) { _, newText in
            // Send a real-time notification to the search box when content changes RN Side
            NotificationCenter.default.post(
                name: .searchInputChangedFromNative,
                object: nil,
                userInfo: [
                    "inputText": newText
                ]
            )
        }
        .onOpenURL { url in
            handleUniversalLink(url)
        }
    }

    /// Processing generic links: matching Actrium Create new pagemark and open full screen when a domain name is created
    private func handleUniversalLink(_ url: URL) {
        guard let appId = ActriumUniversalLink.appId(from: url) else { return }

        // Create a new page sign, which will appId Import RN Side
        let newTab = Tab(
            title: TM.active.blackTab(),
            appId: appId
        )
        viewModel.tabs.append(newTab)
        viewModel.tabSlotPositions[newTab.id] = viewModel.tabs.count - 1
        viewModel.currentTabId = newTab.id
        selectedTab = .tabs
        fullscreenTab = newTab
    }

    /// Capture the current full screen tab Snippets
    private func takeSnapshot(for tab: Tab, completion: @escaping () -> Void) {
        // Delay a frame to ensure that the right contents are captured.
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
                
                // Intercept central section of the page: width = Page width, height = Width * 0.54
                let screenWidth = window.bounds.width
                let screenHeight = window.bounds.height
                let cropHeight = screenWidth * 0.54 - 56
                
                // Centre Vertically
                let cropY = (screenHeight - cropHeight) / 2
                
                let cropRect = CGRect(
                    x: 0,
                    y: cropY,
                    width: screenWidth,
                    height: cropHeight
                )
                
                if let croppedImage = fullImage.cropped(to: cropRect) {
                    viewModel.setTabSnapshot(croppedImage, for: tab.id)
                } else {
                    // If crop failed, keep full screen as fallback
                    viewModel.setTabSnapshot(fullImage, for: tab.id)
                }
                
            }
        }
    }
}
