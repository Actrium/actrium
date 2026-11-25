import AppKit

enum RendererCursor: String, Decodable, Sendable {
    case arrow = "default"
    case text
    case pointer

    @MainActor
    var appKitCursor: NSCursor {
        switch self {
        case .arrow: .arrow
        case .text: .iBeam
        case .pointer: .pointingHand
        }
    }
}

/// The visible window tracks selection and cursor state.
/// The renderer supplies the selected text when the user invokes Copy.
@MainActor
final class RendererDocumentInteraction {
    private weak var surface: RendererSurfaceHostView?
    private var observers: [NSObjectProtocol] = []
    private var outsidePressMonitor: Any?
    private(set) var hasSelection = false
    private(set) var cursor: RendererCursor = .arrow

    init(surface: RendererSurfaceHostView) {
        self.surface = surface
        let center = NotificationCenter.default
        observers = [
            center.addObserver(forName: .rendererDocumentSelection, object: nil, queue: .main) { [weak self] note in
                guard let tabId = note.userInfo?["tabId"] as? UUID,
                      let active = note.userInfo?["active"] as? Bool else { return }
                MainActor.assumeIsolated {
                    self?.updateSelection(tabId: tabId, active: active)
                }
            },
            center.addObserver(forName: .rendererCursor, object: nil, queue: .main) { [weak self] note in
                guard let tabId = note.userInfo?["tabId"] as? UUID,
                      let cursor = note.userInfo?["cursor"] as? RendererCursor else { return }
                MainActor.assumeIsolated {
                    self?.updateCursor(tabId: tabId, cursor: cursor)
                }
            },
        ]
    }

    isolated deinit {
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        removeOutsidePressMonitor()
    }

    private func updateSelection(tabId: UUID, active: Bool) {
        guard surface?.tabId == tabId else { return }
        hasSelection = active
        updateOutsidePressMonitor()
    }

    private func updateCursor(tabId: UUID, cursor: RendererCursor) {
        guard let surface, surface.tabId == tabId else { return }
        self.cursor = cursor
        guard surface.allowsInput, let window = surface.window, window.isKeyWindow,
              contains(window.mouseLocationOutsideOfEventStream, in: window) else { return }
        cursor.appKitCursor.set()
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
        cursor = .arrow
        clearSelection()
    }

    func clearSelection() {
        hasSelection = false
        removeOutsidePressMonitor()
    }

    func mouseExited() { cursor = .arrow }
}
