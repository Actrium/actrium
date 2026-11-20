import AppKit

@MainActor
private final class Recorder: NSGestureRecognizer {
    var received: [NSEvent.EventType] = []
    var cancellations = 0
    override func mouseDown(with event: NSEvent) { received.append(event.type) }
    override func mouseDragged(with event: NSEvent) { received.append(event.type) }
    override func mouseUp(with event: NSEvent) { received.append(event.type) }
    override func rightMouseDown(with event: NSEvent) { received.append(event.type) }
    override func rightMouseDragged(with event: NSEvent) { received.append(event.type) }
    override func reset() { cancellations += 1 }
}

@main
struct RendererMouseTrackingRegression {
    @MainActor
    static func main() {
        func event(_ type: NSEvent.EventType) -> NSEvent {
            NSEvent.mouseEvent(with: type, location: .zero, modifierFlags: [], timestamp: 0,
                               windowNumber: 0, context: nil, eventNumber: 1, clickCount: 1, pressure: 1)!
        }
        let tracking = RendererMouseTracking()
        let text = Recorder()
        let other = Recorder()
        tracking.begin(event(.leftMouseDown), button: 0, recognizers: [text])
        tracking.begin(event(.rightMouseDown), button: 1, recognizers: [other])
        precondition(tracking.drag(event(.leftMouseDragged), button: 0))
        precondition(tracking.drag(event(.rightMouseDragged), button: 1))
        tracking.end(event(.leftMouseUp), button: 0)
        precondition(text.received == [.leftMouseDown, .leftMouseDragged, .leftMouseUp])
        precondition(other.received == [.rightMouseDown, .rightMouseDragged])
        precondition(!tracking.drag(event(.leftMouseDragged), button: 0))
        tracking.cancel()
        precondition(other.cancellations == 1 && text.cancellations == 0)
        precondition(!tracking.drag(event(.rightMouseDragged), button: 1))

        let replacement = Recorder()
        tracking.begin(event(.leftMouseDown), button: 0, recognizers: [text])
        tracking.begin(event(.rightMouseDown), button: 1, recognizers: [other])
        tracking.begin(event(.leftMouseDown), button: 0, recognizers: [replacement])
        precondition(text.cancellations == 1, "The previous press must reset before it is replaced")
        precondition(other.cancellations == 1, "A different button must keep its press")
        precondition(tracking.drag(event(.leftMouseDragged), button: 0))
        tracking.end(event(.leftMouseUp), button: 0)
        precondition(replacement.received == [.leftMouseDown, .leftMouseDragged, .leftMouseUp])
        tracking.cancel(button: 1)
        precondition(other.cancellations == 2)
        precondition(!tracking.drag(event(.rightMouseDragged), button: 1))
    }
}
