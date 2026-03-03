import AppKit
import SwiftUI
import Combine

/// Application delegate - manages AppKit windows
final class AppDelegate: NSObject, NSApplicationDelegate {

    // MARK: - Properties

    /// Shared view models (global across all windows)
    let historyVM = HistoryViewModel()
    let bookmarkVM = BookmarkViewModel()

    /// Active window controllers
    private var windowControllers: [MainWindowController] = []

    /// Currently focused browser view model
    var focusedViewModel: BrowserViewModel? {
        return windowControllers.first { $0.window?.isKeyWindow == true }?.viewModel
    }

    private var cancellables = Set<AnyCancellable>()

    // MARK: - NSApplicationDelegate

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Disable window state restoration
        UserDefaults.standard.set(false, forKey: "NSQuitAlwaysKeepsWindows")

        // Suppress common system warnings in development
        UserDefaults.standard.set(false, forKey: "NSConstraintBasedLayoutLogUnsatisfiable")

        // Initialize services
        initializeServices()

        // Create initial window
        createNewWindow()

        // Setup notification listeners
        setupNotifications()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return false  // Keep app running even when all windows are closed
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

        // Prewarm XPC services
        Task { @MainActor in
            _ = XPCServicePool.shared.verifyServiceBundles()
            await XPCServicePool.shared.prewarmInitialServicesAsync()
        }
    }

    private func setupNotifications() {
        // Listen for new window requests
        NotificationCenter.default.publisher(for: .openNewWindowWithGroup)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.createNewWindow()
            }
            .store(in: &cancellables)
    }

    // MARK: - Window Management

    @objc func createNewWindow() {
        let viewModel = BrowserViewModel()
        viewModel.historyVM = historyVM

        let windowController = MainWindowController(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM
        )

        windowControllers.append(windowController)
        windowController.showWindow()

        // Register with DevTools
        DevToolsManager.shared.activeBrowserViewModel = viewModel
    }

    func removeWindowController(_ controller: MainWindowController) {
        windowControllers.removeAll { $0 === controller }
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
