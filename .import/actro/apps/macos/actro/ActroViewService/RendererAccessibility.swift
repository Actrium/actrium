import ActroIPC
import AppKit

/// Walks the Renderer NSView / NSAccessibility tree and snapshots it for the host.
enum RendererAccessibility {
    static let maxNodes = RemoteAccessibilityLimits.maxNodes

    static func snapshot(
        tabId: String,
        root: NSView,
        generation: UInt64
    ) -> Actro_Ipc_AccessibilityTree {
        var nodes: [Actro_Ipc_AccessibilityNode] = []
        var focusedId: UInt64 = 0
        _ = collect(
            from: root,
            parentId: 0,
            root: root,
            includeSelf: true,
            nodes: &nodes,
            focusedId: &focusedId
        )
        var tree = Actro_Ipc_AccessibilityTree()
        tree.tabID = tabId
        tree.focusedID = focusedId
        tree.generation = generation
        tree.nodes = Array(nodes.prefix(RemoteAccessibilityLimits.maxNodes))
        return tree
    }

    static func perform(
        action: String,
        nodeId: UInt64,
        in root: NSView
    ) -> Bool {
        guard let view = findView(nodeId: nodeId, in: root) else {
            return false
        }
        switch action {
        case "AXPress", "press":
            return view.accessibilityPerformPress()
        case "AXShowMenu", "showMenu":
            return view.accessibilityPerformShowMenu()
        case "AXIncrement", "increment":
            return view.accessibilityPerformIncrement()
        case "AXDecrement", "decrement":
            return view.accessibilityPerformDecrement()
        case "AXConfirm", "confirm":
            return view.accessibilityPerformConfirm()
        case "AXCancel", "cancel":
            return view.accessibilityPerformCancel()
        case "AXRaise", "raise":
            return view.accessibilityPerformRaise()
        default:
            return false
        }
    }

    private static func collect(
        from view: NSView,
        parentId: UInt64,
        root: NSView,
        includeSelf: Bool,
        nodes: inout [Actro_Ipc_AccessibilityNode],
        focusedId: inout UInt64
    ) -> UInt64? {
        guard nodes.count < RemoteAccessibilityLimits.maxNodes else { return nil }

        let children = accessibilityChildren(of: view)
        let shouldEmit = includeSelf || view.isAccessibilityElement()

        if !shouldEmit {
            for child in children {
                _ = collect(
                    from: child,
                    parentId: parentId,
                    root: root,
                    includeSelf: false,
                    nodes: &nodes,
                    focusedId: &focusedId
                )
            }
            return nil
        }

        let nodeId = identifier(for: view)
        if view.isAccessibilityFocused() {
            focusedId = nodeId
        }

        let frame = convertFrame(view.accessibilityFrame(), into: root)
        var node = Actro_Ipc_AccessibilityNode()
        node.nodeID = nodeId
        node.parentID = parentId
        node.role = view.accessibilityRole()?.rawValue
            ?? NSAccessibility.Role.unknown.rawValue
        node.subrole = view.accessibilitySubrole()?.rawValue ?? ""
        node.label = view.accessibilityLabel() ?? ""
        node.value = stringValue(view.accessibilityValue())
        node.help = view.accessibilityHelp() ?? ""
        node.roleDescription = view.accessibilityRoleDescription() ?? ""
        node.x = frame.origin.x
        node.y = frame.origin.y
        node.width = frame.size.width
        node.height = frame.size.height
        node.enabled = view.isAccessibilityEnabled()
        node.focused = view.isAccessibilityFocused()
        node.selected = view.isAccessibilitySelected()
        node.actions = view.accessibilityActionNames().map(\.rawValue)

        let index = nodes.count
        nodes.append(node)

        var childIds: [UInt64] = []
        for child in children {
            if let childId = collect(
                from: child,
                parentId: nodeId,
                root: root,
                includeSelf: false,
                nodes: &nodes,
                focusedId: &focusedId
            ) {
                childIds.append(childId)
            }
        }
        nodes[index].childIds = childIds
        return nodeId
    }

    private static func accessibilityChildren(of view: NSView) -> [NSView] {
        if let declared = view.accessibilityChildren() as? [NSView], !declared.isEmpty {
            return declared
        }
        return view.subviews
    }

    private static func identifier(for view: NSView) -> UInt64 {
        UInt64(UInt(bitPattern: ObjectIdentifier(view)))
    }

    private static func convertFrame(_ screenFrame: NSRect, into root: NSView) -> NSRect {
        guard let window = root.window else { return screenFrame }
        let inWindow = window.convertFromScreen(screenFrame)
        return root.convert(inWindow, from: nil)
    }

    private static func stringValue(_ value: Any?) -> String {
        switch value {
        case let text as String:
            return text
        case let number as NSNumber:
            return number.stringValue
        case let object as NSObject:
            return object.description
        default:
            return ""
        }
    }

    private static func findView(nodeId: UInt64, in root: NSView) -> NSView? {
        if identifier(for: root) == nodeId {
            return root
        }
        for child in accessibilityChildren(of: root) {
            if let match = findView(nodeId: nodeId, in: child) {
                return match
            }
        }
        return nil
    }
}

enum RemoteAccessibilityLimits {
    static let maxNodes = 400
}
