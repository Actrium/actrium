import ActriumCore
import Foundation
import Testing
@testable import Actrium

@MainActor
struct PublisherStorageRecoveryTests {
    @Test func aDatabaseCommitBeforePreferenceMirroringRecoversAfterRestart() async throws {
        let suite = "publisher-recovery-\(UUID().uuidString)"
        let preferences = try #require(UserDefaults(suiteName: suite))
        defer { preferences.removePersistentDomain(forName: suite) }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("storage.sqlite").path
        let initial = try PublisherStorageFixture.profile(rotated: false)
        _ = try RealmProfileStore.acceptPublisher(body: initial, domain: "publisher.example", preferences: preferences)
        let identity: String
        let canonical: String
        do {
            let core = GuestLocalStorage(path: path)
            let database = GuestStorageDatabase(database: core)
            let accepted = try await database.acceptPublisherProfile(body: initial, domain: "publisher.example")
            identity = try #require(accepted.profile.storageIdentity)
            let key = try PublisherStorageFixture.key(3).publicKey.rawRepresentation.base64EncodedString()
            canonical = try await database.resolveSignedOrigin("signed:\(key):publisher.example/counter", publisherIdentity: identity)
            try core.setItem(origin: canonical, key: "theme", value: "dark")
            _ = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: true), domain: "publisher.example")
        }
        // Reopening models a crash after Core committed but before preferences were updated.
        let reopened = GuestLocalStorage(path: path)
        let body = try PublisherStorageFixture.profileAfterRemovingOldProfileKey()
        let recovered = try await RealmProfileStore.acceptPublisher(
            body: body, domain: "publisher.example", preferences: preferences,
            database: GuestStorageDatabase(database: reopened)
        )
        #expect(recovered.storageIdentity == identity)
        #expect(try reopened.getItem(origin: canonical, key: "theme") == "dark")
        #expect(preferences.dictionary(forKey: "actr.acceptedPublisherProfile.publisher.example")?["body"] as? String == body)
        let key = try PublisherStorageFixture.key(4).publicKey.rawRepresentation
        try RealmProfileStore.requirePublisherKey(key, domain: "publisher.example", preferences: preferences)
    }

    @Test func registeredDatabaseTrustRepairsAnUnreadablePreferenceCache() async throws {
        let suite = "publisher-cache-repair-\(UUID().uuidString)"
        let preferences = try #require(UserDefaults(suiteName: suite))
        defer { preferences.removePersistentDomain(forName: suite) }
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        let initial = try await database.acceptPublisherProfile(
            body: PublisherStorageFixture.profile(rotated: true), domain: "publisher.example"
        )
        let cacheKey = "actr.acceptedPublisherProfile.publisher.example"
        preferences.set(["pins": ["unreadable": true], "body": 42], forKey: cacheKey)
        let body = try PublisherStorageFixture.profileAfterRemovingOldProfileKey()
        let accepted = try await RealmProfileStore.acceptPublisher(
            body: body, domain: "publisher.example", preferences: preferences, database: database
        )
        #expect(accepted.storageIdentity == initial.profile.storageIdentity)
        #expect(preferences.dictionary(forKey: cacheKey)?["body"] as? String == body)
        #expect(preferences.dictionary(forKey: cacheKey)?["pins"] is String)
    }
}
