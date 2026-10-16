import ActriumIPC
import AppKit

/// One remote AX node exposed to VoiceOver in the host process.
final class RemoteAccessibilityElement: NSAccessibilityElement {
    weak var host: RendererSurfaceHostView?
    private(set) var node: Actrium_Ipc_AccessibilityNode
    private var childElements: [RemoteAccessibilityElement] = []

    init(node: Actrium_Ipc_AccessibilityNode, host: RendererSurfaceHostView?) {
        self.node = node
        self.host = host
        super.init()
        setAccessibilityElement(true)
        apply(node)
    }

    func apply(_ node: Actrium_Ipc_AccessibilityNode) {
        self.node = node
        setAccessibilityRole(RemoteAccessibilityRole.role(from: node.role))
        if !node.subrole.isEmpty {
            setAccessibilitySubrole(NSAccessibility.Subrole(rawValue: node.subrole))
        }
        setAccessibilityLabel(node.label.isEmpty ? nil : node.label)
        setAccessibilityValue(node.value.isEmpty ? nil : node.value)
        setAccessibilityHelp(node.help.isEmpty ? nil : node.help)
        setAccessibilityRoleDescription(
            node.roleDescription.isEmpty ? nil : node.roleDescription
        )
        setAccessibilityEnabled(node.enabled)
        setAccessibilityFocused(node.focused)
        setAccessibilitySelected(node.selected)
    }

    func setChildren(_ children: [RemoteAccessibilityElement]) {
        childElements = children
        setAccessibilityChildren(children)
        for child in children {
            child.setAccessibilityParent(self)
        }
    }

    override func accessibilityActionNames() -> [NSAccessibility.Action] {
        node.actions.map { RemoteAccessibilityRole.action(from: $0) }
    }

    override func accessibilityPerformAction(_ action: NSAccessibility.Action) {
        _ = host?.performRemoteAccessibilityAction(action.rawValue, nodeId: node.nodeID)
    }

    override func accessibilityFrame() -> NSRect {
        let local = NSRect(x: node.x, y: node.y, width: node.width, height: node.height)
        guard let host else { return local }
        let inWindow = host.convert(local, to: nil)
        return host.window?.convertToScreen(inWindow) ?? inWindow
    }

    override func accessibilityHitTest(_ point: NSPoint) -> Any? {
        guard let host, let window = host.window else { return self }
        let inWindow = window.convertFromScreen(NSRect(origin: point, size: .zero)).origin
        let local = host.convert(inWindow, from: nil)
        if let hit = childElements.reversed().first(where: {
            NSRect(x: $0.node.x, y: $0.node.y, width: $0.node.width, height: $0.node.height)
                .contains(local)
        }) {
            return hit.accessibilityHitTest(point) ?? hit
        }
        return self
    }

    override func accessibilityPerformPress() -> Bool {
        host?.performRemoteAccessibilityAction(
            NSAccessibility.Action.press.rawValue,
            nodeId: node.nodeID
        ) ?? false
    }

    override func accessibilityPerformShowMenu() -> Bool {
        host?.performRemoteAccessibilityAction(
            NSAccessibility.Action.showMenu.rawValue,
            nodeId: node.nodeID
        ) ?? false
    }

    override func accessibilityPerformIncrement() -> Bool {
        host?.performRemoteAccessibilityAction(
            NSAccessibility.Action.increment.rawValue,
            nodeId: node.nodeID
        ) ?? false
    }

    override func accessibilityPerformDecrement() -> Bool {
        host?.performRemoteAccessibilityAction(
            NSAccessibility.Action.decrement.rawValue,
            nodeId: node.nodeID
        ) ?? false
    }

    override func accessibilityPerformConfirm() -> Bool {
        host?.performRemoteAccessibilityAction(
            NSAccessibility.Action.confirm.rawValue,
            nodeId: node.nodeID
        ) ?? false
    }

    override func accessibilityPerformCancel() -> Bool {
        host?.performRemoteAccessibilityAction(
            NSAccessibility.Action.cancel.rawValue,
            nodeId: node.nodeID
        ) ?? false
    }
}
