import SwiftUI
import Combine

private let logger = AppLogger.app

// MARK: - DistributedNotificationCenter Publisher Extension

extension DistributedNotificationCenter {
    /// Create a Combine publisher for distributed notifications
    func publisher(for name: NSNotification.Name) -> AnyPublisher<Notification, Never> {
        NotificationCenter.default.publisher(for: name)
            .eraseToAnyPublisher()
    }
}

/// Helper class to bridge DistributedNotificationCenter to NotificationCenter for SwiftUI
final class XPCNotificationBridge {
    static let shared = XPCNotificationBridge()

    // Local notification names (for SwiftUI .onReceive)
    static let openBookmark = Notification.Name("xpc.openBookmark")
    static let openHistoryItem = Notification.Name("xpc.openHistoryItem")
    static let deleteBookmark = Notification.Name("xpc.deleteBookmark")
    static let deleteHistoryItem = Notification.Name("xpc.deleteHistoryItem")
    static let restoreHistoryGroup = Notification.Name("xpc.restoreHistoryGroup")
    static let clearHistory = Notification.Name("xpc.clearHistory")
    static let openURLFromStartPage = Notification.Name("xpc.openURLFromStartPage")
    static let xpcLog = Notification.Name("xpc.log")

    private var observers: [NSObjectProtocol] = []

    private init() {
        setupListeners()
    }

    private func setupListeners() {
        // Bridge distributed notifications to local NotificationCenter

        // Open bookmark
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.openBookmark"),
            object: nil,
            queue: .main
        ) { notification in
            NotificationCenter.default.post(name: Self.openBookmark, object: nil, userInfo: notification.userInfo)
        })

        // Open history item
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.openHistoryItem"),
            object: nil,
            queue: .main
        ) { notification in
            NotificationCenter.default.post(name: Self.openHistoryItem, object: nil, userInfo: notification.userInfo)
        })

        // Delete bookmark
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.deleteBookmark"),
            object: nil,
            queue: .main
        ) { notification in
            NotificationCenter.default.post(name: Self.deleteBookmark, object: nil, userInfo: notification.userInfo)
        })

        // Delete history item
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.deleteHistoryItem"),
            object: nil,
            queue: .main
        ) { notification in
            NotificationCenter.default.post(name: Self.deleteHistoryItem, object: nil, userInfo: notification.userInfo)
        })

        // Restore history group
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.restoreHistoryGroup"),
            object: nil,
            queue: .main
        ) { notification in
            NotificationCenter.default.post(name: Self.restoreHistoryGroup, object: nil, userInfo: notification.userInfo)
        })

        // Clear history
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.clearHistory"),
            object: nil,
            queue: .main
        ) { notification in
            NotificationCenter.default.post(name: Self.clearHistory, object: nil, userInfo: notification.userInfo)
        })

        // Open URL from Start Page
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.openURLFromStartPage"),
            object: nil,
            queue: .main
        ) { notification in
            NotificationCenter.default.post(name: Self.openURLFromStartPage, object: nil, userInfo: notification.userInfo)
        })

        // XPC Log (forward to DevTools)
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.xpcLog"),
            object: nil,
            queue: .main
        ) { notification in
            guard let userInfo = notification.userInfo,
                  let level = userInfo["level"] as? String,
                  let source = userInfo["source"] as? String,
                  let message = userInfo["message"] as? String else { return }

            let logLevel: DevToolsLogEntry.LogLevel
            switch level {
            case "warn": logLevel = .warn
            case "error": logLevel = .error
            case "debug": logLevel = .debug
            default: logLevel = .info
            }

            Task { @MainActor in
                DevToolsManager.shared.log(message, level: logLevel, source: "[XPC] \(source)")
            }
        })

        logger.info("XPCNotificationBridge: Initialized with \(self.observers.count) listeners")
    }
}

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
    @State private var hasHandledLaunchURL = false
    @State private var hasAppliedDebugConfig = false

    var body: some View {
        // No longer wait for Metro - apps load from local bundles
        mainContent
    }

    /// Main browser content
    private var mainContent: some View {
        ContentView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM)
            .focusedSceneValue(\.browserViewModel, viewModel)
            .onAppear {
                // Register with DevTools for detached window support
                DevToolsManager.shared.activeBrowserViewModel = viewModel
                // Check for pending group transfer
                checkPendingGroupTransfer()
                // Handle launch URL (only once)
                handleLaunchURLIfNeeded()
                // Apply debug configuration (only once)
                applyDebugConfigIfNeeded()
            }
            // Handle bookmark open from XPC service
            .onReceive(NotificationCenter.default.publisher(for: XPCNotificationBridge.openBookmark)) { notification in
                guard let userInfo = notification.userInfo,
                      let urlString = userInfo["url"] as? String else { return }

                // Parse URL from string (XPC passes strings, not URL objects)
                let url: URL
                if let parsed = URL(string: urlString) {
                    url = parsed
                } else {
                    url = URL(fileURLWithPath: urlString)
                }

                // Check if it's a directory or .actr file
                var isDirectory: ObjCBool = false
                if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) {
                    if isDirectory.boolValue {
                        viewModel.loadFromDevDirectory(url)
                    } else {
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
            // Handle history item open from XPC service
            .onReceive(NotificationCenter.default.publisher(for: XPCNotificationBridge.openHistoryItem)) { notification in
                guard let userInfo = notification.userInfo,
                      let urlString = userInfo["url"] as? String else { return }

                let url: URL
                if let parsed = URL(string: urlString) {
                    url = parsed
                } else {
                    url = URL(fileURLWithPath: urlString)
                }

                var isDirectory: ObjCBool = false
                if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) {
                    if isDirectory.boolValue {
                        viewModel.loadFromDevDirectory(url)
                    } else {
                        viewModel.importACTRPackage(from: url)
                    }
                }
            }
            // Handle bookmark delete from XPC service
            .onReceive(NotificationCenter.default.publisher(for: XPCNotificationBridge.deleteBookmark)) { notification in
                guard let userInfo = notification.userInfo,
                      let bookmarkId = userInfo["bookmarkId"] as? String,
                      let uuid = UUID(uuidString: bookmarkId) else { return }
                bookmarkVM.removeBookmark(id: uuid)
            }
            // Handle history item delete from XPC service
            .onReceive(NotificationCenter.default.publisher(for: XPCNotificationBridge.deleteHistoryItem)) { notification in
                guard let userInfo = notification.userInfo,
                      let itemId = userInfo["itemId"] as? String,
                      let uuid = UUID(uuidString: itemId),
                      let item = historyVM.recentItems.first(where: { $0.id == uuid }) else { return }
                historyVM.deleteItem(item)
            }
            // Handle clear history from XPC service
            .onReceive(NotificationCenter.default.publisher(for: XPCNotificationBridge.clearHistory)) { _ in
                historyVM.clearHistory()
            }
            // Handle internal URI navigation (e.g., canvas://bookmarks/tags/{tagId})
            .onReceive(NotificationCenter.default.publisher(for: NSNotification.Name("com.canvas.openURI"))) { notification in
                guard let userInfo = notification.userInfo,
                      let uri = userInfo["uri"] as? String else { return }

                // Parse canvas:// URI scheme
                if uri.hasPrefix("canvas://bookmarks") {
                    // Open bookmarks tab (tag filtering will be handled by the tab itself later)
                    _ = viewModel.openSystemTab(.bookmarks)

                    // Log the tag info for future implementation
                    if let tagId = userInfo["tagId"] as? String,
                       let tagName = userInfo["tagName"] as? String {
                        NSLog("🏷️ Opening bookmarks with tag filter: \(tagName) (\(tagId))")
                    }
                }
            }
            // Handle URL open from Start Page (XPC service)
            // All URL types are handled by navigateCurrentTab: about:*, http/https, file paths
            .onReceive(NotificationCenter.default.publisher(for: XPCNotificationBridge.openURLFromStartPage)) { notification in
                guard let userInfo = notification.userInfo,
                      let urlString = userInfo["url"] as? String else { return }

                // 统一由 navigateCurrentTab 处理所有类型的 URL
                viewModel.navigateCurrentTab(to: urlString)
            }
    }

    private func handleLaunchURLIfNeeded() {
        guard !hasHandledLaunchURL,
              let launchURL = CanvasApp.launchURL else { return }
        hasHandledLaunchURL = true

        // Create a new tab, navigate to the URL, and enter focus mode (don't focus address bar)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            let tab = self.viewModel.addNewTab(focusAddressBar: false)
            self.viewModel.navigateCurrentTab(to: launchURL.absoluteString)

            // Auto enter focus mode
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                self.viewModel.canvasMode = .focus(tabId: tab.id)
            }
        }
    }

    /// Apply debug launch configuration
    private func applyDebugConfigIfNeeded() {
        guard !hasAppliedDebugConfig else { return }
        hasAppliedDebugConfig = true

        let config = CanvasApp.debugConfig
        guard config.hasDebugOptions else { return }

        NSLog("🔧 [Debug] Applying debug configuration...")

        // Apply window frame
        if let frame = config.windowFrame {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                if let window = NSApp.windows.first(where: { $0.isKeyWindow || $0.isMainWindow }) {
                    window.setFrame(frame, display: true, animate: false)
                    NSLog("🔧 [Debug] Window frame set to: \(frame)")
                }
            }
        }

        // Apply mode and tab
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            // Create specific tab type if requested
            if let tabType = config.tabType {
                switch tabType {
                case "startPage":
                    _ = self.viewModel.openSystemTab(.startPage)
                case "history":
                    _ = self.viewModel.openSystemTab(.history)
                case "bookmarks":
                    _ = self.viewModel.openSystemTab(.bookmarks)
                default:
                    NSLog("🔧 [Debug] Unknown tab type: \(tabType)")
                }
            }

            // Apply display mode
            if let mode = config.mode {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                    switch mode {
                    case "focus":
                        if let currentTab = self.viewModel.currentTab {
                            self.viewModel.canvasMode = .focus(tabId: currentTab.id)
                            NSLog("🔧 [Debug] Mode set to: focus (tab: \(currentTab.id.uuidString.prefix(8)))")
                        }
                    case "overview":
                        self.viewModel.canvasMode = .overview
                        NSLog("🔧 [Debug] Mode set to: overview")
                    default:
                        NSLog("🔧 [Debug] Unknown mode: \(mode)")
                    }
                }
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
            let systemTypeRaw = tabData["systemTabType"] as? String
            let systemTabType = systemTypeRaw.flatMap { SystemTabType(rawValue: $0) }

            let tab = Tab(
                title: title,
                url: url,
                htmlContent: tabData["htmlContent"] as? String,
                systemTabType: systemTabType,
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

// MARK: - Debug Launch Configuration

/// Configuration for debugging launch with specific window/mode settings
/// Usage: Canvas.app --window-frame=100,100,800,600 --mode=focus --tab=startPage
struct DebugLaunchConfig {
    /// Window frame (x, y, width, height)
    var windowFrame: NSRect?
    /// Display mode: "focus" or "overview"
    var mode: String?
    /// Initial tab type
    var tabType: String?
    /// Log file path for redirecting output
    var logFile: String?

    /// Parse command line arguments
    static func parse(_ args: [String]) -> DebugLaunchConfig {
        var config = DebugLaunchConfig()

        for arg in args {
            if arg.hasPrefix("--window-frame=") {
                let value = String(arg.dropFirst("--window-frame=".count))
                let parts = value.split(separator: ",").compactMap { Double($0) }
                if parts.count == 4 {
                    config.windowFrame = NSRect(x: parts[0], y: parts[1], width: parts[2], height: parts[3])
                    NSLog("🔧 [DebugConfig] Window frame: \(config.windowFrame!)")
                }
            } else if arg.hasPrefix("--mode=") {
                config.mode = String(arg.dropFirst("--mode=".count))
                NSLog("🔧 [DebugConfig] Mode: \(config.mode!)")
            } else if arg.hasPrefix("--tab=") {
                config.tabType = String(arg.dropFirst("--tab=".count))
                NSLog("🔧 [DebugConfig] Tab: \(config.tabType!)")
            } else if arg.hasPrefix("--log-file=") {
                config.logFile = String(arg.dropFirst("--log-file=".count))
                NSLog("🔧 [DebugConfig] Log file: \(config.logFile!)")
                // Setup log file redirection
                setupLogFile(config.logFile!)
            }
        }

        return config
    }

    /// Redirect stderr/stdout to a log file
    private static func setupLogFile(_ path: String) {
        // Clear existing log file
        try? FileManager.default.removeItem(atPath: path)
        FileManager.default.createFile(atPath: path, contents: nil)

        // Redirect stderr to file (Swift print and fputs go here)
        freopen(path, "a", stderr)
        // Also redirect stdout
        freopen(path, "a", stdout)

        NSLog("🔧 [DebugConfig] Logs redirected to: \(path)")
    }

    /// Check if any debug options are set
    var hasDebugOptions: Bool {
        windowFrame != nil || mode != nil || tabType != nil
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

    /// 从命令行参数解析的启动 URL
    static var launchURL: URL? = {
        // 支持 --url https://example.com 或 -url https://example.com
        let args = CommandLine.arguments
        NSLog("🚀 Canvas: Command line args: \(args)")
        if let urlIndex = args.firstIndex(where: { $0 == "--url" || $0 == "-url" }),
           urlIndex + 1 < args.count {
            let urlString = args[urlIndex + 1]
            if let url = URL(string: urlString) {
                NSLog("🚀 Canvas: Launch URL from command line: \(urlString)")
                return url
            }
        }
        return nil
    }()

    /// Debug launch configuration from command line arguments
    /// Supports:
    /// - `--window-frame=x,y,width,height` - Set window position and size
    /// - `--mode=focus|overview` - Start in focus or overview mode
    /// - `--tab=startPage|history|bookmarks` - Initial tab type
    /// - `--log-file=/path/to/log` - Redirect logs to file
    static let debugConfig: DebugLaunchConfig = DebugLaunchConfig.parse(CommandLine.arguments)

    init() {
        // Setup crash handler to log crashes
        CrashHandler.setup()

        // Disable window state restoration to prevent multiple windows on launch
        UserDefaults.standard.set(false, forKey: "NSQuitAlwaysKeepsWindows")

        // Suppress common system warnings in development
        UserDefaults.standard.set(false, forKey: "NSConstraintBasedLayoutLogUnsatisfiable")

        // 🔥 Prewarm XPC Services for faster tab creation
        // Only prewarm slot 0 (system apps) + slot 1 (app reserve)
        // Metro is no longer auto-started - apps load from local bundles
        let prewarmStart = Date()
        Task { @MainActor in
            #if DEBUG
            logger.notice("⚠️ 注意：控制台中关于 Metal shader、字体沙盒、WebKit 的警告是系统级警告，不影响应用运行")
            #endif

            // Verify service bundles exist
            _ = XPCServicePool.shared.verifyServiceBundles()
            // Prewarm XPC services for faster startup
            // Service 端会发送 xpcConnectionCreated 通知，Host 捕获 internalId
            // 后续消费 prewarm VC 时通过 bindTab 通知绑定 tabId
            await XPCServicePool.shared.prewarmInitialServicesAsync()

            let elapsed = Date().timeIntervalSince(prewarmStart) * 1000
            logger.info("✅ System startup complete - prewarm took \(Int(elapsed))ms")
        }

        // Initialize TabProcessManager early
        _ = TabProcessManager.shared

        // Initialize XPC notification bridge (listens for notifications from XPC services)
        _ = XPCNotificationBridge.shared
    }

    var body: some Scene {
        // Main window group - each window gets its own BrowserViewModel
        WindowGroup(id: "browser") {
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
                // For Web tabs: use WebKit Inspector; for actr tabs: use built-in DevTools
                Button("Toggle DevTools") {
                    toggleDevToolsForCurrentTab()
                }
                .keyboardShortcut("i", modifiers: [.command, .option])

                Divider()

                // actr DevTools section
                Button("Toggle ACTR DevTools") {
                    devTools.toggle()
                }

                Button("Open ACTR DevTools in New Window") {
                    devTools.detach()
                    openWindow(id: "devtools")
                }
                .keyboardShortcut("i", modifiers: [.command, .option, .shift])

                Button("Dock ACTR DevTools") {
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

                Divider()

                Button("聚焦地址栏") {
                    // Only works in Focus mode (when address bar is visible)
                    if case .focus = focusedViewModel?.canvasMode {
                        focusedViewModel?.shouldAutoFocusAddressBar = true
                    }
                }
                .keyboardShortcut("l", modifiers: .command)
                .disabled(focusedViewModel == nil)

                Divider()

                Button("打印...") {
                    printCurrentPage()
                }
                .keyboardShortcut("p", modifiers: .command)
                .disabled(focusedViewModel == nil)
            }

            // View menu - Zoom controls
            CommandGroup(after: .toolbar) {
                Button("放大") {
                    zoomIn()
                }
                .keyboardShortcut("+", modifiers: .command)
                .disabled(focusedViewModel == nil)

                Button("缩小") {
                    zoomOut()
                }
                .keyboardShortcut("-", modifiers: .command)
                .disabled(focusedViewModel == nil)

                Button("实际大小") {
                    resetZoom()
                }
                .keyboardShortcut("0", modifiers: .command)
                .disabled(focusedViewModel == nil)

                Divider()

                Button("进入全屏") {
                    toggleFullScreen()
                }
                .keyboardShortcut("f", modifiers: [.command, .control])
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
            // 统一由 navigateCurrentTab 处理
            vm.navigateCurrentTab(to: url.path)
            bookmarkVM.markAsUsed(bookmark)
            // Also record in history
            if let tab = vm.currentTab {
                historyVM.recordVisit(tab: tab)
            }
        }
    }

    // MARK: - DevTools

    /// Toggle DevTools based on current tab type:
    /// - Web tab: Open WebKit Web Inspector
    /// - ACTR tab: Toggle built-in DevTools panel
    private func toggleDevToolsForCurrentTab() {
        guard let vm = focusedViewModel,
              let tab = vm.currentTab else {
            // No tab, fallback to actr DevTools
            devTools.toggle()
            return
        }

        switch tab.contentType {
        case .web:
            // Web tab: open WebKit Inspector
            if let holder = WebViewHolderManager.shared.existingHolder(for: tab.id) {
                holder.showInspector()
            }
        case .reactNative, .empty:
            // ACTR/RN tab or empty: use built-in DevTools
            devTools.toggle()
        }
    }

    // MARK: - Print

    private func printCurrentPage() {
        guard let vm = focusedViewModel,
              let tab = vm.currentTab,
              case .web = tab.contentType,
              let holder = WebViewHolderManager.shared.existingHolder(for: tab.id) else {
            return
        }
        holder.print()
    }

    // MARK: - Zoom

    private func zoomIn() {
        guard let vm = focusedViewModel,
              let tab = vm.currentTab,
              case .web = tab.contentType,
              let holder = WebViewHolderManager.shared.existingHolder(for: tab.id) else {
            return
        }
        holder.zoomIn()
    }

    private func zoomOut() {
        guard let vm = focusedViewModel,
              let tab = vm.currentTab,
              case .web = tab.contentType,
              let holder = WebViewHolderManager.shared.existingHolder(for: tab.id) else {
            return
        }
        holder.zoomOut()
    }

    private func resetZoom() {
        guard let vm = focusedViewModel,
              let tab = vm.currentTab,
              case .web = tab.contentType,
              let holder = WebViewHolderManager.shared.existingHolder(for: tab.id) else {
            return
        }
        holder.resetZoom()
    }

    // MARK: - Fullscreen

    private func toggleFullScreen() {
        guard let window = NSApp.keyWindow else { return }
        window.toggleFullScreen(nil)
    }
}

