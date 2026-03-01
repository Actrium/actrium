import AppKit
import SwiftUI
import Combine

/// Main window controller - manages window chrome, 4px edge inset, and toolbar
final class MainWindowController: NSWindowController, NSWindowDelegate {

    // MARK: - Properties

    let viewModel: BrowserViewModel  // Public for AppDelegate access
    private let historyVM: HistoryViewModel
    private let bookmarkVM: BookmarkViewModel

    private var splitViewController: MainSplitViewController!
    private var cancellables = Set<AnyCancellable>()

    /// Window edge inset (transparent border)
    private let edgeInset: CGFloat = 4

    // MARK: - Initialization

    init(viewModel: BrowserViewModel, historyVM: HistoryViewModel, bookmarkVM: BookmarkViewModel) {
        self.viewModel = viewModel
        self.historyVM = historyVM
        self.bookmarkVM = bookmarkVM

        // Create window
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )

        super.init(window: window)

        configureWindow(window)
        setupContentViewController()
        setupToolbar()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Window Configuration

    private func configureWindow(_ window: NSWindow) {
        window.delegate = self

        // Transparent titlebar for custom appearance
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.backgroundColor = .clear

        // Minimum size
        window.minSize = NSSize(width: 640, height: 428)

        // Center on screen
        window.center()
    }

    // MARK: - Content Setup

    private func setupContentViewController() {
        guard let window = window else { return }

        // Create the split view controller
        splitViewController = MainSplitViewController(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM,
            edgeInset: edgeInset
        )

        // Set as window's content view controller
        window.contentViewController = splitViewController
    }

    // MARK: - Toolbar Setup

    private func setupToolbar() {
        guard let window = window else { return }

        // Create toolbar
        let toolbar = NSToolbar(identifier: "MainToolbar")
        toolbar.displayMode = .iconOnly
        toolbar.delegate = self
        toolbar.showsBaselineSeparator = false

        window.toolbar = toolbar
        window.toolbarStyle = .unified
    }

    // MARK: - Public Methods

    func showWindow() {
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    // MARK: - NSWindowDelegate

    func windowWillClose(_ notification: Notification) {
        // Cleanup if needed
    }

    func windowDidBecomeKey(_ notification: Notification) {
        // Update DevTools reference
        DevToolsManager.shared.activeBrowserViewModel = viewModel
    }
}

// MARK: - NSToolbarDelegate

extension MainWindowController: NSToolbarDelegate {

    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier, willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        // Toolbar items will be handled by ToolbarViewController
        return nil
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        return []
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        return []
    }
}
