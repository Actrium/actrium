import AppKit

func extendMultiClickSelection(from start: NSRange, to target: NSRange, at index: Int) -> (range: NSRange, anchor: Int) {
    (NSUnionRange(start, target), index < start.location ? start.upperBound : start.location)
}

/// AppKit cannot track the real pointer over an offscreen window. Use the
/// renderer's hit path and dispatch transitions to the interested Fabric views.
@MainActor
final class RendererHoverTracking {
    private final class Target {
        weak var view: NSView?
        init(_ view: NSView) { self.view = view }
    }

    private var targets: [Target] = []

    func update(hitView: NSView?, windowNumber: Int, location: NSPoint) {
        let previous = targets.compactMap(\.view)
        let next = sequence(first: hitView, next: { $0?.superview })
            .prefix(while: { $0 != nil }).compactMap { $0 }
            .filter(ActriumRendererTracksHover)
        let oldIDs = Set(previous.map(ObjectIdentifier.init))
        let newIDs = Set(next.map(ObjectIdentifier.init))
        guard oldIDs != newIDs else { return }
        targets = next.map(Target.init)

        send(.mouseExited, to: previous.filter { !newIDs.contains(ObjectIdentifier($0)) },
             windowNumber: windowNumber, location: location)
        send(.mouseEntered, to: next.reversed().filter { !oldIDs.contains(ObjectIdentifier($0)) },
             windowNumber: windowNumber, location: location)
    }

    private func send(_ type: NSEvent.EventType, to views: [NSView], windowNumber: Int, location: NSPoint) {
        guard !views.isEmpty, let event = NSEvent.enterExitEvent(
            with: type, location: location, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: windowNumber, context: nil, eventNumber: 0, trackingNumber: 0, userData: nil
        ) else { return }
        for view in views {
            if type == .mouseEntered { view.mouseEntered(with: event) }
            else { view.mouseExited(with: event) }
        }
    }
}

/// The offscreen window does not dispatch synthetic events to gesture recognizers.
/// Keep the original hit path for the lifetime of each physical press.
@MainActor
final class RendererMouseTracking {
    private var pressed: [UInt32: [NSGestureRecognizer]] = [:]

    func begin(_ event: NSEvent, button: UInt32, recognizers: [NSGestureRecognizer]) {
        cancel(button: button)
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

    func cancel(button: UInt32) {
        guard let recognizers = pressed.removeValue(forKey: button) else { return }
        for recognizer in recognizers { recognizer.reset() }
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
