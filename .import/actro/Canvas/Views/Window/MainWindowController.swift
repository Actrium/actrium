import AppKit
import SwiftUI
import Combine

/// Main window controller - manages window chrome with self-drawn toolbar
final class MainWindowController: NSWindowController, NSWindowDelegate {

    // MARK: - Properties

    let viewModel: BrowserViewModel  // Public for AppDelegate access
    private let historyVM: HistoryViewModel
    private let bookmarkVM: BookmarkViewModel

    private var rootViewController: RootContentViewController!

    // MARK: - Initialization

    init(viewModel: BrowserViewModel, historyVM: HistoryViewModel, bookmarkVM: BookmarkViewModel) {
        self.viewModel = viewModel
        self.historyVM = historyVM
        self.bookmarkVM = bookmarkVM

        // Create window with transparent titlebar
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )

        super.init(window: window)

        configureWindow(window)
        setupContentViewController()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Layout Constants

    /// Extra Y offset to move traffic lights down from default position
    private let trafficLightYOffset: CGFloat = 11

    /// Extra X offset to move traffic lights right from default position
    private let trafficLightXOffset: CGFloat = 9

    // MARK: - Window Configuration

    private func configureWindow(_ window: NSWindow) {
        window.delegate = self

        // Transparent titlebar - traffic lights float over content
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.backgroundColor = .clear

        // No system toolbar - we draw our own
        window.toolbar = nil

        // Minimum size
        window.minSize = NSSize(width: 640, height: 480)

        // Center on screen
        window.center()

        // Adjust traffic light positions after a brief delay to ensure layout is complete
        DispatchQueue.main.async { [weak self] in
            self?.adjustTrafficLights()
        }
    }

    // MARK: - Traffic Light Positioning

    private func adjustTrafficLights() {
        guard let window = window else { return }

        let buttonTypes: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]

        for buttonType in buttonTypes {
            guard let button = window.standardWindowButton(buttonType) else { continue }

            // Move button down and right by offsets
            var frame = button.frame
            frame.origin.x += trafficLightXOffset
            frame.origin.y -= trafficLightYOffset
            button.setFrameOrigin(frame.origin)
        }
    }

    // MARK: - Content Setup

    private func setupContentViewController() {
        guard let window = window else { return }

        // Create root content view controller with 4px edge inset
        rootViewController = RootContentViewController(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM
        )

        // Set as window's content view controller
        window.contentViewController = rootViewController
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

    func windowDidResize(_ notification: Notification) {
        adjustTrafficLights()
    }
}
