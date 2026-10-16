import Darwin
import XCTest
@testable import ActriumIPC

@MainActor
final class IPCLifecycleTests: XCTestCase {
    func testPeerDisconnectFinishesPendingRequest() async throws {
        var descriptors: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors), 0)
        let broker = IPCConnection(tabId: "pending", connectedSocket: descriptors[0])
        let renderer = IPCHelperConnection(tabId: "pending", connectedSocket: descriptors[1])
        defer {
            renderer.onGetAccessibilityTree = nil
            renderer.disconnect()
            broker.stop()
        }
        renderer.onGetAccessibilityTree = { _, _ in renderer.disconnect() }
        try broker.start()
        try renderer.connect()

        do {
            _ = try await broker.getAccessibilityTree(tabId: "pending")
            XCTFail("A disconnected peer cannot complete the request")
        } catch IPCError.connectionClosed {
            // Disconnect must finish the request before its timeout.
        } catch {
            XCTFail("Unexpected request failure: \(error)")
        }
    }

    func testStopFinishesPendingRequestBeforeLateReply() async throws {
        var descriptors: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors), 0)
        let broker = IPCConnection(tabId: "stopped", connectedSocket: descriptors[0])
        let renderer = IPCHelperConnection(tabId: "stopped", connectedSocket: descriptors[1])
        defer {
            renderer.onGetAccessibilityTree = nil
            renderer.disconnect()
            broker.stop()
        }
        renderer.onGetAccessibilityTree = { _, reply in
            broker.stop()
            reply(Actrium_Ipc_AccessibilityTree())
        }
        try broker.start()
        try renderer.connect()

        do {
            _ = try await broker.getAccessibilityTree(tabId: "stopped")
            XCTFail("Stop must cancel the pending request")
        } catch IPCError.connectionClosed {
            // The late reply must not resume the continuation a second time.
        } catch {
            XCTFail("Unexpected request failure: \(error)")
        }
    }

    func testStopShutsDownReaderWaitingForPartialFrame() throws {
        var descriptors: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors), 0)
        let server = IPCServer(connectedSocket: descriptors[0])
        let client = IPCClient(connectedSocket: descriptors[1])
        let disconnected = expectation(description: "peer observes shutdown")
        client.onDisconnected = { disconnected.fulfill() }
        try server.start()
        try client.connect()
        var headerByte: UInt8 = 0
        XCTAssertEqual(Darwin.write(descriptors[1], &headerByte, 1), 1)

        server.stop()
        wait(for: [disconnected], timeout: 2)
        XCTAssertThrowsError(try server.send(Data([1])))
        client.disconnect()
    }

    func testFilesystemListenerAcceptsAnotherClientAfterDisconnect() throws {
        let path = "/tmp/ipc-lifecycle-\(UUID().uuidString).sock"
        let server = IPCServer(socketPath: path)
        defer { server.stop() }
        try server.start()

        for value: UInt8 in [1, 2] {
            let connected = expectation(description: "client connected")
            let received = expectation(description: "frame received")
            let disconnected = expectation(description: "client disconnected")
            server.onConnected = { connected.fulfill() }
            server.onMessage = { data in
                XCTAssertEqual(data, Data([value]))
                received.fulfill()
            }
            server.onDisconnected = { disconnected.fulfill() }
            let client = IPCClient(socketPath: path)
            try client.connect()
            wait(for: [connected], timeout: 2)
            try client.send(Data([value]))
            wait(for: [received], timeout: 2)
            client.disconnect()
            wait(for: [disconnected], timeout: 2)
        }
    }

}
