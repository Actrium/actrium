import ActriumCore
import AppKit
import CryptoKit
import Foundation
import Testing
@testable import Actrium

@MainActor
struct RendererStorageLaunchTests {
    @Test(arguments: [false, true])
    func independentGuestStartsWhileTheSharedDatabaseQueueIsFull(signed: Bool) async throws {
        _ = NSApplication.shared
        let key = try PublisherStorageFixture.key(3).publicKey.rawRepresentation.base64EncodedString()
        let origin = signed ? "signed:\(key):publisher.example/counter" : "dev:/tmp/storage-launch-guest"
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"), pendingLimit: 1)
        let started = AsyncStream<Void>.makeStream()
        let resume = DispatchSemaphore(value: 0)
        let blocker = Task {
            try await database.run { _ in
                started.continuation.yield(())
                guard resume.wait(timeout: .now() + 15) == .success else {
                    throw GuestLocalStorageError.DatabaseFailure(message: "The test did not release its queue blocker.")
                }
            }
        }
        defer { resume.signal() }
        let timeout = Task { try await Task.sleep(for: .seconds(5)); started.continuation.finish() }
        defer { timeout.cancel() }
        var events = started.stream.makeAsyncIterator()
        try #require(await events.next() != nil)
        let broker = RendererLocalStorage(access: database)
        let pool = RendererProcessPool(localStorage: broker)
        defer { pool.shutdown() }
        let tabId = UUID()
        _ = try pool.allocateRenderer(for: tabId, fingerprint: "local-storage-start-\(tabId)")
        _ = try await pool.prepareTab(tabId: tabId, config: [
            "reloadKey": 1, "localStorageOrigin": origin,
            "permissions": ["host:local-storage.getItem"],
            "capabilityManifestJSON": #"{"hostModules":{"local-storage":{"rpc":["getItem"]}}}"#,
        ])
        #expect(database.pendingOperationCount == 1)
        resume.signal()
        try await blocker.value
        #expect(database.pendingOperationCount == 0)
    }

    @Test func storageOnlyGuestCanCopyLegacyDataWithoutFileImportPermission() async throws {
        _ = NSApplication.shared
        let core = GuestLocalStorage(path: ":memory:")
        let database = GuestStorageDatabase(database: core)
        let accepted = try await database.acceptPublisherProfile(
            body: PublisherStorageFixture.profile(rotated: false), domain: "publisher.example"
        )
        let identity = try #require(accepted.profile.storageIdentity)
        let publicKey = try PublisherStorageFixture.key(3).publicKey.rawRepresentation.base64EncodedString()
        let origin = "signed:\(publicKey):publisher.example/counter"
        try core.setItem(origin: origin, key: "theme", value: "dark")
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 200),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        var prompts = 0
        let consent = StorageMigrationConsent(present: { _, presentedWindow in
            #expect(presentedWindow === window)
            prompts += 1
            return true
        })
        let pool = RendererProcessPool(localStorage: RendererLocalStorage(access: database, migrationConsent: consent))
        let tabId = UUID()
        var fileImportLookups = 0
        pool.documentWindow = { requestedTab, reloadKey in
            #expect(requestedTab == tabId && reloadKey == 1)
            return window
        }
        pool.fileImportWindow = { _, _ in fileImportLookups += 1; return nil }
        defer { pool.shutdown(); window.close() }
        _ = try pool.allocateRenderer(for: tabId, fingerprint: "storage-launch-\(tabId)")
        _ = try await pool.prepareTab(tabId: tabId, config: [
            "reloadKey": 1, "localStorageOrigin": origin, "localStoragePublisherIdentity": identity,
            "permissions": ["host:local-storage.getItem"],
            "capabilityManifestJSON": #"{"hostModules":{"local-storage":{"rpc":["getItem"]}}}"#,
        ])
        #expect(prompts == 1 && fileImportLookups == 0)
        let canonical = try await database.resolveSignedOrigin(origin, publisherIdentity: identity)
        #expect(try core.getItem(origin: canonical, key: "theme") == "dark")
        #expect(try core.getItem(origin: origin, key: "theme") == "dark")
    }
}
