import ActriumCore
import ActriumIPC
import Foundation
import Testing
@testable import Actrium

@MainActor
struct RendererLocalStorageTests {
    @Test func signedStorageOriginFollowsTheProgramAcrossVersions() throws {
        let publisherKey = Data(repeating: 1, count: 32)
        let old = try #require(ActrPackageManifestInfo.signedStorageOrigin(
            appId: "dev.example/counter@1.0.0", signingKey: publisherKey
        ))
        #expect(ActrPackageManifestInfo.signedStorageOrigin(
            appId: "dev.example/counter@2.0.0", signingKey: publisherKey
        ) == old)
        #expect(ActrPackageManifestInfo.signedStorageOrigin(
            appId: "dev.example/other@1.0.0", signingKey: publisherKey
        ) != old)
        #expect(ActrPackageManifestInfo.signedStorageOrigin(
            appId: "dev.example/counter@1.0.0", signingKey: Data(repeating: 2, count: 32)
        ) != old)
    }

    @Test func guestRoundTripsThroughTheNativeRenderer() async throws {
        let tabId = UUID()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(tabId.uuidString)
        let database = GuestLocalStorage(path: url.path)
        let pool = RendererProcessPool(localStorage: RendererLocalStorage(database: database))
        let origin = "test:\(tabId.uuidString)"
        let permissions = ["getItem", "setItem", "removeItem"].map { "host:local-storage.\($0)" }
        let snapshot = try #require(CapabilityPermissionStore.shared.snapshotJSON(
            tabId: tabId, appId: nil, version: nil, declaredCapabilities: permissions
        ))
        defer {
            pool.releaseRenderer(for: tabId)
            if FileManager.default.fileExists(atPath: url.path) {
                try? FileManager.default.removeItem(at: url)
            }
        }

        _ = try pool.allocateRenderer(for: tabId, fingerprint: "test.storage.\(tabId.uuidString)")
        try await pool.prepareTab(tabId: tabId, config: [
            "title": "Storage Probe",
            "unifiedScript": Self.probeBundle,
            "containerWidth": 800,
            "containerHeight": 500,
            "localStorageOrigin": origin,
            "permissions": permissions,
            "capabilitySnapshotJSON": snapshot,
            "capabilityManifestJSON": Self.probeManifest,
        ])

        let deadline = ContinuousClock.now + .seconds(8)
        var outcome: String?
        while outcome == nil && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(100))
            outcome = try database.getItem(origin: origin, key: "report")
        }
        #expect(outcome == "dark/null")
    }

    @Test func brokerScopesStorageToOriginAndCurrentDocument() async throws {
        let broker = RendererLocalStorage(database: GuestLocalStorage(path: ":memory:"))
        let writer = UUID()
        let reader = UUID()
        let other = UUID()
        let writerConfig = configuration(origin: "app-a", permissions: ["host:local-storage.setItem"])
        let prepared = broker.prepare(tabId: writer, config: writerConfig)
        let writeId = try document(in: prepared)
        #expect(prepared["localStorageOrigin"] == nil)
        let readConfig = configuration(origin: "app-a", permissions: ["host:local-storage.getItem"])
        let readId = try document(in: broker.prepare(tabId: reader, config: readConfig))
        let otherConfig = configuration(origin: "app-b", permissions: ["host:local-storage.getItem"])
        let otherId = try document(in: broker.prepare(tabId: other, config: otherConfig))

        let write = try await request(broker, writer, writeId, .setItem, value: "dark")
        let read = try await request(broker, reader, readId, .getItem)
        let isolated = try await request(broker, other, otherId, .getItem)
        let denied = try await request(broker, reader, readId, .setItem, value: "light")
        #expect(write.error == nil)
        #expect(read.value == "dark")
        #expect(isolated.value == nil && isolated.error == nil)
        #expect(denied.error != nil)

        var reloaded = readConfig
        reloaded["reloadKey"] = 1
        let newId = try document(in: broker.prepare(tabId: reader, config: reloaded))
        let stale = try await request(broker, reader, readId, .getItem)
        let current = try await request(broker, reader, newId, .getItem)
        #expect(stale.error != nil)
        #expect(current.value == "dark")

        broker.remove(reader)
        let closed = try await request(broker, reader, newId, .getItem)
        #expect(closed.error != nil)
    }

    private func configuration(origin: String, permissions: [String]) -> [String: Any] {
        [
            "localStorageOrigin": origin,
            "permissions": permissions,
            "capabilityManifestJSON": #"{"hostModules":{"local-storage":{"rpc":["getItem","setItem"]}}}"#,
        ]
    }

    private func document(in config: [String: Any]) throws -> UUID {
        try #require((config["localStorageDocumentId"] as? String).flatMap(UUID.init(uuidString:)))
    }

    private func request(
        _ broker: RendererLocalStorage, _ tabId: UUID, _ documentId: UUID,
        _ operation: GuestLocalStorageCall.Operation, value: String? = nil
    ) async throws -> GuestLocalStorageReply {
        let call = GuestLocalStorageCall(id: UUID(), documentId: documentId, operation: operation, key: "theme", value: value)
        let input = String(decoding: try JSONEncoder().encode(call), as: UTF8.self)
        let response = try await broker.reply(to: input, tabId: tabId)
        let output = try #require(response)
        return try JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(output.utf8))
    }

    private static let probeManifest = """
        {"schemaVersion":1,"keelApi":"1.0","guestEntry":"workload/guest.js",
        "imports":["host:local-storage"],"components":[],
        "hostModules":{"local-storage":{"rpc":["getItem","removeItem","setItem"]}}}
        """

    private static let probeBundle = """
        // keel:v1-linkage {"guestEntry":"workload/guest.js","links":[["host:local-storage","getItem"],["host:local-storage","removeItem"],["host:local-storage","setItem"]]}
        var store = globalThis.__keel.hostModules['host:local-storage'];
        (async function () {
          await store.setItem({key: 'theme', value: 'dark'});
          var theme = await store.getItem({key: 'theme'});
          await store.removeItem({key: 'theme'});
          var missing = await store.getItem({key: 'theme'});
          await store.setItem({key: 'report', value: theme + '/' + missing});
        })();
        """
}
