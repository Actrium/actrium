import AppKit
import Darwin
import QuartzCore

/// A background renderer window must look active to native text controls.
/// NSTextView uses the key-window state to install its insertion indicator;
/// activating the helper NSApplication itself would steal focus from the Host.
final class RendererWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var isKeyWindow: Bool { true }
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
