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
    @StateObject private var viewModel = BrowserViewModel()
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel

    var body: some View {
        ContentView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM)
            .focusedSceneValue(\.browserViewModel, viewModel)
            .onAppear {
                // Register with DevTools for detached window support
                DevToolsManager.shared.activeBrowserViewModel = viewModel
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

                // Mark bookmark as used if we can find it
                if let bookmarkId = userInfo["bookmarkId"] as? String,
                   let uuid = UUID(uuidString: bookmarkId),
                   let bookmark = bookmarkVM.bookmarks.first(where: { $0.id == uuid }) {
                    bookmarkVM.markAsUsed(bookmark)
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
            // Handle bookmark delete from RN
            .onReceive(NotificationCenter.default.publisher(for: .deleteBookmarkFromRN)) { notification in
                guard let userInfo = notification.userInfo,
                      let bookmarkId = userInfo["bookmarkId"] as? String,
                      let uuid = UUID(uuidString: bookmarkId) else { return }
                bookmarkVM.removeBookmark(id: uuid)
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
    // Shared view models for history/bookmarks (global across all windows)
    @StateObject private var historyVM = HistoryViewModel()
    @StateObject private var bookmarkVM = BookmarkViewModel()
    @StateObject private var windowState = WindowStateManager.shared
    @ObservedObject private var devTools = DevToolsManager.shared
    @Environment(\.openWindow) private var openWindow

    // The focused window's viewModel
    @FocusedValue(\.browserViewModel) private var focusedViewModel

    init() {
        // Suppress common system warnings in development
        UserDefaults.standard.set(false, forKey: "NSConstraintBasedLayoutLogUnsatisfiable")

        #if DEBUG
        logger.notice("⚠️ 注意：控制台中关于 Metal shader、字体沙盒、WebKit 的警告是系统级警告，不影响应用运行")
        #endif

        // Initialize TabProcessManager early
        _ = TabProcessManager.shared

        // Swizzle NSWindow.sendEvent to intercept magnify gestures
        MagnifyEventInterceptor.install()
    }

    var body: some Scene {
        // Main window group - each window gets its own BrowserViewModel
        WindowGroup("Canvas0", id: "browser") {
            BrowserWindowView(historyVM: historyVM, bookmarkVM: bookmarkVM)
                .onReceive(NotificationCenter.default.publisher(for: .openNewWindowWithGroup)) { _ in
                    openWindow(id: "browser")
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

            // Bookmarks menu
            CommandMenu("Bookmarks") {
                Button("Add Bookmark...") {
                    // TODO: Show bookmark sheet for current tab
                }
                .keyboardShortcut("d", modifiers: .command)
                .disabled(focusedViewModel == nil)

                Button("Show All Bookmarks") {
                    focusedViewModel?.openSystemTab(.bookmarks)
                }
                .keyboardShortcut("b", modifiers: [.command, .option])
                .disabled(focusedViewModel == nil)

                Divider()

                if bookmarkVM.bookmarks.isEmpty {
                    Text("No Bookmarks")
                        .foregroundColor(.secondary)
                } else {
                    ForEach(bookmarkVM.bookmarks.prefix(10)) { bookmark in
                        Button(bookmark.title) {
                            openBookmark(bookmark)
                        }
                    }
                }
            }

            // Window menu additions
            CommandGroup(after: .windowList) {
                Divider()

                Button("App Manager") {
                    AppsManagerWindowController.shared.showWindow()
                }
                .keyboardShortcut("m", modifiers: [.command, .shift])
            }

            // DevTools menu
            CommandMenu("Developer") {
                Button("Toggle DevTools") {
                    devTools.toggle()
                }
                .keyboardShortcut("i", modifiers: [.command, .option])

                Divider()

                Button("Open DevTools in New Window") {
                    devTools.detach()
                    openWindow(id: "devtools")
                }
                .keyboardShortcut("i", modifiers: [.command, .option, .shift])

                Button("Dock DevTools") {
                    devTools.dock()
                }
                .disabled(devTools.displayMode != .detached)

                Divider()

                Menu("Dock Position") {
                    Button("Bottom") {
                        devTools.dockPosition = .bottom
                    }
                    .disabled(devTools.dockPosition == .bottom)

                    Button("Right") {
                        devTools.dockPosition = .right
                    }
                    .disabled(devTools.dockPosition == .right)
                }
                .disabled(devTools.displayMode != .docked)

                Divider()

                Button("Clear Console") {
                    devTools.clearConsoleLogs()
                }
                .keyboardShortcut("k", modifiers: [.command, .option])
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

        // Detached DevTools window
        Window("DevTools", id: "devtools") {
            DevToolsWindowView(viewModel: focusedViewModel)
                .frame(minWidth: 600, minHeight: 400)
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 900, height: 500)
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

    private func openBookmark(_ bookmark: Bookmark) {
        guard let vm = focusedViewModel else { return }
        if let url = bookmark.appURL {
            vm.importACTRPackage(from: url)
            bookmarkVM.markAsUsed(bookmark)
            // Also record in history
            if let tab = vm.currentTab {
                historyVM.recordVisit(tab: tab)
            }
        }
    }
}

// MARK: - Magnify Event Interceptor (CGEventTap - Quartz Level)

/// 通过 CGEventTap 在 Quartz 层面拦截手势事件
/// 这是最底层的拦截方式，在事件到达任何窗口之前捕获
final class MagnifyEventInterceptor {
    static var isInstalled = false
    static var cumulativeMagnification: CGFloat = 0
    static var eventTap: CFMachPort?
    static var runLoopSource: CFRunLoopSource?

    /// 安装拦截器（只能调用一次）
    static func install() {
        guard !isInstalled else { return }
        isInstalled = true

        // 创建 CGEventTap 来监听手势事件
        // 使用 .cghidEventTap 位置，这是在 HID 系统之后、窗口系统之前
        let eventMask: CGEventMask = (1 << CGEventType.gesture.rawValue)

        guard let tap = CGEvent.tapCreate(
            tap: .cghidEventTap,  // 在 HID 系统级别拦截
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: eventMask,
            callback: { (proxy, type, event, refcon) -> Unmanaged<CGEvent>? in
                return MagnifyEventInterceptor.handleEvent(proxy: proxy, type: type, event: event)
            },
            userInfo: nil
        ) else {
            fputs("❌ MagnifyEventInterceptor: Failed to create CGEventTap (need Accessibility permissions?)\n", stderr)
            return
        }

        eventTap = tap
        runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)

        if let source = runLoopSource {
            CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
            CGEvent.tapEnable(tap: tap, enable: true)
            fputs("🤏 MagnifyEventInterceptor: CGEventTap installed at HID level\n", stderr)
        }
    }

    /// 处理 CGEvent
    static func handleEvent(proxy: CGEventTapProxy, type: CGEventType, event: CGEvent) -> Unmanaged<CGEvent>? {
        // 获取手势子类型
        let subtype = event.getIntegerValueField(.gestureType)

        // 子类型 8 = magnify (缩放手势)
        // 子类型 6 = rotate (旋转手势)
        // 子类型 1 = swipe (滑动手势)
        if subtype == 8 {  // magnify
            let magnification = event.getDoubleValueField(.gestureZoomValue)
            fputs("🤏 [CGEventTap] magnify: \(magnification), cumulative: \(cumulativeMagnification)\n", stderr)

            cumulativeMagnification += magnification

            // 捏合缩小超过阈值
            if cumulativeMagnification < -0.3 {
                fputs("🤏 [CGEventTap] Threshold reached! Posting notification\n", stderr)
                cumulativeMagnification = 0

                // 在主线程发送通知
                DispatchQueue.main.async {
                    NotificationCenter.default.post(name: .pinchToOverview, object: nil)
                }

                // 返回 nil 消费事件，阻止传递
                return nil
            }
        } else if subtype == 61 {  // gesture begin
            fputs("🤏 [CGEventTap] gesture begin\n", stderr)
            cumulativeMagnification = 0
        } else if subtype == 62 {  // gesture end
            fputs("🤏 [CGEventTap] gesture end\n", stderr)
            cumulativeMagnification = 0
        } else {
            // 其他手势类型，记录一下
            fputs("🤏 [CGEventTap] gesture subtype=\(subtype)\n", stderr)
        }

        // 继续传递事件
        return Unmanaged.passRetained(event)
    }

    /// 清理
    static func uninstall() {
        if let tap = eventTap {
            CGEvent.tapEnable(tap: tap, enable: false)
        }
        if let source = runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), source, .commonModes)
        }
        eventTap = nil
        runLoopSource = nil
        isInstalled = false
    }
}

extension Notification.Name {
    static let pinchToOverview = Notification.Name("pinchToOverview")
}
