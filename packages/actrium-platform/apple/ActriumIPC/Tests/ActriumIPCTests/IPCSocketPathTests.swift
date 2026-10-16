import Darwin
import XCTest
@testable import ActriumIPC

@MainActor
final class IPCSocketPathTests: XCTestCase {
    func testPathIsDeterministicAndFitsDarwinUnixSocketLimit() {
        let identifier = "renderer-" + String(repeating: "very-long-identifier-", count: 20)

        let first = IPCSocketPath.forTab(identifier)
        let second = IPCSocketPath.forTab(identifier)

        XCTAssertEqual(first, second)
        XCTAssertTrue(first.hasPrefix("/tmp/actrium-"))
        XCTAssertLessThanOrEqual(
            first.utf8CString.count,
            MemoryLayout<sockaddr_un>.size
        )
    }

    func testDifferentIdentifiersHaveDifferentPaths() {
        XCTAssertNotEqual(
            IPCSocketPath.forTab("renderer-a"),
            IPCSocketPath.forTab("renderer-b")
        )
    }

    func testConnectedSocketPairTransportsAFrameWithoutFilesystemPath() throws {
        var descriptors: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors), 0)

        let server = IPCServer(connectedSocket: descriptors[0])
        let client = IPCClient(connectedSocket: descriptors[1])
        let received = expectation(description: "server receives framed payload")
        let payload = Data("renderer-ready".utf8)

        server.onMessage = { data in
            XCTAssertEqual(data, payload)
            received.fulfill()
        }

        try server.start()
        try client.connect()
        try client.send(payload)
        wait(for: [received], timeout: 1)

        client.disconnect()
        server.stop()
    }

    func testRendererHostEventReachesBrokerOverSocketPair() throws {
        var descriptors: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors), 0)

        let broker = IPCConnection(tabId: "renderer-test", connectedSocket: descriptors[0])
        let renderer = IPCHelperConnection(tabId: "renderer-test", connectedSocket: descriptors[1])
        let received = expectation(description: "broker receives renderer host event")

        broker.onHostEvent = { name, json, tabId in
            XCTAssertEqual(name, "com.actro.openHistoryItem")
            XCTAssertEqual(tabId, "tab-1")
            XCTAssertTrue(json.contains("history-1"))
            received.fulfill()
        }

        try broker.start()
        try renderer.connect()
        try renderer.sendHostEvent(
            name: "com.actro.openHistoryItem",
            userInfoJSON: #"{"itemId":"history-1"}"#,
            tabId: "tab-1"
        )
        wait(for: [received], timeout: 1)

        renderer.disconnect()
        broker.stop()
    }

    func testClosedPeerReturnsWriteErrorInsteadOfRaisingSIGPIPE() throws {
        var descriptors: [Int32] = [-1, -1]
        XCTAssertEqual(Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors), 0)

        let client = IPCClient(connectedSocket: descriptors[0])
        try client.connect()
        Darwin.close(descriptors[1])

        XCTAssertThrowsError(try client.send(Data("after-close".utf8)))
        client.disconnect()
    }
}
