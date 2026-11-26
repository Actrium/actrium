import Darwin
import XCTest
@testable import ActriumIPC

@MainActor
final class RendererIPCWriterTests: XCTestCase {
    func testPausedReaderReceivesLatestSnapshotPerTabAndEveryAcknowledgment() async throws {
        let (host, renderer) = try pausedConnection()
        defer { renderer.disconnect(); host.stop() }

        for generation in 1...20 {
            try renderer.sendViewportApplied(tabId: "one", requestId: UInt32(generation), width: 100, height: 100)
            try renderer.sendAccessibilityTree(snapshot(tabId: "one", generation: UInt64(generation)))
            try renderer.sendAccessibilityTree(snapshot(tabId: "two", generation: UInt64(generation)))
        }
        try renderer.sendHostEvent(name: "drained", userInfoJSON: "{}")

        var acknowledgments: [UInt32] = []
        var generations: [String: [UInt64]] = [:]
        let drained = expectation(description: "Host reads the last enqueued frame")
        host.onViewportApplied = { _, id, _, _ in acknowledgments.append(id) }
        host.onAccessibilityTree = { generations[$0.tabID, default: []].append($0.generation) }
        host.onHostEvent = { name, _, _ in
            if name == "drained" { drained.fulfill() }
        }
        try host.start()
        await fulfillment(of: [drained], timeout: 5)
        XCTAssertEqual(acknowledgments, (1...20).map(UInt32.init))
        XCTAssertEqual(generations, ["one": [20], "two": [20]])
    }

    func testQueueRejectsMoreBytesThanItsCapacityAndDisconnects() async throws {
        let (host, renderer) = try pausedConnection()
        defer { renderer.disconnect(); host.stop() }
        let disconnected = expectation(description: "renderer disconnects when a required frame cannot be enqueued")
        renderer.onDisconnected = { disconnected.fulfill() }

        let large = String(repeating: "x", count: IPCFrameLimits.maxFrameBytes / 2)
        XCTAssertThrowsError(try sendRequiredFrames(renderer, payload: large)) { error in
            guard case IPCError.sendQueueFull = error else { return XCTFail("Unexpected error: \(error)") }
        }
        await fulfillment(of: [disconnected], timeout: 3)
    }

    func testRequiredFramesEvictQueuedSnapshotWhenFrameSlotsFill() async throws {
        let (host, renderer) = try pausedConnection()
        defer { renderer.disconnect(); host.stop() }
        try renderer.sendAccessibilityTree(snapshot(tabId: "one", generation: 1))

        // One slot is still occupied by the blocked write.
        let ids = (1..<IPCFrameLimits.maxQueuedRendererFrames).map(UInt32.init)
        for id in ids {
            try renderer.sendViewportApplied(tabId: "one", requestId: id, width: 100, height: 100)
        }

        var received: [UInt32] = []
        let drained = expectation(description: "Host receives the last required frame")
        host.onViewportApplied = { _, id, _, _ in
            received.append(id)
            if id == ids.last { drained.fulfill() }
        }
        host.onAccessibilityTree = { _ in XCTFail("A replaceable snapshot occupied a required frame's slot") }
        try host.start()
        await fulfillment(of: [drained], timeout: 5)
        XCTAssertEqual(received, ids)
    }

    func testStorageAdmissionFailsDefinitivelyWhileRequiredFramesUseReservedSpace() async throws {
        let (host, renderer) = try pausedConnection()
        defer { renderer.disconnect(); host.stop() }
        let large = String(repeating: "x", count: 3 * IPCFrameLimits.maxFrameBytes / 4)
        for _ in 0..<3 {
            try renderer.sendHostEvent(name: "required", userInfoJSON: large)
        }

        let client = GuestLocalStorageClient(connection: renderer)
        let rejected = expectation(description: "the unsent request is rejected before its deadline")
        let call = GuestLocalStorageCall(
            id: UUID(), documentId: UUID(), operation: .getItem, key: "history", value: nil
        )
        client.send(call, tabId: "one") { reply in
            XCTAssertEqual(reply.error, "Too many pending local storage operations.")
            rejected.fulfill()
        }
        try renderer.sendViewportApplied(tabId: "one", requestId: 42, width: 100, height: 100)
        await fulfillment(of: [rejected], timeout: 1)
    }

    private func sendRequiredFrames(_ renderer: IPCHelperConnection, payload: String) throws {
        for _ in 0..<8 {
            try renderer.sendHostEvent(name: "required", userInfoJSON: payload)
        }
    }

    private func snapshot(tabId: String, generation: UInt64) -> Actrium_Ipc_AccessibilityTree {
        var tree = Actrium_Ipc_AccessibilityTree()
        tree.tabID = tabId
        tree.generation = generation
        return tree
    }

    private func pausedConnection() throws -> (IPCConnection, IPCHelperConnection) {
        var descriptors: [Int32] = [-1, -1]
        guard Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0 else {
            throw IPCError.socketCreationFailed(errno)
        }
        let host = IPCConnection(tabId: "tab", connectedSocket: descriptors[0])
        let renderer = IPCHelperConnection(tabId: "tab", connectedSocket: descriptors[1])
        var bytes: Int32 = 128 * 1024
        guard setsockopt(descriptors[1], SOL_SOCKET, SO_SNDBUF, &bytes, socklen_t(MemoryLayout.size(ofValue: bytes))) == 0 else {
            throw IPCError.socketCreationFailed(errno)
        }
        try renderer.connect()
        // Fill the send buffer so subsequent frames stay queued until the Host starts.
        let blocker = String(repeating: "x", count: 512 * 1024)
        try renderer.sendHostEvent(name: "blocker", userInfoJSON: blocker)
        return (host, renderer)
    }
}
