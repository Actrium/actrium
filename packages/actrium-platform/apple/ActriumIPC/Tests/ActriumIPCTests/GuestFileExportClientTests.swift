import Darwin
import XCTest
@testable import ActriumIPC

@MainActor
final class GuestFileExportClientTests: XCTestCase {
    func testSendsMaximumFileWithoutBlockingAndWaitsForHostResult() async throws {
        let sockets = makeSocketPair()
        let tabId = UUID().uuidString
        let host = IPCConnection(tabId: tabId, connectedSocket: sockets[0])
        let renderer = IPCHelperConnection(tabId: tabId, connectedSocket: sockets[1])
        let client = GuestFileExportClient(connection: renderer)
        defer {
            renderer.disconnect()
            host.stop()
        }
        try renderer.connect()

        let received = expectation(description: "Host received the export")
        let completed = expectation(description: "Guest received the save result")
        let bytes = Data(repeating: 0xFF, count: GuestFileExportCall.maxBytes)
        let call = GuestFileExportCall(reloadKey: 1, name: "export.zip", data: bytes)
        host.onHostEvent = { name, json, receivedTabId in
            XCTAssertEqual(name, GuestFileExportCall.requestEvent)
            XCTAssertEqual(receivedTabId, tabId)
            let decoded = try? JSONDecoder().decode(GuestFileExportCall.self, from: Data(json.utf8))
            XCTAssertEqual(decoded?.id, call.id)
            XCTAssertEqual(decoded?.data, bytes)
            received.fulfill()
        }
        renderer.onHostEvent = { name, json, receivedTabId in
            XCTAssertEqual(name, GuestFileExportCall.responseEvent)
            client.receive(json: json, tabId: receivedTabId)
        }

        // The frame exceeds the socket buffer. Only start the Host reader after
        // send returns, so a synchronous send cannot pass this test.
        client.send(call, tabId: tabId) { reply in
            XCTAssertEqual(reply.id, call.id)
            XCTAssertTrue(reply.saved)
            completed.fulfill()
        }
        try host.start()
        await fulfillment(of: [received], timeout: 10)

        client.send(GuestFileExportCall(reloadKey: 1, name: "retry.zip", data: Data()), tabId: tabId) { reply in
            XCTAssertEqual(reply.error, "A file export is already in progress.")
        }
        let result = try JSONEncoder().encode(GuestFileExportReply(id: call.id, saved: true))
        try host.sendHostEvent(
            name: GuestFileExportCall.responseEvent, userInfoJSON: String(decoding: result, as: UTF8.self), tabId: tabId
        )
        await fulfillment(of: [completed], timeout: 5)
    }

    func testSendFailureCompletesTheRequestAndReleasesTheTab() async {
        let renderer = IPCHelperConnection(tabId: "tab", connectedSocket: -1)
        let client = GuestFileExportClient(connection: renderer)
        for name in ["first.zip", "retry.zip"] {
            let completed = expectation(description: "Failed to send \(name)")
            let call = GuestFileExportCall(reloadKey: 1, name: name, data: Data())
            client.send(call, tabId: "tab") { reply in
                XCTAssertEqual(reply.id, call.id)
                XCTAssertFalse(reply.saved)
                XCTAssertEqual(reply.error, "Could not send the file to the save dialog.")
                completed.fulfill()
            }
            await fulfillment(of: [completed], timeout: 2)
        }
    }

    private func makeSocketPair() -> [Int32] {
        var sockets: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &sockets), 0)
        // Bound the test if a synchronous writer fills the socket buffer.
        var timeout = timeval(tv_sec: 5, tv_usec: 0)
        XCTAssertEqual(setsockopt(sockets[1], SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size)), 0)
        return sockets
    }
}
