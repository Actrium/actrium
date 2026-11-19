import ActriumCore
import ActriumIPC
import Darwin
import Foundation
import Testing
@testable import Actrium

@MainActor
struct RendererLocalStorageTimeoutTests {
    @Test func expiredQueuedWritesAndDeletesNeverChangeStoredData() async throws {
        for operation in [GuestLocalStorageCall.Operation.setItem, .removeItem] {
            let store = TimeoutStorage(path: ":memory:")
            try store.setItem(origin: "app", key: "key", value: "original")
            let broker = RendererLocalStorage(database: store)
            let tab = UUID()
            let document = try prepare(broker, tab)
            let blocker = Task { try await send(broker, tab, call(document, .setItem, key: "barrier", value: "blocked")) }
            defer { store.resume.signal() }
            try await store.waitUntilStarted()
            let pair = try StorageTransport(broker: broker, tab: tab)
            defer { pair.close() }
            let request = try expiring(call(document, operation, value: operation == .setItem ? "changed" : nil))
            let response = try await pair.send(request)
            #expect(try errorCode(response) == "local_storage_outcome_unknown")
            #expect(broker.pendingOperationCount == 2)
            store.resume.signal()
            _ = try await blocker.value
            let hostReply = try await pair.nextHostReply()
            #expect(try errorCode(hostReply) == "local_storage_timeout")
            #expect(try store.getItem(origin: "app", key: "key") == "original")
            #expect(broker.pendingOperationCount == 0)
        }
    }

    @Test(arguments: [GuestLocalStorageCall.Operation.setItem, .removeItem])
    func anInProgressWriteReportsUnknownThenMayCompleteWithoutASecondReply(_ operation: GuestLocalStorageCall.Operation) async throws {
        try await verifyRunningWrite(operation)
    }

    @Test func aRevokedRunningWriteKeepsItsUnknownOutcome() async throws {
        try await verifyRunningWrite(.setItem, revoke: true)
    }

    private func verifyRunningWrite(_ operation: GuestLocalStorageCall.Operation, revoke: Bool = false) async throws {
        let store = TimeoutStorage(path: ":memory:")
        try store.setItem(origin: "app", key: "key", value: "original")
        let broker = RendererLocalStorage(database: store)
        let tab = UUID()
        let document = try prepare(broker, tab)
        let pair = try StorageTransport(broker: broker, tab: tab)
        defer { pair.close(); store.resume.signal() }
        let request = try expiring(call(document, operation, value: operation == .setItem ? "blocked" : nil))
        let result = Task { try await pair.send(request) }
        try await store.waitUntilStarted()
        if revoke { broker.remove(tab) }
        let response = try await result.value
        #expect(try errorCode(response) == "local_storage_outcome_unknown")
        #expect(response.error?.contains("unknown") == true)
        store.resume.signal()
        let hostReply = try await pair.nextHostReply()
        if revoke {
            #expect(try errorCode(hostReply) == "local_storage_outcome_unknown")
        } else {
            #expect(hostReply.error == nil)
        }
        #expect(try store.getItem(origin: "app", key: "key") == (operation == .setItem ? "blocked" : nil))
        #expect(pair.completions == 1)
    }

    @Test func anExpiredUnsentWriteIsDefinitivelyRejectedWithoutIPC() async throws {
        let broker = RendererLocalStorage(database: GuestLocalStorage(path: ":memory:"))
        let tab = UUID()
        let document = try prepare(broker, tab)
        let pair = try StorageTransport(broker: broker, tab: tab)
        defer { pair.close() }
        let response = try await pair.send(expiring(call(document, .setItem, value: "changed"), deadline: 0))
        #expect(try errorCode(response) == "local_storage_timeout")
        #expect(pair.requests == 0)
    }

    @Test func aReadWatchdogReportsTimeoutWithoutClaimingAnUnknownWrite() async throws {
        let pair = try StorageTransport()
        defer { pair.close() }
        let response = try await pair.send(expiring(call(UUID(), .getItem), budget: .milliseconds(200)))
        #expect(try errorCode(response) == "local_storage_timeout")
        #expect(response.error?.contains("unknown") != true)
    }

    @Test func aKnownSuccessfulWriteWinsAndItsWatchdogCannotReplyAgain() async throws {
        let store = TimeoutStorage(path: ":memory:")
        let broker = RendererLocalStorage(database: store)
        let tab = UUID()
        let document = try prepare(broker, tab)
        let pair = try StorageTransport(broker: broker, tab: tab)
        defer { pair.close() }
        let request = try expiring(call(document, .setItem, value: "saved"))
        let response = try await pair.send(request)
        #expect(response.error == nil)
        #expect(try store.getItem(origin: "app", key: "key") == "saved")
        let remaining = requestDeadline(request)
        if remaining > 0 { try await Task.sleep(nanoseconds: remaining) }
        #expect(pair.completions == 1)
    }

    private func requestDeadline(_ call: GuestLocalStorageCall) -> UInt64 {
        let now = DispatchTime.now().uptimeNanoseconds
        let deadline = call.deadlineUptimeNanoseconds + 50_000_000
        return deadline > now ? deadline - now : 0
    }

    private func prepare(_ broker: RendererLocalStorage, _ tab: UUID) throws -> UUID {
        let methods = GuestLocalStorageCall.Operation.allCases.map(\.rawValue)
        let manifest = try JSONSerialization.data(withJSONObject: ["hostModules": ["local-storage": ["rpc": methods]]])
        let config = broker.prepare(tabId: tab, config: [
            "localStorageOrigin": "app", "permissions": methods.map { "host:local-storage.\($0)" },
            "capabilityManifestJSON": String(decoding: manifest, as: UTF8.self),
        ])
        return try #require((config["localStorageDocumentId"] as? String).flatMap(UUID.init(uuidString:)))
    }

    private func call(
        _ document: UUID, _ operation: GuestLocalStorageCall.Operation, key: String = "key", value: String? = nil
    ) -> GuestLocalStorageCall {
        GuestLocalStorageCall(id: UUID(), documentId: document, operation: operation, key: key, value: value)
    }

    private func expiring(
        _ call: GuestLocalStorageCall, deadline: UInt64? = nil,
        budget: DispatchTimeInterval = .seconds(GuestLocalStorageCall.timeoutSeconds)
    ) throws -> GuestLocalStorageCall {
        var json = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(call)) as? [String: Any])
        json["deadlineUptimeNanoseconds"] = deadline ?? (DispatchTime.now() + budget).uptimeNanoseconds
        return try JSONDecoder().decode(GuestLocalStorageCall.self, from: JSONSerialization.data(withJSONObject: json))
    }

    private func errorCode(_ reply: GuestLocalStorageReply) throws -> String? {
        let json = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(reply)) as? [String: Any])
        return json["errorCode"] as? String
    }

    private func send(_ broker: RendererLocalStorage, _ tab: UUID, _ call: GuestLocalStorageCall) async throws -> GuestLocalStorageReply {
        let json = String(decoding: try JSONEncoder().encode(call), as: UTF8.self)
        let reply = try #require(try await broker.reply(to: json, tabId: tab))
        return try JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(reply.utf8))
    }
}

@MainActor
private final class StorageTransport {
    private let host: IPCConnection
    private let helper: IPCHelperConnection
    private let client: GuestLocalStorageClient
    private let replies = AsyncStream<GuestLocalStorageReply>.makeStream()
    private let results = AsyncStream<GuestLocalStorageReply>.makeStream()
    private let tab: UUID
    private(set) var requests = 0
    private(set) var completions = 0

    init(broker: RendererLocalStorage? = nil, tab: UUID = UUID()) throws {
        var descriptors: [Int32] = [-1, -1]
        guard Darwin.socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0 else {
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
        }
        self.tab = tab
        host = IPCConnection(tabId: tab.uuidString, connectedSocket: descriptors[0])
        helper = IPCHelperConnection(tabId: tab.uuidString, connectedSocket: descriptors[1])
        client = GuestLocalStorageClient(connection: helper)
        host.onHostEvent = { [weak self] _, json, _ in
            guard let self else { return }
            requests += 1
            guard let broker else { return }
            Task {
                do {
                    if let reply = try await broker.reply(to: json, tabId: tab) {
                        try self.host.sendHostEvent(
                            name: GuestLocalStorageCall.responseEvent, userInfoJSON: reply, tabId: tab.uuidString
                        )
                    }
                } catch { Issue.record(error) }
            }
        }
        helper.onHostEvent = { [weak self] _, json, tabId in
            guard let self else { return }
            do {
                let decoded = try JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(json.utf8))
                client.receive(json: json, tabId: tabId)
                replies.continuation.yield(decoded)
            } catch { Issue.record(error) }
        }
        try host.start()
        try helper.connect()
    }

    func send(_ call: GuestLocalStorageCall) async throws -> GuestLocalStorageReply {
        client.send(call, tabId: tab.uuidString) { [weak self] reply in
            self?.completions += 1
            self?.results.continuation.yield(reply)
        }
        return try await next(results)
    }

    func nextHostReply() async throws -> GuestLocalStorageReply { try await next(replies) }

    func close() {
        helper.disconnect()
        host.stop()
        results.continuation.finish()
        replies.continuation.finish()
    }

    private func next(_ stream: (stream: AsyncStream<GuestLocalStorageReply>, continuation: AsyncStream<GuestLocalStorageReply>.Continuation)) async throws -> GuestLocalStorageReply {
        let timeout = Task {
            try await Task.sleep(for: .seconds(7))
            stream.continuation.finish()
        }
        defer { timeout.cancel() }
        var iterator = stream.stream.makeAsyncIterator()
        return try #require(await iterator.next())
    }
}

private final class TimeoutStorage: GuestLocalStorage, @unchecked Sendable {
    private let started = AsyncStream<Void>.makeStream()
    let resume = DispatchSemaphore(value: 0)

    override func setItem(origin: String, key: String, value: String) throws {
        if value == "blocked" {
            try pause()
        }
        try super.setItem(origin: origin, key: key, value: value)
    }

    override func removeItem(origin: String, key: String) throws {
        try pause()
        try super.removeItem(origin: origin, key: key)
    }

    private func pause() throws {
        started.continuation.yield(())
        guard resume.wait(timeout: .now() + 10) == .success else {
            throw GuestLocalStorageError.DatabaseFailure(message: "The test did not resume its write.")
        }
    }

    func waitUntilStarted() async throws {
        let timeout = Task {
            try await Task.sleep(for: .seconds(7))
            started.continuation.finish()
        }
        defer { timeout.cancel() }
        var iterator = started.stream.makeAsyncIterator()
        try #require(await iterator.next() != nil)
    }
}
