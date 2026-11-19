import AppKit

/// The offscreen window does not dispatch synthetic events to gesture recognizers.
/// Keep the original hit path for the lifetime of each physical press.
@MainActor
final class RendererMouseTracking {
    private var pressed: [UInt32: [NSGestureRecognizer]] = [:]

    func begin(_ event: NSEvent, button: UInt32, recognizers: [NSGestureRecognizer]) {
        pressed[button] = recognizers
        forward(event, to: recognizers)
    }

    func drag(_ event: NSEvent, button: UInt32) -> Bool {
        guard let recognizers = pressed[button] else { return false }
        forward(event, to: recognizers)
        return true
    }

    func end(_ event: NSEvent, button: UInt32) {
        guard let recognizers = pressed.removeValue(forKey: button) else { return }
        forward(event, to: recognizers)
    }

    func cancel() {
        let recognizers = Set(pressed.values.joined())
        pressed.removeAll()
        for recognizer in recognizers { recognizer.reset() }
    }

    private func forward(_ event: NSEvent, to recognizers: [NSGestureRecognizer]) {
        for recognizer in recognizers {
            switch event.type {
            case .leftMouseDown: recognizer.mouseDown(with: event)
            case .rightMouseDown: recognizer.rightMouseDown(with: event)
            case .otherMouseDown: recognizer.otherMouseDown(with: event)
            case .leftMouseDragged: recognizer.mouseDragged(with: event)
            case .rightMouseDragged: recognizer.rightMouseDragged(with: event)
            case .otherMouseDragged: recognizer.otherMouseDragged(with: event)
            case .leftMouseUp: recognizer.mouseUp(with: event)
            case .rightMouseUp: recognizer.rightMouseUp(with: event)
            case .otherMouseUp: recognizer.otherMouseUp(with: event)
            default: break
            }
        }
    }
}
