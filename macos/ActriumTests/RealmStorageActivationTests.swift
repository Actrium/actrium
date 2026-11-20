import ActriumCore
import AppKit
import CryptoKit
import Foundation
import Testing
@testable import Actrium

@MainActor
struct RealmStorageActivationTests {
    @Test func finalKeyCheckRejectsAConcurrentDatabaseRevocationWithStalePreferences() async throws {
        let suite = "activation-revocation-\(UUID().uuidString)"
        let preferences = try #require(UserDefaults(suiteName: suite))
        defer { preferences.removePersistentDomain(forName: suite) }
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        let initial = try PublisherStorageFixture.profile(rotated: false)
        _ = try await RealmProfileStore.acceptPublisher(body: initial, domain: "publisher.example", preferences: preferences, database: database)
        let started = AsyncStream<Void>.makeStream()
        let resume = AsyncStream<Void>.makeStream()
        let pendingSwitch = Task { @MainActor in
            started.continuation.yield(())
            for await _ in resume.stream { break }
            try await RealmProfileStore.requirePublisherKey(
                PublisherStorageFixture.key(3).publicKey.rawRepresentation,
                domain: "publisher.example", database: database
            )
        }
        var events = started.stream.makeAsyncIterator()
        _ = await events.next()
        _ = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: true), domain: "publisher.example")
        resume.continuation.yield(())
        resume.continuation.finish()
        guard case .failure = await pendingSwitch.result else { Issue.record("The final switch check accepted a revoked key."); return }
        #expect(preferences.dictionary(forKey: "actr.acceptedPublisherProfile.publisher.example")?["body"] as? String == initial)
    }

    @Test(arguments: [false, true])
    func failedStoragePreparationCanBeRetriedBeforeActivation(quota: Bool) async throws {
        _ = NSApplication.shared
        let core = GuestLocalStorage(path: ":memory:")
        let database = GuestStorageDatabase(database: core)
        _ = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: false), domain: "publisher.example")
        let accepted = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: true), domain: "publisher.example")
        let identity = try #require(accepted.profile.storageIdentity)
        let old = "signed:\(try PublisherStorageFixture.key(3).publicKey.rawRepresentation.base64EncodedString()):publisher.example/counter"
        let current = "signed:\(try PublisherStorageFixture.key(4).publicKey.rawRepresentation.base64EncodedString()):publisher.example/counter"
        for index in 0..<(quota ? 9 : 1) {
            try core.setItem(origin: old, key: "old-\(index)", value: quota ? String(repeating: "x", count: 65536) : "dark")
            try core.setItem(origin: current, key: quota ? "new-\(index)" : "old-0", value: quota ? String(repeating: "x", count: 65536) : "light")
        }
        let window = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let consent = StorageMigrationConsent(present: { _, _ in true }, reportFailure: { _ in })
        let broker = RendererLocalStorage(access: database, migrationConsent: consent)
        let config: [String: Any] = ["localStorageOrigin": current, "localStoragePublisherIdentity": identity,
            "permissions": ["host:local-storage.getItem"],
            "capabilityManifestJSON": #"{"hostModules":{"local-storage":{"rpc":["getItem"]}}}"#]
        do {
            _ = try await broker.prepareForActivation(config: config, window: window)
            Issue.record("Storage preparation unexpectedly succeeded.")
        } catch GuestLocalStorageError.MigrationConflict { #expect(!quota) }
          catch GuestLocalStorageError.QuotaExceeded { #expect(quota) }
        for index in 0..<(quota ? 9 : 1) { try core.removeItem(origin: current, key: quota ? "new-\(index)" : "old-0") }
        let prepared = try #require(try await broker.prepareForActivation(config: config, window: window))
        let saturated = RendererLocalStorage(database: core, pendingLimit: 0)
        var ready = config
        ready["preparedLocalStorage"] = prepared.configuration
        #expect(JSONSerialization.isValidJSONObject(ready))
        let resolved = try await saturated.resolveOrigin(config: ready)
        #expect(resolved["localStorageOrigin"] as? String == prepared.origin)
        #expect(try core.getItem(origin: prepared.origin, key: "old-0") != nil)
        #expect(JSONSerialization.isValidJSONObject(saturated.prepare(tabId: UUID(), config: resolved)))
        ready["localStoragePublisherIdentity"] = "another-publisher"
        do { _ = try await saturated.resolveOrigin(config: ready); Issue.record("A preparation crossed Publisher identities.") }
        catch GuestStorageDatabaseError.busy { }
    }
}
