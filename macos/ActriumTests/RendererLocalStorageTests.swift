import ActriumCore
import ActriumIPC
import CoreGraphics
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
        let permissions = ["getItem", "setItem", "removeItem"].map { "host:local-storage.\($0)" }
        var tab = Tab(
            title: "Storage Probe", unifiedScript: Self.probeBundle,
            capabilityManifestJSON: Self.probeManifest, permissions: permissions
        )
        let tabId = tab.id
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(tabId.uuidString)
        let database = try GuestLocalStorage.newEncrypted(path: url.path, key: Data(repeating: 42, count: 32))
        let pool = RendererProcessPool(localStorage: RendererLocalStorage(database: database))
        let origin = "test:\(tabId.uuidString)"
        tab.localStorageOrigin = origin
        defer {
            pool.releaseRenderer(for: tabId)
            if FileManager.default.fileExists(atPath: url.path) {
                try? FileManager.default.removeItem(at: url)
            }
        }

        _ = try pool.allocateRenderer(for: tabId, fingerprint: "test.storage.\(tabId.uuidString)")
        for revision in 0...2 {
            tab.reloadNonce = min(revision, 1)
            tab.capabilityRevision = revision
            if revision == 2 { tab.permissions.append("host:clipboard.write") }
            try database.removeItem(origin: origin, key: "report")
            try await pool.prepareTab(tabId: tabId, config: rendererConfig(for: tab))

            let deadline = ContinuousClock.now + .seconds(8)
            var outcome: String?
            while outcome == nil && ContinuousClock.now < deadline {
                try await Task.sleep(for: .milliseconds(100))
                outcome = try database.getItem(origin: origin, key: "report")
            }
            #expect(outcome == "[\"dark\\u0000\u{e9}🌲\",\"plain\",null]")
        }
    }

    @Test func resentConfigsPreserveStorageAcrossReloadAndCapabilityChanges() async throws {
        let broker = RendererLocalStorage(database: GuestLocalStorage(path: ":memory:"))
        var tab = Tab(
            title: "Storage Probe", unifiedScript: Self.probeBundle,
            capabilityManifestJSON: Self.probeManifest, localStorageOrigin: "app-a",
            permissions: ["host:local-storage.getItem", "host:local-storage.setItem"]
        )
        let initial = broker.prepare(tabId: tab.id, config: rendererConfig(for: tab))
        let firstId = try document(in: initial)
        let write = try await request(broker, tab.id, firstId, .setItem, value: "dark")
        #expect(write.error == nil)

        tab.reloadNonce += 1
        let reloaded = broker.prepare(tabId: tab.id, config: rendererConfig(for: tab))
        let reloadedId = try document(in: reloaded)
        #expect(reloadedId != firstId)
        let stale = try await request(broker, tab.id, firstId, .getItem)
        #expect(stale.error != nil)

        tab.permissions.append("host:clipboard.write")
        tab.capabilityRevision += 1
        let updated = broker.prepare(tabId: tab.id, config: rendererConfig(for: tab))
        let currentId = try document(in: updated)
        #expect(currentId == reloadedId)
        let read = try await request(broker, tab.id, currentId, .getItem)
        #expect(read.value == "dark" && read.error == nil)
        #expect(updated["localStorageOrigin"] == nil)
        #expect(updated["capabilitySnapshotJSON"] as? String != nil)
        #expect(updated["containerWidth"] as? CGFloat == 800)
        #expect(updated["containerHeight"] as? CGFloat == 600)
    }

    @Test func aConfigWithoutAnOriginRevokesThePreviousDocument() async throws {
        let broker = RendererLocalStorage(database: GuestLocalStorage(path: ":memory:"))
        let tabId = UUID()
        var config = configuration(origin: "app-a", permissions: ["host:local-storage.getItem"])
        let previousId = try document(in: broker.prepare(tabId: tabId, config: config))
        config.removeValue(forKey: "localStorageOrigin")
        let revoked = broker.prepare(tabId: tabId, config: config)
        #expect(revoked["localStorageDocumentId"] == nil)
        let denied = try await request(broker, tabId, previousId, .getItem)
        #expect(denied.error != nil)
    }

    @Test func anAcceptedWriteCompletesAfterItsDocumentIsRevoked() async throws {
        let database = PausedLocalStorage(path: ":memory:")
        let broker = RendererLocalStorage(database: database)
        let tabId = UUID()
        let permissions = ["host:local-storage.getItem", "host:local-storage.setItem"]
        let original = broker.prepare(tabId: tabId, config: configuration(origin: "app-a", permissions: permissions))
        let originalId = try document(in: original)
        let write = Task { try await request(broker, tabId, originalId, .setItem, value: "accepted") }
        defer { database.resumeWrite.signal() }
        let timeout = Task {
            try await Task.sleep(for: .seconds(5))
            database.writeStarted.continuation.finish()
        }
        defer { timeout.cancel() }
        var started = database.writeStarted.stream.makeAsyncIterator()
        let event = await started.next()
        try #require(event != nil)

        broker.remove(tabId)
        let replacement = broker.prepare(tabId: tabId, config: configuration(origin: "app-b", permissions: permissions))
        let replacementId = try document(in: replacement)
        database.resumeWrite.signal()
        let reply = try await write.value
        #expect(reply.error == "The local storage document changed.")
        #expect(try database.getItem(origin: "app-a", key: "theme") == "accepted")
        #expect(try database.getItem(origin: "app-b", key: "theme") == nil)
        let denied = try await request(broker, tabId, originalId, .setItem, value: "denied")
        #expect(denied.error == "Local storage access was denied.")
        let current = try await request(broker, tabId, replacementId, .getItem)
        #expect(current.value == nil && current.error == nil)
        #expect(try database.getItem(origin: "app-a", key: "theme") == "accepted")
    }

    @Test func panelChangesPreserveTheLatestStorageConfig() throws {
        let tabId = UUID()
        let coordinator = ViewBridgeCoordinator()
        coordinator.tabId = tabId
        coordinator.allocatedServiceId = "test.storage"
        coordinator.tabConfig = XPCTabConfig(
            tabId: tabId, appId: nil, legacyActorBridge: false, title: "Old", backgroundColor: "#ffffff",
            containerSize: CGSize(width: 800, height: 600), systemAppType: nil,
            unifiedScript: "old", capabilityManifestJSON: nil, capabilitySnapshotJSON: nil,
            leftPanelScript: nil, leftPanelVisible: true, rightPanelVisible: false,
            permissions: [], libraryData: [:], reloadKey: 0, serviceCallUrl: nil, serviceEventsUrl: nil
        )
        let view = RendererSurfaceHostView(frame: .zero)
        view.tabId = tabId
        view.fingerprint = "test.storage"
        coordinator.rendererHostView = view
        defer { view.prepareForReuse() }
        var latest = configuration(origin: "app-a", permissions: ["host:local-storage.getItem"])
        latest["reloadKey"] = 2
        latest["unifiedScript"] = "current"
        latest["leftPanelVisible"] = true
        view.apply(config: latest)

        coordinator.sendLeftPanelVisibilityChanged(false)
        let updated = try #require(view.lastConfig)
        #expect(updated["leftPanelVisible"] as? Bool == false)
        #expect(updated["localStorageOrigin"] as? String == "app-a")
        #expect(updated["reloadKey"] as? Int == 2)
        #expect(updated["unifiedScript"] as? String == "current")
        let broker = RendererLocalStorage(database: GuestLocalStorage(path: ":memory:"))
        _ = try document(in: broker.prepare(tabId: tabId, config: updated))
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

    private func rendererConfig(for tab: Tab) -> [String: Any] {
        let coordinator = ViewBridgeCoordinator()
        coordinator.currentViewportSize = CGSize(width: 800, height: 600)
        let layer = UnifiedReactNativeLayer(
            tab: tab, fingerprint: "test.storage", viewportSize: CGSize(width: 1280, height: 720),
            cardSize: CGSize(width: 300, height: 200), cardPosition: .zero,
            isFocused: true, isOverviewMode: false, isTransitioning: false,
            coordinator: coordinator, libraryData: [:]
        )
        return layer.rendererConfigForResend()
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
          var key = 'theme\\u0000\\u00e9';
          await store.setItem({key: 'theme', value: 'plain'});
          await store.setItem({key: key, value: 'dark\\u0000\\u00e9\\ud83c\\udf32'});
          var theme = await store.getItem({key: key});
          var plain = await store.getItem({key: 'theme'});
          await store.removeItem({key: key});
          var missing = await store.getItem({key: key});
          await store.setItem({key: 'report', value: JSON.stringify([theme, plain, missing])});
        })();
        """
}

private final class PausedLocalStorage: GuestLocalStorage, @unchecked Sendable {
    let writeStarted = AsyncStream<Void>.makeStream()
    let resumeWrite = DispatchSemaphore(value: 0)

    override func setItem(origin: String, key: String, value: String) throws {
        writeStarted.continuation.yield(())
        guard resumeWrite.wait(timeout: .now() + 5) == .success else {
            throw GuestLocalStorageError.DatabaseFailure(message: "The test write was never resumed.")
        }
        try super.setItem(origin: origin, key: key, value: value)
    }
}
