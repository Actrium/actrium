import AppKit
import SwiftUI
import Combine
import ActriumIPC

/// Application delegate - manages AppKit windows
final class AppDelegate: NSObject, NSApplicationDelegate, ObservableObject {

    // MARK: - Properties

    /// Shared view models (global across all windows)
    let historyVM = HistoryViewModel()
    let bookmarkVM = BookmarkViewModel()

    /// Active window controllers
    private var windowControllers: [MainWindowController] = []

    /// Currently focused browser view model
    @Published var focusedViewModel: BrowserViewModel?

    private var cancellables = Set<AnyCancellable>()
    private var pendingCapabilityPrompts: Set<String> = []

    // MARK: - NSApplicationDelegate

    func applicationDidFinishLaunching(_ notification: Notification) {
        AppearanceManager.shared.apply()
        // Disable window state restoration
        UserDefaults.standard.set(false, forKey: "NSQuitAlwaysKeepsWindows")

        // Suppress common system warnings in development
        UserDefaults.standard.set(false, forKey: "NSConstraintBasedLayoutLogUnsatisfiable")

        // Initialize services
        initializeServices()

        // Setup notification listeners before creating windows
        setupNotifications()

        // Create initial window
        createNewWindow()

        // Ensure focusedViewModel is set after the run loop settles
        DispatchQueue.main.async { [weak self] in
            self?.refreshFocusedViewModel()
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return false  // Keep app running even when all windows are closed
    }

    func applicationWillTerminate(_ notification: Notification) {
        RendererProcessPool.shared.shutdown()
        VaultManager.shared.lock()
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag {
            createNewWindow()
        }
        return true
    }

    // MARK: - Initialization

    private func initializeServices() {
        // Setup crash handler
        CrashHandler.setup()

        // Initialize XPC notification bridge
        _ = XPCNotificationBridge.shared

        // Initialize TabProcessManager
        _ = TabProcessManager.shared

        // Prewarm the browser-style homogeneous Renderer pool.
        RendererProcessPool.shared.documentWindow = { [weak self] tabId, reloadKey in
            self?.documentController(tabId: tabId, reloadKey: reloadKey)?.window
        }
        RendererProcessPool.shared.fileImportWindow = { [weak self] tabId, reloadKey in
            self?.guestFileWindow(
                tabId: tabId, reloadKey: reloadKey, capability: GuestFileImportCall.capability,
                isDeclared: RendererFileImport.isDeclared
            )
        }
        RendererProcessPool.shared.fileExportWindow = { [weak self] tabId, reloadKey in
            self?.guestFileWindow(
                tabId: tabId, reloadKey: reloadKey, capability: GuestFileExportCall.capability,
                isDeclared: RendererFileExport.isDeclared
            )
        }
        Task { @MainActor in
            _ = RendererProcessPool.shared.verifyRendererHelper()
            await RendererProcessPool.shared.prewarmInitialRenderersAsync()
        }

        // Initialize vault manager
        _ = VaultManager.shared

        // Pre-compile ad-blocking rules so they're ready when tabs open
        ContentBlockerManager.shared.loadRules { _ in }

        // Restore the local-folder browser sync provider selection
        SyncManager.shared.restoreOnLaunch()

        Task.detached(priority: .utility) {
            ActrPackageDownloadService.cleanupOrphanedTemporaryDownloads()
        }
    }

    private func documentController(tabId: UUID, reloadKey: Int) -> MainWindowController? {
        windowControllers.first { controller in
            controller.viewModel.tabs.contains { $0.id == tabId && $0.reloadNonce == reloadKey }
        }
    }

    private func guestFileWindow(
        tabId: UUID, reloadKey: Int, capability: String, isDeclared: @MainActor (String?, [String]) -> Bool
    ) -> NSWindow? {
        guard let controller = documentController(tabId: tabId, reloadKey: reloadKey),
              controller.viewModel.currentTabId == tabId,
              controller.viewModel.actriumMode == .focus(tabId: tabId),
              let tab = controller.viewModel.currentTab,
              isDeclared(tab.capabilityManifestJSON, tab.permissions),
              CapabilityPermissionStore.shared.hasGrant(
                tabId: tabId, appId: tab.appId, capability: capability
              ) else { return nil }
        return controller.window
    }

    private func setupNotifications() {
        // Listen for new window requests
        NotificationCenter.default.publisher(for: .openNewWindowWithGroup)
            .receive(on: RunLoop.main)
            .sink { [weak self] notification in
                guard let transfer = notification.object as? TabGroupWindowTransfer else { return }
                self?.createWindow(groupTransfer: transfer)
            }
            .store(in: &cancellables)

        // Track key window changes so SwiftUI menus re-evaluate
        for name in [NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification] {
            NotificationCenter.default.publisher(for: name)
                .receive(on: RunLoop.main)
                .sink { [weak self] _ in
                    self?.refreshFocusedViewModel()
                }
                .store(in: &cancellables)
        }

        NotificationCenter.default.publisher(
            for: XPCNotificationBridge.capabilityPermissionRequest
        )
        .receive(on: RunLoop.main)
        .sink { [weak self] notification in
            self?.handleCapabilityPermissionRequest(notification)
        }
        .store(in: &cancellables)
    }

    private func handleCapabilityPermissionRequest(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabIdString = userInfo["tabId"] as? String,
              let tabId = UUID(uuidString: tabIdString),
              let capability = userInfo["capability"] as? String else {
            return
        }

        guard let controller = windowControllers.first(where: {
            $0.viewModel.tabs.contains(where: { $0.id == tabId })
        }), let tab = controller.viewModel.tabs.first(where: { $0.id == tabId }) else {
            return
        }
        let declared = tab.permissions.contains {
            capability == $0 || capability.hasPrefix($0 + ".")
        }
        guard declared else {
            NSLog("Capability permission request rejected as undeclared: %@", capability)
            return
        }

        let promptKey = "\(tabId.uuidString):\(capability)"
        guard !pendingCapabilityPrompts.contains(promptKey),
              let window = controller.window else {
            return
        }
        pendingCapabilityPrompts.insert(promptKey)

        let alert = NSAlert()
        alert.alertStyle = .informational
        // The requester must be the shell's verified application identity, not
        // the tab title: a guest can retitle its tab at runtime and must not
        // be able to spoof the name in a permission prompt.
        let requester = tab.verifiedAppName ?? "this application"
        alert.messageText = "Allow \(requester) to use this capability?"
        alert.informativeText = "\(capability)\n\nThe application requested access while handling a user action."
        alert.addButton(withTitle: "Allow for This Session")
        alert.addButton(withTitle: "Always Allow")
        alert.addButton(withTitle: "Deny")

        alert.beginSheetModal(for: window) { [weak self, weak controller] response in
            guard let self else { return }
            self.pendingCapabilityPrompts.remove(promptKey)
            let choice: CapabilityPermissionChoice
            switch response {
            case .alertFirstButtonReturn:
                choice = .allowForSession
            case .alertSecondButtonReturn:
                choice = .alwaysAllow
            case .alertThirdButtonReturn:
                choice = .deny
            default:
                // A cancelled sheet is not a user denial.
                return
            }
            controller?.viewModel.resolveCapabilityPermission(
                tabId: tabId,
                capability: capability,
                choice: choice
            )
        }
    }

    private func refreshFocusedViewModel() {
        focusedViewModel = windowControllers.first { $0.window?.isKeyWindow == true }?.viewModel
    }

    // MARK: - Window Management

    @MainActor
    func protectedPackageDigests() -> Set<String> {
        var digests = Set<String>()
        for controller in windowControllers {
            for tab in controller.viewModel.tabs {
                guard let path = tab.guestExtractedPath,
                      let digest = ActrPackageCache.digest(fromExtractedPath: path)
                else { continue }
                digests.insert(digest)
            }
        }
        return digests
    }

    @objc func createNewWindow() {
        createWindow()
    }

    private func createWindow(groupTransfer: TabGroupWindowTransfer? = nil) {
        let viewModel = BrowserViewModel()
        viewModel.historyVM = historyVM

        let windowController = MainWindowController(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM
        )

        if let groupTransfer, !groupTransfer.move(to: viewModel) { return }

        windowControllers.append(windowController)
        windowController.showWindow()
        refreshFocusedViewModel()

        // Restart restore (15-B B7): rebuild the persisted session into the
        // first window; actr:// instance addresses rebind with the same ActorId.
        if groupTransfer == nil { viewModel.restoreSessionIfNeeded() }

        // Register with DevTools
        DevToolsManager.shared.activeBrowserViewModel = viewModel
    }

    func removeWindowController(_ controller: MainWindowController) {
        windowControllers.removeAll { $0 === controller }
        refreshFocusedViewModel()
    }

    func toggleNativeFullScreenForKeyWindow() {
        if let controller = windowControllers.first(where: { $0.window?.isKeyWindow == true }) {
            controller.toggleNativeFullScreen()
            return
        }

        if let keyWindow = NSApp.keyWindow,
           let controller = windowControllers.first(where: { $0.window === keyWindow }) {
            controller.toggleNativeFullScreen()
            return
        }

        NSApp.keyWindow?.toggleFullScreen(nil)
    }
}
