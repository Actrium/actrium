import AppKit
import Testing
@testable import Actrium

struct RemoteAccessibilityTests {
    @Test func roleRoundTripUsesNativeAppKitRoles() {
        let button = RemoteAccessibilityRole.name(.button)
        #expect(button.contains("Button") || button == NSAccessibility.Role.button.rawValue)
        #expect(RemoteAccessibilityRole.role(from: button) == .button)
        #expect(RemoteAccessibilityRole.role(from: "") == .unknown)
    }

    @Test func hitTestPicksTheSmallestContainingNode() {
        let parent = RemoteAccessibilitySnapshot.node(
            id: 1,
            parent: 0,
            role: NSAccessibility.Role.group.rawValue,
            label: "Page",
            frame: NSRect(x: 0, y: 0, width: 200, height: 200),
            children: [2]
        )
        let child = RemoteAccessibilitySnapshot.node(
            id: 2,
            parent: 1,
            role: NSAccessibility.Role.button.rawValue,
            label: "Open",
            frame: NSRect(x: 20, y: 20, width: 40, height: 20),
            actions: ["AXPress"]
        )
        let tree = RemoteAccessibilitySnapshot.tree(
            tabId: "tab",
            nodes: [parent, child]
        )
        #expect(RemoteAccessibilitySnapshot.hitTest(tree, point: NSPoint(x: 25, y: 25)) == 2)
        #expect(RemoteAccessibilitySnapshot.hitTest(tree, point: NSPoint(x: 180, y: 180)) == 1)
        #expect(RemoteAccessibilitySnapshot.hitTest(tree, point: NSPoint(x: 400, y: 400)) == nil)
    }

    @Test func treeCapsNodeCount() {
        let nodes = (0..<RemoteAccessibilitySnapshot.maxNodes + 20).map { index in
            RemoteAccessibilitySnapshot.node(
                id: UInt64(index + 1),
                parent: 0,
                role: NSAccessibility.Role.staticText.rawValue,
                label: "n\(index)",
                frame: NSRect(x: 0, y: Double(index), width: 10, height: 10)
            )
        }
        let tree = RemoteAccessibilitySnapshot.tree(tabId: "tab", nodes: nodes)
        #expect(RemoteAccessibilitySnapshot.nodeCount(tree) == RemoteAccessibilitySnapshot.maxNodes)
    }

    @Test func elementExposesNativeRoleLabelAndPress() {
        let node = RemoteAccessibilitySnapshot.node(
            id: 1,
            parent: 0,
            role: NSAccessibility.Role.button.rawValue,
            label: "Save",
            frame: NSRect(x: 0, y: 0, width: 40, height: 20),
            actions: [NSAccessibility.Action.press.rawValue]
        )
        let element = RemoteAccessibilityElement(node: node, host: nil)
        #expect(element.accessibilityRole() == .button)
        #expect(element.accessibilityLabel() == "Save")
        #expect(element.accessibilityActionNames().contains(.press))
        #expect(element.accessibilityPerformPress() == false)
    }
}
