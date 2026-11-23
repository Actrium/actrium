import AppKit

/// The visible window tracks selection and cursor state.
/// The renderer supplies the selected text when the user invokes Copy.
@MainActor
final class RendererDocumentInteraction {
    private weak var surface: RendererSurfaceHostView?
    private var observers: [NSObjectProtocol] = []
    private var outsidePressMonitor: Any?
    private(set) var hasSelection = false
    private(set) var usesTextCursor = false

    init(surface: RendererSurfaceHostView) {
        self.surface = surface
        let center = NotificationCenter.default
        let names: [Notification.Name] = [.rendererDocumentSelection, .rendererTextCursor]
        observers = names.map { name in
            center.addObserver(forName: name, object: nil, queue: .main) { [weak self] note in
                guard let tabId = note.userInfo?["tabId"] as? UUID,
                      let active = note.userInfo?["active"] as? Bool else { return }
                MainActor.assumeIsolated {
                    self?.update(name, tabId: tabId, active: active)
                }
            }
        }
    }

    isolated deinit {
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        removeOutsidePressMonitor()
    }

    private func update(_ name: Notification.Name, tabId: UUID, active: Bool) {
        guard let surface, surface.tabId == tabId else { return }
        if name == .rendererDocumentSelection {
            hasSelection = active
            updateOutsidePressMonitor()
        } else {
            usesTextCursor = active
            guard surface.allowsInput, let window = surface.window, window.isKeyWindow,
                  contains(window.mouseLocationOutsideOfEventStream, in: window) else { return }
            (active ? NSCursor.iBeam : NSCursor.arrow).set()
        }
    }

    private func contains(_ point: NSPoint, in window: NSWindow) -> Bool {
        guard let content = window.contentView, let surface,
              let hit = content.hitTest(content.superview?.convert(point, from: nil) ?? point) else { return false }
        return hit === surface || hit.isDescendant(of: surface)
    }

    func updateOutsidePressMonitor() {
        guard hasSelection, let surface, surface.allowsInput, surface.window != nil else {
            removeOutsidePressMonitor()
            return
        }
        guard outsidePressMonitor == nil else { return }
        // Some window controls handle clicks without becoming first responder.
        outsidePressMonitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            if let self, let window = self.surface?.window, event.window === window,
               !self.contains(event.locationInWindow, in: window) {
                self.surface?.clearDocumentSelection()
            }
            return event
        }
    }

    func removeOutsidePressMonitor() {
        guard let outsidePressMonitor else { return }
        NSEvent.removeMonitor(outsidePressMonitor)
        self.outsidePressMonitor = nil
    }

    func reset() {
        usesTextCursor = false
        clearSelection()
    }

    func clearSelection() {
        hasSelection = false
        removeOutsidePressMonitor()
    }

    func mouseExited() { usesTextCursor = false }
}
