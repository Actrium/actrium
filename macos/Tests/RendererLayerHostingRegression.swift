import AppKit
import QuartzCore

private final class ContentView: NSView {
    override var acceptsFirstResponder: Bool { true }
    override var isFlipped: Bool { true }
}

@main
struct RendererLayerHostingRegression {
    @MainActor
    static func main() {
        let app = NSApplication.shared
        app.setActivationPolicy(.prohibited)
        app.finishLaunching()
        let window = RendererWindow(
            contentRect: NSRect(x: -100_000, y: -100_000, width: 800, height: 600),
            styleMask: .borderless, backing: .buffered, defer: false
        )
        window.isReleasedWhenClosed = false
        window.isOpaque = false
        window.backgroundColor = .clear
        let content = ContentView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        window.contentView = content
        window.orderOut(nil)
        content.wantsLayer = true
        let first = NSTextField(frame: NSRect(x: 30, y: 30, width: 500, height: 40))
        let second = NSTextField(frame: NSRect(x: 30, y: 100, width: 500, height: 40))
        content.addSubview(first)
        content.addSubview(second)
        guard let context = RendererCAContext() else {
            fatalError("Could not create the renderer's remote CAContext")
        }

        func drain() {
            RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.02))
            app.updateWindows()
            CATransaction.flush()
        }
        func require(_ condition: @autoclosure () -> Bool, _ message: String) {
            guard condition() else { fatalError(message) }
        }
        func requireHostedTree(_ stage: String) {
            var ancestor = content.layer
            while ancestor != nil && ancestor !== context.layer {
                ancestor = ancestor?.superlayer
            }
            require(context.layer != nil && ancestor === context.layer, "Content disappeared from remote tree: \(stage)")
            require(content.layer?.superlayer != nil, "Content left the native window hierarchy: \(stage)")
        }

        // Exercise the first mount in production order, before any run-loop
        // drain can initialize the window's backing tree for the exporter.
        content.needsLayout = true
        content.layoutSubtreeIfNeeded()
        context.attachWindowLayerTree(for: content)
        drain()
        requireHostedTree("initial layout")
        print("PASS: first attachment survives the initial window update without a pre-attachment run-loop drain")
        // Do not reattach during focus transitions: that would mask the bug.
        for cycle in 0..<30 {
            require(window.makeFirstResponder(first), "Could not focus the first field")
            drain()
            requireHostedTree("focus \(cycle)")
            if cycle % 3 == 0 {
                require(window.makeFirstResponder(second), "Could not switch text fields")
                drain()
                requireHostedTree("field switch \(cycle)")
            }
            require(window.makeFirstResponder(cycle % 2 == 0 ? content : nil), "Could not blur the text field")
            drain()
            requireHostedTree("blur \(cycle)")
        }
        print("PASS: 30 focus/blur cycles and 10 field switches retain remote content")

        for cycle in 1...5 {
            window.setContentSize(NSSize(width: 800 + cycle * 20, height: 600 + cycle * 10))
            content.layoutSubtreeIfNeeded()
            context.attachWindowLayerTree(for: content)
            drain()
            requireHostedTree("resize \(cycle)")
        }
        print("PASS: 5 window resizes retain remote content")
        context.layer = nil
        window.contentView = nil
        window.close()
        require(context.layer == nil, "Closed surface retained its remote layer")
        print("PASS: closing the surface clears its exported tree")
    }
}
