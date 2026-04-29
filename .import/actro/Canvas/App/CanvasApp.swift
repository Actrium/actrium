import SwiftUI

private let logger = AppLogger.app

// MARK: - Focused Value for Active Window's ViewModel

struct FocusedBrowserViewModelKey: FocusedValueKey {
    typealias Value = BrowserViewModel
}

extension FocusedValues {
    var browserViewModel: BrowserViewModel? {
        get { self[FocusedBrowserViewModelKey.self] }
        set { self[FocusedBrowserViewModelKey.self] = newValue }
    }
}

// MARK: - Browser Window View (wraps ContentView with its own ViewModel)

struct BrowserWindowView: View {
    @StateObject private var themeManager = ThemeManager.shared
    @StateObject private var viewModel = BrowserViewModel()
    @ObservedObject var historyVM: HistoryViewModel
    
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        ContentView(viewModel: viewModel, historyVM: historyVM)
            .environmentObject(themeManager)
            .focusedSceneValue(\.browserViewModel, viewModel)
            .onChange(of: colorScheme, { oldValue, newValue in
                appLogger.info("====== oldValue: \(oldValue), newValue: \(newValue)")
                themeManager.isDarkMode = newValue == .dark
            })
            .onAppear {
                // Check for pending group transfer
                checkPendingGroupTransfer()
            }
            // Handle bookmark open from RN
            .onReceive(NotificationCenter.default.publisher(for: .openBookmarkFromRN)) { notification in
                guard let userInfo = notification.userInfo,
                      let url = userInfo["url"] as? URL else { return }

                // Check if it's a directory or .actr file
                var isDirectory: ObjCBool = false
                if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) {
                    if isDirectory.boolValue {
                        // It's a directory - use dev directory loader
                        viewModel.loadFromDevDirectory(url)
                    } else {
                        // It's a file - use ACTR package loader
                        viewModel.importACTRPackage(from: url)
                    }
                }
            }
            // Handle history item open from RN
            .onReceive(NotificationCenter.default.publisher(for: .openHistoryItemFromRN)) { notification in
                guard let userInfo = notification.userInfo,
                      let url = userInfo["url"] as? URL else { return }

                // Check if it's a directory or .actr file
                var isDirectory: ObjCBool = false
                if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) {
                    if isDirectory.boolValue {
                        viewModel.loadFromDevDirectory(url)
                    } else {
                        viewModel.importACTRPackage(from: url)
                    }
                }
            }
            // Handle history item delete from RN
            .onReceive(NotificationCenter.default.publisher(for: .deleteHistoryItemFromRN)) { notification in
                guard let userInfo = notification.userInfo,
                      let itemId = userInfo["itemId"] as? String,
                      let uuid = UUID(uuidString: itemId),
                      let item = historyVM.recentItems.first(where: { $0.id == uuid }) else { return }
                historyVM.deleteItem(item)
            }
            // Handle clear history from RN
            .onReceive(NotificationCenter.default.publisher(for: .clearHistoryFromRN)) { _ in
                historyVM.clearHistory()
            }
            // Handle open system tab from RN
            .onReceive(NotificationCenter.default.publisher(for: .openTabFromRN)) { notification in
                guard let userInfo = notification.userInfo,
                      let systemAppTypeStr = userInfo["systemAppType"] as? String,
                      let systemTabType = SystemTabType(rawValue: systemAppTypeStr) else { return }
                let additionalProps = userInfo["additionalProps"] as? [String: Any]
                
                withAnimation {
                    let _ = viewModel.openSystemTab(
                        systemTabType,
                        additionalProps: additionalProps,
                        replaceAdditionalProps: additionalProps != nil
                    )
                }
            }
            // Handle select tab from RN
            .onReceive(NotificationCenter.default.publisher(for: .selectTabFromRN)) { notification in
                guard let userInfo = notification.userInfo,
                      let tabIdStr = userInfo["tabId"] as? String,
                      let tabId = UUID(uuidString: tabIdStr) else { return }
                
                withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                    viewModel.switchToTab(id: tabId)
                }
            }
            // Handle close tab from RN
            .onReceive(NotificationCenter.default.publisher(for: .closeTabFromRN)) { notification in
                guard let userInfo = notification.userInfo,
                      let tabIdStr = userInfo["tabId"] as? String,
                      let tabId = UUID(uuidString: tabIdStr) else { return }
                
                withAnimation {
                    viewModel.closeTab(id: tabId)
                }
            }
    }

    private func checkPendingGroupTransfer() {
        guard let groupData = UserDefaults.standard.dictionary(forKey: "pendingGroupTransfer"),
              let tabsData = groupData["tabs"] as? [[String: Any]],
              !tabsData.isEmpty else {
            return
        }

        // Clear the pending transfer immediately
        UserDefaults.standard.removeObject(forKey: "pendingGroupTransfer")

        let groupName = groupData["groupName"] as? String ?? ""
        let colorRaw = groupData["groupColor"] as? String ?? "gray"
        let color = TabGroupColor(rawValue: colorRaw) ?? .gray

        // Create tabs from serialized data
        var newTabIds: [UUID] = []
        for tabData in tabsData {
            let title = tabData["title"] as? String ?? "Untitled"
            let urlString = tabData["urlString"] as? String
            let url = urlString.flatMap { URL(string: $0) }

            let tab = Tab(
                title: title,
                url: url,
                htmlContent: tabData["htmlContent"] as? String,
                guestScript: tabData["guestScript"] as? String,
                leftPanelScript: tabData["leftPanelScript"] as? String,
                rightPanelScript: tabData["rightPanelScript"] as? String,
                unifiedScript: tabData["unifiedScript"] as? String,
                contractName: tabData["contractName"] as? String,
                contractVersion: tabData["contractVersion"] as? Int,
                permissions: tabData["permissions"] as? [String] ?? [],
                rightPanelDefaultVisible: tabData["rightPanelDefaultVisible"] as? Bool ?? false,
                backgroundColor: tabData["backgroundColor"] as? String
            )

            viewModel.tabs.append(tab)
            viewModel.tabSlotPositions[tab.id] = viewModel.tabs.count - 1
            newTabIds.append(tab.id)
        }

        // Set current tab to first imported tab
        if let firstId = newTabIds.first {
            viewModel.currentTabId = firstId
        }

        // Create group from imported tabs
        if !newTabIds.isEmpty {
            viewModel.createGroupFromTabs(newTabIds, name: groupName, color: color)
        }
    }
}

// MARK: - App

@main
struct CanvasApp: App {
    // 注入自定义 AppDelegate 以满足 React Native 原生层对 window 属性的依赖
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    // Shared view models for history/bookmarks (global across all windows)
    @StateObject private var historyVM = HistoryViewModel()
    @StateObject private var windowState = WindowStateManager.shared
    @Environment(\.openWindow) private var openWindow

    // The focused window's viewModel
    @FocusedValue(\.browserViewModel) private var focusedViewModel

    init() {
        // Suppress common system warnings in development
        UserDefaults.standard.set(false, forKey: "NSConstraintBasedLayoutLogUnsatisfiable")
        
        LoginManager.shared.register()

        #if DEBUG
        logger.notice("⚠️ 注意：控制台中关于 Metal shader、字体沙盒、WebKit 的警告是系统级警告，不影响应用运行")
        #endif
    }

    var body: some Scene {
        // Main window group - each window gets its own BrowserViewModel
        WindowGroup(id: "browser") {
            BrowserWindowView(historyVM: historyVM)
                .onReceive(NotificationCenter.default.publisher(for: .openNewWindowWithGroup)) { _ in
                    openWindow(id: "browser")
                }
                .onOpenURL { url in
                    // SwiftUI App lifecycle may deliver URL callbacks here instead of AppDelegate.
                    // 微信回调路径包含 weChatAppId，需优先交给微信处理；
                    // AskAway 通用链接（路径不含 weChatAppId）由各端视图 onOpenURL 处理。
                    if url.path.contains(AppConstants.weChatAppId) {
                        _ = WeChatManager.shared.handleOpenURL(url)
                    }
                }
        }
        .commands {
            // New Window command
            CommandGroup(before: .newItem) {
                Button("New Window") {
                    openNewWindow()
                }
                .keyboardShortcut("n", modifiers: [.command, .shift])
            }

            // History menu
            CommandMenu("History") {
                Button("Show All History") {
                    focusedViewModel?.openSystemTab(.history)
                }
                .keyboardShortcut("y", modifiers: .command)
                .disabled(focusedViewModel == nil)

                Divider()

                if historyVM.recentItems.isEmpty {
                    Text("No Recent History")
                        .foregroundColor(.secondary)
                } else {
                    ForEach(historyVM.recentItems.prefix(10)) { item in
                        Button(item.title) {
                            openHistoryItem(item)
                        }
                    }
                }

                Divider()

                Button("Clear History...") {
                    historyVM.clearHistory()
                }
                .disabled(historyVM.recentItems.isEmpty)
            }
            
            // Help menu
            CommandGroup(replacing: .help) {
                Button("About App Status") {
                    logger.notice("✅ App running normally")
                    logger.notice("✅ All views loaded")
                }
            }

            // Tab management shortcuts
            CommandGroup(after: .newItem) {
                Button("新建标签页") {
                    withAnimation {
                        _ = focusedViewModel?.addNewTab()
                    }
                }
                .keyboardShortcut("t", modifiers: .command)
                .disabled(focusedViewModel == nil)

                Button("关闭当前标签页") {
                    withAnimation {
                        focusedViewModel?.closeCurrentTab()
                    }
                }
                .keyboardShortcut("w", modifiers: .command)
                .disabled(focusedViewModel == nil)

                Divider()

                Button("刷新当前标签页") {
                    focusedViewModel?.reloadCurrentTab()
                }
                .keyboardShortcut("r", modifiers: .command)
                .disabled(focusedViewModel == nil)
            }

            // Tab switching shortcuts
            CommandGroup(after: .sidebar) {
                ForEach(1...8, id: \.self) { index in
                    Button("切换到标签 \(index)") {
                        switchToTab(at: index - 1)
                    }
                    .keyboardShortcut(
                        KeyEquivalent(Character("\(index)")),
                        modifiers: .command
                    )
                    .disabled(focusedViewModel == nil)
                }

                Button("切换到最后一个标签") {
                    switchToLastTab()
                }
                .keyboardShortcut("9", modifiers: .command)
                .disabled(focusedViewModel == nil)
            }
        }
    }

    private func openNewWindow() {
        openWindow(id: "browser")
    }

    private func switchToTab(at index: Int) {
        guard let vm = focusedViewModel,
              index >= 0, index < vm.tabs.count else { return }
        let tab = vm.tabs[index]
        withAnimation {
            vm.switchToTab(id: tab.id)
        }
    }

    private func switchToLastTab() {
        guard let vm = focusedViewModel,
              !vm.tabs.isEmpty else { return }
        let lastIndex = vm.tabs.count - 1
        let tab = vm.tabs[lastIndex]
        withAnimation {
            vm.switchToTab(id: tab.id)
        }
    }

    private func openHistoryItem(_ item: HistoryItem) {
        guard let vm = focusedViewModel else { return }
        if let url = item.appURL {
            vm.importACTRPackage(from: url)
            historyVM.recordVisit(item: item)
        }
    }
}

extension Notification.Name {
    static let openNewWindowWithGroup = Notification.Name("openNewWindowWithGroup")
}
