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

    func testFinalSnapshotRejectedAtCapacityIsRecoveredWithoutAnotherUIChange() async throws {
        let (host, renderer) = try pausedConnection()
        defer { renderer.disconnect(); host.stop() }

        // One slot is occupied by the blocked write.
        for id in 1..<UInt32(IPCFrameLimits.maxQueuedNonRequiredFrames) {
            try renderer.sendViewportApplied(tabId: "one", requestId: id, width: 100, height: 100)
        }
        let finalTree = snapshot(tabId: "one", generation: 2)
        let recovered = expectation(description: "Host receives the final snapshot after capacity returns")
        let refresh = expectation(description: "renderer regenerates the snapshot without a UI event")
        let delivery = trackSnapshot(using: renderer, refresh: refresh) {
            self.snapshot(tabId: "one", generation: 3)
        }
        host.onAccessibilityTree = { tree in
            XCTAssertEqual(tree.tabID, finalTree.tabID)
            XCTAssertEqual(tree.generation, 3)
            recovered.fulfill()
        }
        try renderer.sendAccessibilityTree(finalTree, delivery: delivery)
        try host.start()
        await fulfillment(of: [refresh, recovered], timeout: 3)
    }

    func testRequiredFramesEvictQueuedSnapshotWhenFrameSlotsFill() async throws {
        let (host, renderer) = try pausedConnection()
        defer { renderer.disconnect(); host.stop() }
        let finalTree = snapshot(tabId: "one", generation: 1)
        let refresh = expectation(description: "renderer regenerates the evicted snapshot")
        let delivery = trackSnapshot(using: renderer, refresh: refresh) {
            self.snapshot(tabId: "one", generation: 2)
        }
        try renderer.sendAccessibilityTree(finalTree, delivery: delivery)

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
        let recovered = expectation(description: "Host receives the evicted snapshot after required frames")
        host.onAccessibilityTree = { tree in
            XCTAssertEqual(received, ids)
            XCTAssertEqual(tree.tabID, finalTree.tabID)
            XCTAssertEqual(tree.generation, 2)
            recovered.fulfill()
        }
        try host.start()
        await fulfillment(of: [drained, refresh, recovered], timeout: 5)
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

    func testRealmStreamTimesOutWhileReaderIsPausedAndNeverWritesExpiredRequest() async throws {
        let (host, renderer) = try pausedConnection()
        defer { renderer.disconnect(); host.stop() }
        let client = RealmServiceClient(connection: renderer)
        let call = streamCall(sequence: 1, timeoutMs: 50)
        let timedOut = expectation(description: "The blocked stream request times out")
        let responsive = expectation(description: "The main actor still processes work")
        client.send(call, tabId: "one") { reply in
            XCTAssertEqual(reply.id, call.id)
            XCTAssertEqual(reply.failure?.code, "realm_service_timeout")
            timedOut.fulfill()
        }
        DispatchQueue.main.async { responsive.fulfill() }
        await fulfillment(of: [responsive, timedOut], timeout: 1)

        let drained = expectation(description: "Host receives a frame queued after the expired request")
        host.onHostEvent = { name, _, _ in
            XCTAssertNotEqual(name, RealmServiceCall.requestEvent)
            if name == "drained" { drained.fulfill() }
        }
        try renderer.sendHostEvent(name: "drained", userInfoJSON: "{}")
        try host.start()
        await fulfillment(of: [drained], timeout: 3)
    }

    func testRealmStreamSendsFullChunksInOrderWhenReaderResumes() async throws {
        let (host, renderer) = try pausedConnection()
        defer { renderer.disconnect(); host.stop() }
        let client = RealmServiceClient(connection: renderer)
        let first = streamCall(sequence: 1, timeoutMs: 15_000)
        let second = streamCall(sequence: 2, timeoutMs: 15_000)
        let delivered = expectation(description: "Both calls get a reply")
        delivered.expectedFulfillmentCount = 2
        for call in [first, second] {
            client.send(call, tabId: "one") { reply in
                XCTAssertEqual(reply.id, call.id)
                XCTAssertEqual(reply.payload, Data())
                delivered.fulfill()
            }
        }
        let overflow = streamCall(sequence: 3, timeoutMs: 15_000)
        let rejected = expectation(description: "The full queue rejects the next chunk")
        client.send(overflow, tabId: "one") { reply in
            XCTAssertEqual(reply.id, overflow.id)
            XCTAssertEqual(reply.failure?.code, "realm_service_busy")
            rejected.fulfill()
        }
        await fulfillment(of: [rejected], timeout: 1)
        var sequences: [UInt64] = []
        renderer.onHostEvent = { _, json, tabId in client.receive(json: json, tabId: tabId) }
        host.onHostEvent = { name, json, tabId in
            guard name == RealmServiceCall.requestEvent else { return }
            do {
                let call = try JSONDecoder().decode(RealmServiceCall.self, from: Data(json.utf8))
                sequences.append(try XCTUnwrap(call.streamChunk).sequence)
                XCTAssertEqual(call.payload, first.payload)
                let reply = RealmServiceReply(id: call.id, payload: Data())
                let data = try JSONEncoder().encode(reply)
                try host.sendHostEvent(
                    name: RealmServiceCall.responseEvent, userInfoJSON: String(decoding: data, as: UTF8.self),
                    tabId: tabId
                )
            } catch {
                XCTFail("Failed to echo the stream call: \(error)")
            }
        }
        try host.start()
        await fulfillment(of: [delivered], timeout: 16)
        XCTAssertEqual(sequences, [1, 2])
    }

    private func streamCall(sequence: UInt64, timeoutMs: Int) -> RealmServiceCall {
        RealmServiceCall(
            id: UUID(), route: "example.Service.Upload",
            payload: Data(repeating: 0xFF, count: RealmServiceCall.maxStreamChunkBytes),
            timeoutMs: timeoutMs, streamChunk: .init(id: "stream", sequence: sequence)
        )
    }

    private func sendRequiredFrames(_ renderer: IPCHelperConnection, payload: String) throws {
        for _ in 0..<8 {
            try renderer.sendHostEvent(name: "required", userInfoJSON: payload)
        }
    }

    private func trackSnapshot(
        using renderer: IPCHelperConnection, refresh: XCTestExpectation,
        makeSnapshot: @escaping () -> Actrium_Ipc_AccessibilityTree
    ) -> AccessibilitySnapshotDelivery {
        let delivery = AccessibilitySnapshotDelivery()
        renderer.onAccessibilitySnapshotRefresh = { [weak renderer] in
            guard delivery.takeRefreshRequest() else { return }
            do {
                try renderer?.sendAccessibilityTree(makeSnapshot(), delivery: delivery)
                refresh.fulfill()
            } catch {
                XCTFail("Snapshot refresh failed: \(error)")
            }
        }
        return delivery
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
