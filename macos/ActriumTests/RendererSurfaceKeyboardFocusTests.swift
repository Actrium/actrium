import AppKit
import Testing
@testable import Actrium

struct RendererSurfaceKeyboardFocusTests {
    @Test @MainActor func copyRequiresSelectionInTheFocusedSurface() {
        let window = NSWindow(contentRect: .zero, styleMask: .titled, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let surface = RendererSurfaceHostView(frame: .zero)
        let address = NSTextField()
        window.contentView?.addSubview(surface)
        window.contentView?.addSubview(address)
        let tab = UUID()
        surface.tabId = tab
        #expect(window.makeFirstResponder(surface))
        let copy = NSMenuItem(title: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        func select(_ id: UUID) {
            NotificationCenter.default.post(name: .rendererDocumentSelection, object: nil,
                                             userInfo: ["tabId": id, "active": true])
        }
        select(UUID())
        #expect(!surface.validateMenuItem(copy))
        select(tab)
        #expect(surface.validateMenuItem(copy))
        #expect(window.makeFirstResponder(address))
        #expect(!surface.validateMenuItem(copy))
    }

    @Test @MainActor func restoresOnlyTheActiveTabWithoutTakingAddressBarFocus() async {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 300),
            styleMask: .titled, backing: .buffered, defer: false
        )
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let first = RendererSurfaceHostView(frame: NSRect(x: 0, y: 0, width: 400, height: 220))
        let second = RendererSurfaceHostView(frame: first.frame)
        let address = NSTextField(frame: NSRect(x: 20, y: 240, width: 360, height: 28))
        window.contentView?.addSubview(first)
        window.contentView?.addSubview(second)
        window.contentView?.addSubview(address)
        second.allowsInput = false

        #expect(window.makeFirstResponder(first))
        second.allowsInput = true
        first.allowsInput = false
        #expect(window.firstResponder !== first)
        #expect(window.makeFirstResponder(second))

        first.allowsInput = true
        second.allowsInput = false
        await drainMainQueue()
        #expect(window.firstResponder === first)

        // The second Tab becomes inactive again before its restore can run.
        second.allowsInput = true
        first.allowsInput = false
        first.allowsInput = true
        second.allowsInput = false
        await drainMainQueue()
        #expect(window.firstResponder === first)

        first.allowsInput = false
        #expect(window.makeFirstResponder(address))
        let addressEditor = window.firstResponder
        first.allowsInput = true
        await drainMainQueue()
        #expect(window.firstResponder === addressEditor)
    }

    private func drainMainQueue() async {
        await withCheckedContinuation { continuation in
            DispatchQueue.main.async { continuation.resume() }
        }
    }
}
