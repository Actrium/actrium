import Darwin
import XCTest
@testable import ActriumIPC

@MainActor
final class GuestLocalStorageClientTests: XCTestCase {
    func testBlockedStorageWriteLeavesTheMainActorAndTimeoutResponsive() async throws {
        let descriptors = try socketPair()
        defer { Darwin.close(descriptors[0]) }
        let renderer = IPCHelperConnection(tabId: "tab", connectedSocket: descriptors[1])
        defer { renderer.disconnect() }
        try renderer.connect()

        // Release a synchronous writer so a regression fails instead of hanging the test.
        let watchdogSocket = Darwin.dup(descriptors[1])
        guard watchdogSocket >= 0 else { throw IPCError.socketCreationFailed(errno) }
        let watchdog = DispatchSource.makeTimerSource(queue: .global())
        watchdog.setEventHandler { Darwin.shutdown(watchdogSocket, SHUT_RDWR) }
        watchdog.setCancelHandler { Darwin.close(watchdogSocket) }
        watchdog.schedule(deadline: .now() + .seconds(3))
        watchdog.resume()
        defer { watchdog.cancel() }

        let items = (0..<16).map { ["key": "chunk/\($0)", "value": String(repeating: "x", count: 32 * 1024)] }
        let replacement = try decodeReplacement(items)
        let deadline = DispatchTime.now() + .seconds(1)
        let call = makeCall(replacement, deadline: deadline)
        let client = GuestLocalStorageClient(connection: renderer)
        let timedOut = expectation(description: "write times out while the peer is not reading")
        timedOut.assertForOverFulfill = true
        client.send(call, tabId: "tab") { reply in
            XCTAssertEqual(reply.errorCode?.rawValue, "local_storage_outcome_unknown")
            timedOut.fulfill()
        }
        let queued = expectation(description: "queued write expires before starting")
        let queuedCall = GuestLocalStorageCall(
            id: UUID(), documentId: UUID(), operation: .setItem, key: "queued", value: "value",
            deadlineUptimeNanoseconds: deadline.uptimeNanoseconds
        )
        client.send(queuedCall, tabId: "tab") { reply in
            XCTAssertEqual(reply.errorCode?.rawValue, "local_storage_timeout")
            queued.fulfill()
        }

        // A second renderer event must not wait on the same socket write lock.
        try renderer.sendHostEvent(name: "input", userInfoJSON: "{}", tabId: "tab")
        XCTAssertLessThan(DispatchTime.now(), deadline)
        await fulfillment(of: [timedOut, queued], timeout: 2)
    }

    func testReplacementThatFitsGuestJSONButExceedsEncodedLimitIsNeverSent() async throws {
        let descriptors = try socketPair()
        let host = IPCConnection(tabId: "tab", connectedSocket: descriptors[0])
        let renderer = IPCHelperConnection(tabId: "tab", connectedSocket: descriptors[1])
        defer { renderer.disconnect(); host.stop() }
        try host.start()
        try renderer.connect()

        let replacement = try replacementAtGuestLimit()
        let call = makeCall(replacement, deadline: .now() + .seconds(2))
        XCTAssertGreaterThan(try JSONEncoder().encode(call).count, GuestLocalStorageCall.maxRequestBytes)
        let rejected = expectation(description: "oversized replacement definitely rejected")
        let nextEvent = expectation(description: "connection can still send another event")
        host.onHostEvent = { name, _, _ in
            if name == "next" { nextEvent.fulfill() }
            else { XCTFail("Rejected replacement reached the Host: \(name)") }
        }
        let client = GuestLocalStorageClient(connection: renderer)
        client.send(call, tabId: "tab") { reply in
            XCTAssertEqual(reply.errorCode?.rawValue, "local_storage_request_too_large")
            rejected.fulfill()
        }
        try renderer.sendHostEvent(name: "next", userInfoJSON: "{}", tabId: "tab")
        await fulfillment(of: [rejected, nextEvent], timeout: 3)
    }

    private func socketPair() throws -> [Int32] {
        var descriptors: [Int32] = [-1, -1]
        guard Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0 else {
            throw IPCError.socketCreationFailed(errno)
        }
        var bytes: Int32 = 4096
        guard setsockopt(descriptors[1], SOL_SOCKET, SO_SNDBUF, &bytes, socklen_t(MemoryLayout.size(ofValue: bytes))) == 0 else {
            let error = IPCError.socketCreationFailed(errno)
            descriptors.forEach { Darwin.close($0) }
            throw error
        }
        return descriptors
    }

    private func replacementAtGuestLimit() throws -> GuestLocalStorageCall.Replacement {
        let valueBytes = 64 * 1024
        var items = (0..<112).map { ["key": "chunk/\($0)", "value": String(repeating: "x", count: valueBytes)] }
        let initial = try guestInput(items)
        let excess = initial.count - GuestLocalStorageCall.maxRequestBytes
        items[0]["value"] = String(repeating: "x", count: valueBytes - excess)
        let final = try guestInput(items)
        XCTAssertEqual(final.count, GuestLocalStorageCall.maxRequestBytes)
        return try JSONDecoder().decode(GuestInput.self, from: final).replacement
    }

    private struct GuestInput: Decodable {
        let replacement: GuestLocalStorageCall.Replacement
    }

    private func guestInput(_ items: [[String: String]]) throws -> Data {
        try JSONSerialization.data(withJSONObject: [
            "key": "history", "replacement": ["expected": NSNull(), "remove": [], "set": items] as [String: Any],
        ], options: .withoutEscapingSlashes)
    }

    private func decodeReplacement(_ items: [[String: String]]) throws -> GuestLocalStorageCall.Replacement {
        try JSONDecoder().decode(GuestInput.self, from: guestInput(items)).replacement
    }

    private func makeCall(_ replacement: GuestLocalStorageCall.Replacement, deadline: DispatchTime) -> GuestLocalStorageCall {
        GuestLocalStorageCall(
            id: UUID(), documentId: UUID(), operation: .replaceItems, key: "history", value: nil,
            replacement: replacement, deadlineUptimeNanoseconds: deadline.uptimeNanoseconds
        )
    }
}
