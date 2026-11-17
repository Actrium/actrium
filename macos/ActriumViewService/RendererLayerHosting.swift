import AppKit
import Darwin
import QuartzCore

/// The renderer stays offscreen. AppKit sees a key window only while the host
/// routes keyboard input here, so the native editor can own the insertion indicator.
final class RendererWindow: NSWindow {
    private(set) var hasHostKeyboardFocus = false
    private weak var suspendedInput: NSView?
    private var suspendedSelection = NSRange()

    override var canBecomeKey: Bool { true }
    override var isKeyWindow: Bool { hasHostKeyboardFocus }

    func setHostKeyboardFocus(_ focused: Bool) {
        guard hasHostKeyboardFocus != focused else { return }
        hasHostKeyboardFocus = focused
        if focused {
            restoreTextInput()
        } else {
            suspendTextInput()
        }
    }

    private func suspendTextInput() {
        guard let editor = firstResponder as? NSTextView, editor.isEditable else { return }
        // NSTextField can reuse its window's field editor for another field.
        // Keep the control and selection so we can reacquire the correct editor.
        suspendedInput = editor.isFieldEditor ? editor.delegate as? NSTextField : editor
        suspendedSelection = editor.selectedRange()
        makeFirstResponder(nil)
    }

    private func restoreTextInput() {
        guard let input = suspendedInput else { return }
        suspendedInput = nil
        guard input.window === self, !input.isHiddenOrHasHiddenAncestor else { return }
        if let field = input as? NSTextField {
            guard field.isEnabled, field.isEditable else { return }
        }
        if let textView = input as? NSTextView {
            guard textView.isEditable else { return }
        }
        guard makeFirstResponder(input), let editor = firstResponder as? NSTextView,
              suspendedSelection.location != NSNotFound else { return }
        // The guest may update its text while the Tab is inactive.
        let length = (editor.string as NSString).length
        let start = min(suspendedSelection.location, length)
        editor.setSelectedRange(NSRange(location: start, length: min(suspendedSelection.length, length - start)))
    }
}

/// Dynamic wrapper for the private CAContext used for cross-process hosting.
@MainActor
final class RendererCAContext {
    private let object: NSObject

    var contextId: UInt32 {
        if let number = object.value(forKey: "contextId") as? NSNumber {
            return number.uint32Value
        }
        return object.value(forKey: "contextId") as? UInt32 ?? 0
    }

    var layer: CALayer? {
        get { object.value(forKey: "layer") as? CALayer }
        set { object.setValue(newValue, forKey: "layer") }
    }

    /// Call after layout. AppKit reparents the content layer when its field
    /// editor ends editing. Exporting the whole native window keeps that
    /// reparent inside the exported tree instead of emptying the CAContext.
    func attachWindowLayerTree(for content: NSView) {
        if layer == nil {
            // Initialize the native backing tree synchronously. Without this,
            // the first window update can replace the root chosen below.
            content.window?.displayIfNeeded()
        }
        var root = content.superview?.layer ?? content.layer
        while let parent = root?.superlayer {
            root = parent
        }
        if layer !== root {
            layer = root
        }
    }

    init?() {
        guard let contextClass = NSClassFromString("CAContext") else {
            return nil
        }

        let options: [String: Any] = [
            "colorSpace": CGColorSpaceCreateDeviceRGB(),
            "displayId": CGMainDisplayID()
        ]

        let connectionSelector = NSSelectorFromString("contextWithCGSConnection:options:")
        if contextClass.responds(to: connectionSelector),
           let symbol = dlsym(dlopen(nil, RTLD_LAZY), "CGSMainConnectionID") {
            typealias ConnectionFunction = @convention(c) () -> UInt32
            let connection = unsafeBitCast(symbol, to: ConnectionFunction.self)()
            let implementation = contextClass.method(for: connectionSelector)
            typealias Factory = @convention(c) (
                AnyClass,
                Selector,
                UInt32,
                [String: Any]
            ) -> NSObject?
            if let created = unsafeBitCast(implementation, to: Factory.self)(
                contextClass,
                connectionSelector,
                connection,
                options
            ) {
                object = created
                return
            }
        }

        let remoteSelector = NSSelectorFromString("remoteContextWithOptions:")
        if contextClass.responds(to: remoteSelector) {
            let implementation = contextClass.method(for: remoteSelector)
            typealias Factory = @convention(c) (
                AnyClass,
                Selector,
                [String: Any]
            ) -> NSObject?
            if let created = unsafeBitCast(implementation, to: Factory.self)(
                contextClass,
                remoteSelector,
                options
            ) {
                object = created
                return
            }
        }

        return nil
    }
}
