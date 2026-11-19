import ActriumCore
import ActriumIPC
import Foundation
import Testing
@testable import Actrium

@MainActor
struct RendererLocalStorageAdmissionTests {
    @Test func queuedOperationsStayBoundedAfterRevocationAndCallerCancellation() async throws {
        let store = BlockingStorage(path: ":memory:")
        let broker = RendererLocalStorage(database: store, pendingLimit: 2)
        let tab = UUID()
        let document = try prepare(broker, tab: tab)
        let first = Task { try await send(broker, tab, document, value: "first") }
        defer { store.resume.signal() }
        let timeout = Task {
            try await Task.sleep(for: .seconds(5))
            store.started.continuation.finish()
        }
        defer { timeout.cancel() }
        var started = store.started.stream.makeAsyncIterator()
        try #require(await started.next() != nil)

        let second = Task { try await send(broker, tab, document, value: "second") }
        let deadline = ContinuousClock.now + .seconds(2)
        while broker.pendingOperationCount != 2 && ContinuousClock.now < deadline {
            await Task.yield()
        }
        try #require(broker.pendingOperationCount == 2)
        second.cancel()
        broker.remove(tab)
        let replacement = try prepare(broker, tab: tab, origin: "app-b")
        let rejected = try await send(broker, tab, replacement, value: "must-not-run")
        #expect(rejected.error == "Too many pending local storage operations.")
        #expect(broker.pendingOperationCount == 2)

        store.resume.signal()
        #expect(try await first.value.error == "The local storage document changed.")
        #expect(try await second.value.error == "The local storage document changed.")
        #expect(broker.pendingOperationCount == 0)
        #expect(try store.getItem(origin: "app-a", key: "key") == "second")
        let accepted = try await send(broker, tab, replacement, value: "after-drain")
        #expect(accepted.error == nil)
        #expect(try store.getItem(origin: "app-b", key: "key") == "after-drain")
        #expect(try store.getItem(origin: "app-a", key: "key") == "second")
    }

    @Test func failedDatabaseOperationsReleaseAdmission() async throws {
        let broker = RendererLocalStorage(database: GuestLocalStorage(path: "/dev/null/database"), pendingLimit: 1)
        let tab = UUID()
        let document = try prepare(broker, tab: tab)
        for _ in 0..<2 {
            let response = try await send(broker, tab, document, value: "value")
            #expect(response.error == "The local storage database operation failed.")
            #expect(broker.pendingOperationCount == 0)
        }
    }

    private func prepare(_ broker: RendererLocalStorage, tab: UUID, origin: String = "app-a") throws -> UUID {
        let config = broker.prepare(tabId: tab, config: [
            "localStorageOrigin": origin,
            "permissions": ["host:local-storage.setItem"],
            "capabilityManifestJSON": #"{"hostModules":{"local-storage":{"rpc":["setItem"]}}}"#,
        ])
        return try #require((config["localStorageDocumentId"] as? String).flatMap(UUID.init(uuidString:)))
    }

    private func send(
        _ broker: RendererLocalStorage, _ tab: UUID, _ document: UUID, value: String
    ) async throws -> GuestLocalStorageReply {
        let call = GuestLocalStorageCall(id: UUID(), documentId: document, operation: .setItem, key: "key", value: value)
        let json = String(decoding: try JSONEncoder().encode(call), as: UTF8.self)
        let reply = try #require(try await broker.reply(to: json, tabId: tab))
        return try JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(reply.utf8))
    }
}

private final class BlockingStorage: GuestLocalStorage, @unchecked Sendable {
    let started = AsyncStream<Void>.makeStream()
    let resume = DispatchSemaphore(value: 0)

    override func setItem(origin: String, key: String, value: String) throws {
        if value == "first" {
            started.continuation.yield(())
            guard resume.wait(timeout: .now() + 5) == .success else {
                throw GuestLocalStorageError.DatabaseFailure(message: "The test write was never resumed.")
            }
        }
        try super.setItem(origin: origin, key: key, value: value)
    }
}
