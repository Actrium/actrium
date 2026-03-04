import SwiftUI
import AppKit

// MARK: - Settings Window Controller

/// Manages the Settings window lifecycle (singleton)
final class SettingsWindowController: NSObject, NSWindowDelegate {
    static let shared = SettingsWindowController()

    private var window: NSWindow?
    private var hostingView: NSHostingView<SettingsView>?

    private override init() {
        super.init()
    }

    func showWindow() {
        if let existingWindow = window, existingWindow.isVisible {
            existingWindow.makeKeyAndOrderFront(nil)
            return
        }

        let contentView = SettingsView()
        let hostingView = NSHostingView(rootView: contentView)
        self.hostingView = hostingView

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 550, height: 420),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )

        window.title = "Settings"
        window.contentView = hostingView
        window.center()
        window.setFrameAutosaveName("SettingsWindow")
        window.isReleasedWhenClosed = false
        window.delegate = self

        // Fixed size
        window.contentMinSize = NSSize(width: 550, height: 420)
        window.contentMaxSize = NSSize(width: 550, height: 420)

        self.window = window
        window.makeKeyAndOrderFront(nil)
    }

    func closeWindow() {
        window?.close()
    }

    // MARK: - NSWindowDelegate

    func windowWillClose(_ notification: Notification) {
        window = nil
        hostingView = nil
    }
}
