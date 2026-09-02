import ActriumIPC
import AppKit

/// Maps remote AX role strings onto AppKit roles. The strings are the system
/// `NSAccessibility.Role.rawValue` values so VoiceOver sees native roles.
enum RemoteAccessibilityRole {
    static func role(from raw: String) -> NSAccessibility.Role {
        let value = raw.isEmpty ? NSAccessibility.Role.unknown.rawValue : raw
        return NSAccessibility.Role(rawValue: value)
    }

    static func name(_ role: NSAccessibility.Role) -> String {
        role.rawValue
    }

    static func action(from raw: String) -> NSAccessibility.Action {
        NSAccessibility.Action(rawValue: raw)
    }
}

enum RemoteAccessibilitySnapshot {
    static let maxNodes = 400

    static func node(
        id: UInt64,
        parent: UInt64,
        role: String,
        label: String,
        value: String = "",
        frame: NSRect,
        children: [UInt64] = [],
        focused: Bool = false,
        actions: [String] = [NSAccessibility.Action.press.rawValue]
    ) -> Actrium_Ipc_AccessibilityNode {
        var node = Actrium_Ipc_AccessibilityNode()
        node.nodeID = id
        node.parentID = parent
        node.role = role
        node.label = label
        node.value = value
        node.x = frame.origin.x
        node.y = frame.origin.y
        node.width = frame.size.width
        node.height = frame.size.height
        node.enabled = true
        node.focused = focused
        node.actions = actions
        node.childIds = children
        return node
    }

    static func tree(
        tabId: String,
        nodes: [Actrium_Ipc_AccessibilityNode],
        focusedId: UInt64 = 0,
        generation: UInt64 = 1
    ) -> Actrium_Ipc_AccessibilityTree {
        var tree = Actrium_Ipc_AccessibilityTree()
        tree.tabID = tabId
        tree.focusedID = focusedId
        tree.generation = generation
        tree.nodes = Array(nodes.prefix(maxNodes))
        return tree
    }

    static func hitTest(
        _ tree: Actrium_Ipc_AccessibilityTree,
        point: NSPoint
    ) -> UInt64? {
        let candidates = tree.nodes.filter { node in
            NSRect(x: node.x, y: node.y, width: node.width, height: node.height)
                .contains(point)
        }
        return candidates.min { lhs, rhs in
            lhs.width * lhs.height < rhs.width * rhs.height
        }?.nodeID
    }

    static func nodeCount(_ tree: Actrium_Ipc_AccessibilityTree) -> Int {
        tree.nodes.count
    }
}
