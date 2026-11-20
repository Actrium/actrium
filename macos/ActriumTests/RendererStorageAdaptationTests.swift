import ActriumCore
import ActriumIPC
import Foundation
import Testing
@testable import Actrium

@MainActor
struct RendererStorageAdaptationTests {
    @Test func firstEnrollmentImportsThePreviousProfilesRemovedPackageKey() async throws {
        let suite = "storage-first-enrollment-\(UUID().uuidString)"
        let preferences = try #require(UserDefaults(suiteName: suite))
        defer { preferences.removePersistentDomain(forName: suite) }
        _ = try RealmProfileStore.acceptPublisher(
            body: PublisherStorageFixture.profile(rotated: false), domain: "publisher.example", preferences: preferences
        )
        let core = GuestLocalStorage(path: ":memory:")
        let key = try PublisherStorageFixture.key(3).publicKey.rawRepresentation.base64EncodedString()
        let oldOrigin = "signed:\(key):publisher.example/counter"
        try core.setItem(origin: oldOrigin, key: "theme", value: "dark")
        let database = GuestStorageDatabase(database: core)
        let profile = try await RealmProfileStore.acceptPublisher(
            body: PublisherStorageFixture.profile(rotated: true), domain: "publisher.example",
            preferences: preferences, database: database
        )
        let current = try PublisherStorageFixture.key(4).publicKey.rawRepresentation.base64EncodedString()
        let currentOrigin = "signed:\(current):publisher.example/counter"
        let identity = try #require(profile.storageIdentity)
        let token = try #require(try await database.migrationToken(currentOrigin, publisherIdentity: identity))
        let canonical = try await database.resolveSignedOrigin(
            currentOrigin, publisherIdentity: identity, migrationApproval: token
        )
        #expect(try core.getItem(origin: canonical, key: "theme") == "dark")
        #expect(try core.getItem(origin: oldOrigin, key: "theme") == "dark")
    }

    @Test func publisherEnrollmentUsesTheExistingHostPinsAndKeepsItsCurrentSnapshot() async throws {
        let suite = "storage-adaptation-\(UUID().uuidString)"
        let preferences = try #require(UserDefaults(suiteName: suite))
        defer { preferences.removePersistentDomain(forName: suite) }
        let body = try PublisherStorageFixture.profile(rotated: false)
        _ = try RealmProfileStore.acceptPublisher(body: body, domain: "publisher.example", preferences: preferences)
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        do {
            _ = try await RealmProfileStore.acceptPublisher(
                body: PublisherStorageFixture.profile(rotated: true, chained: false),
                domain: "publisher.example", preferences: preferences, database: database
            )
            Issue.record("Storage enrollment discarded existing Host trust.")
        } catch GuestLocalStorageError.IdentityChanged { }
        let profile = try await RealmProfileStore.acceptPublisher(
            body: body, domain: "publisher.example", preferences: preferences, database: database
        )
        #expect(profile.storageIdentity != nil)
        let key = try #require(profile.packageSigningKeys.first)
        try RealmProfileStore.requirePublisherKey(
            try #require(Data(base64Encoded: key.publicKeyBase64)),
            domain: profile.domain, preferences: preferences
        )
    }

    @Test func corruptHostTrustIsNotTreatedAsFirstUse() async throws {
        let suite = "storage-corrupt-\(UUID().uuidString)"
        let preferences = try #require(UserDefaults(suiteName: suite))
        defer { preferences.removePersistentDomain(forName: suite) }
        preferences.set(["body": "broken"], forKey: "actr.acceptedPublisherProfile.publisher.example")
        do {
            _ = try await RealmProfileStore.acceptPublisher(
                body: PublisherStorageFixture.profile(rotated: false), domain: "publisher.example",
                preferences: preferences, database: GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
            )
            Issue.record("Corrupt Host trust was silently reset.")
        } catch CoreError.AuthorityDescriptorInvalid { }
    }

    @Test func publisherEnrollmentPreservesTheDurableTrustBaselineAfterPreferencesAreCleared() async throws {
        let suite = "storage-durable-trust-\(UUID().uuidString)"
        let preferences = try #require(UserDefaults(suiteName: suite))
        defer { preferences.removePersistentDomain(forName: suite) }
        let body = try PublisherStorageFixture.profile(rotated: false)
        let pins = try ActriumCore.acceptRealmPackagePublisher(
            body: body, domain: "publisher.example", previousPinsJson: nil
        ).pinsJson
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        do {
            _ = try await RealmProfileStore.acceptPublisher(
                body: PublisherStorageFixture.profile(rotated: true, chained: false),
                domain: "publisher.example", preferences: preferences, database: database, fallbackPins: pins
            )
            Issue.record("Storage enrollment discarded durable Publisher trust.")
        } catch GuestLocalStorageError.IdentityChanged { }
        let profile = try await RealmProfileStore.acceptPublisher(
            body: PublisherStorageFixture.profile(rotated: true), domain: "publisher.example",
            preferences: preferences, database: database, fallbackPins: pins
        )
        #expect(profile.storageIdentity != nil)
    }

    @Test func expiryDuringIdentityValidationStillPreventsTheMutation() async throws {
        let store = ValidationStorage(path: ":memory:")
        let broker = RendererLocalStorage(database: store)
        let tab = UUID()
        let config = broker.prepare(tabId: tab, config: [
            "localStorageOrigin": "slow", "permissions": ["host:local-storage.setItem"],
            "capabilityManifestJSON": #"{"hostModules":{"local-storage":{"rpc":["setItem"]}}}"#,
        ])
        let document = try #require((config["localStorageDocumentId"] as? String).flatMap(UUID.init(uuidString:)))
        let request = GuestLocalStorageCall(
            id: UUID(), documentId: document, operation: .setItem, key: "key", value: "changed",
            deadlineUptimeNanoseconds: (DispatchTime.now() + .milliseconds(250)).uptimeNanoseconds
        )
        let task = Task { try await broker.reply(to: String(decoding: JSONEncoder().encode(request), as: UTF8.self), tabId: tab) }
        defer { store.resume.signal() }
        var iterator = store.started.stream.makeAsyncIterator()
        let timeout = Task { try await Task.sleep(for: .seconds(5)); store.started.continuation.finish() }
        defer { timeout.cancel() }
        try #require(await iterator.next() != nil)
        try await Task.sleep(for: .milliseconds(300))
        store.resume.signal()
        let reply = try JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(try #require(try await task.value).utf8))
        #expect(reply.errorCode == .timeout)
        #expect(try store.getItem(origin: "slow", key: "key") == nil)
        #expect(broker.pendingOperationCount == 0)
    }
}

private final class ValidationStorage: GuestLocalStorage, @unchecked Sendable {
    let started = AsyncStream<Void>.makeStream()
    let resume = DispatchSemaphore(value: 0)

    override func resolveSignedOrigin(origin: String, publisherIdentity: String?, migrationApproval: String?) throws -> String {
        started.continuation.yield(())
        guard resume.wait(timeout: .now() + 5) == .success else {
            throw GuestLocalStorageError.DatabaseFailure(message: "Validation was not resumed.")
        }
        return try super.resolveSignedOrigin(origin: origin, publisherIdentity: publisherIdentity, migrationApproval: migrationApproval)
    }
}
