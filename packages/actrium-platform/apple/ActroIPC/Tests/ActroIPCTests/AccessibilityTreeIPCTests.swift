import Darwin
import XCTest
@testable import ActroIPC

final class AccessibilityTreeIPCTests: XCTestCase {
    func testAccessibilityTreeRoundTripsThroughIPCMessage() throws {
        var node = Actro_Ipc_AccessibilityNode()
        node.nodeID = 42
        node.role = "AXButton"
        node.label = "Save"
        node.x = 8
        node.y = 16
        node.width = 64
        node.height = 24
        node.actions = ["AXPress"]

        var tree = Actro_Ipc_AccessibilityTree()
        tree.tabID = "tab-1"
        tree.focusedID = 42
        tree.generation = 3
        tree.nodes = [node]

        var message = Actro_Ipc_IPCMessage()
        message.accessibilityTree = tree

        let decoded = try Actro_Ipc_IPCMessage(serializedData: try message.serializedData())
        XCTAssertEqual(decoded.accessibilityTree.tabID, "tab-1")
        XCTAssertEqual(decoded.accessibilityTree.focusedID, 42)
        XCTAssertEqual(decoded.accessibilityTree.nodes.first?.label, "Save")
        XCTAssertEqual(decoded.accessibilityTree.nodes.first?.actions, ["AXPress"])
    }

    func testAccessibilityActionRequestRoundTrips() throws {
        var message = Actro_Ipc_IPCMessage()
        message.accessibilityAction.tabID = "tab-1"
        message.accessibilityAction.nodeID = 7
        message.accessibilityAction.action = "AXPress"

        let decoded = try Actro_Ipc_IPCMessage(serializedData: try message.serializedData())
        XCTAssertEqual(decoded.accessibilityAction.tabID, "tab-1")
        XCTAssertEqual(decoded.accessibilityAction.nodeID, 7)
        XCTAssertEqual(decoded.accessibilityAction.action, "AXPress")
    }

    func testAccessibilityTreePushReachesBroker() throws {
        var descriptors: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors), 0)

        let broker = IPCConnection(tabId: "renderer-test", connectedSocket: descriptors[0])
        let renderer = IPCHelperConnection(tabId: "renderer-test", connectedSocket: descriptors[1])
        let received = expectation(description: "broker receives accessibility tree")

        broker.onAccessibilityTree = { tree in
            XCTAssertEqual(tree.tabID, "tab-1")
            XCTAssertEqual(tree.focusedID, 1)
            XCTAssertEqual(tree.nodes.first?.label, "Open")
            received.fulfill()
        }

        try broker.start()
        try renderer.connect()

        var node = Actro_Ipc_AccessibilityNode()
        node.nodeID = 1
        node.role = "AXButton"
        node.label = "Open"
        var tree = Actro_Ipc_AccessibilityTree()
        tree.tabID = "tab-1"
        tree.focusedID = 1
        tree.nodes = [node]
        try renderer.sendAccessibilityTree(tree)

        wait(for: [received], timeout: 1)
        renderer.disconnect()
        broker.stop()
    }

    func testAccessibilityActionRoundTrip() throws {
        var descriptors: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors), 0)

        let broker = IPCConnection(tabId: "renderer-test", connectedSocket: descriptors[0])
        let renderer = IPCHelperConnection(tabId: "renderer-test", connectedSocket: descriptors[1])
        let handled = expectation(description: "renderer handles accessibility action")
        let completed = expectation(description: "broker receives action result")

        renderer.onAccessibilityAction = { request, reply in
            XCTAssertEqual(request.tabID, "tab-1")
            XCTAssertEqual(request.nodeID, 9)
            XCTAssertEqual(request.action, "AXPress")
            handled.fulfill()
            reply(true, nil)
        }

        try broker.start()
        try renderer.connect()

        Task {
            let ok = try await broker.sendAccessibilityAction(
                tabId: "tab-1",
                nodeId: 9,
                action: "AXPress"
            )
            XCTAssertTrue(ok)
            completed.fulfill()
        }

        wait(for: [handled, completed], timeout: 2)
        renderer.disconnect()
        broker.stop()
    }

    func testGetAccessibilityTreeRoundTrip() throws {
        var descriptors: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors), 0)

        let broker = IPCConnection(tabId: "renderer-test", connectedSocket: descriptors[0])
        let renderer = IPCHelperConnection(tabId: "renderer-test", connectedSocket: descriptors[1])
        let completed = expectation(description: "broker receives tree")

        renderer.onGetAccessibilityTree = { tabId, reply in
            XCTAssertEqual(tabId, "tab-1")
            var node = Actro_Ipc_AccessibilityNode()
            node.nodeID = 3
            node.label = "Home"
            var tree = Actro_Ipc_AccessibilityTree()
            tree.tabID = tabId
            tree.generation = 2
            tree.nodes = [node]
            reply(tree)
        }

        try broker.start()
        try renderer.connect()

        Task {
            let tree = try await broker.getAccessibilityTree(tabId: "tab-1")
            XCTAssertEqual(tree.tabID, "tab-1")
            XCTAssertEqual(tree.generation, 2)
            XCTAssertEqual(tree.nodes.first?.label, "Home")
            completed.fulfill()
        }

        wait(for: [completed], timeout: 2)
        renderer.disconnect()
        broker.stop()
    }
}
