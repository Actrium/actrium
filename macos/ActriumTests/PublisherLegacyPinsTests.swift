import ActriumCore
import Foundation
import Testing
@testable import Actrium

@MainActor
struct PublisherLegacyPinsTests {
    @Test func registeredPublisherDoesNotReadAnInvalidLegacyFile() async throws {
        let fixture = try LegacyFixture()
        defer { fixture.remove() }
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        let initial = try await database.acceptPublisherProfile(body: fixture.body, domain: fixture.domain)
        try fixture.writeInvalidFile()
        let accepted = try await RealmProfileStore.acceptPublisher(
            body: fixture.body, domain: fixture.domain, preferences: fixture.preferences,
            database: database, fallbackPins: { try fixture.read() }
        )
        #expect(accepted.storageIdentity == initial.profile.storageIdentity)
        #expect(fixture.reads == 0)
    }

    @Test func firstEnrollmentStillFailsWhenRequiredLegacyEvidenceCannotBeRead() async throws {
        let fixture = try LegacyFixture()
        defer { fixture.remove() }
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        try fixture.writeInvalidFile()
        do {
            _ = try await RealmProfileStore.acceptPublisher(
                body: fixture.body, domain: fixture.domain, preferences: fixture.preferences,
                database: database, fallbackPins: { try fixture.read() }
            )
            Issue.record("First enrollment ignored unreadable legacy trust.")
        } catch { #expect(fixture.reads == 1) }
        #expect(try await database.acceptedPublisherPins(domain: fixture.domain) == nil)
        #expect(fixture.preferences.object(forKey: "actr.acceptedPublisherProfile.publisher.example") == nil)
    }

    @Test func existingPreferencePinsDoNotNeedTheLegacyFile() async throws {
        let fixture = try LegacyFixture()
        defer { fixture.remove() }
        _ = try RealmProfileStore.acceptPublisher(body: fixture.body, domain: fixture.domain, preferences: fixture.preferences)
        try fixture.writeInvalidFile()
        let accepted = try await RealmProfileStore.acceptPublisher(
            body: fixture.body, domain: fixture.domain, preferences: fixture.preferences,
            database: GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:")),
            fallbackPins: { try fixture.read() }
        )
        #expect(accepted.storageIdentity != nil)
        #expect(fixture.reads == 0)
    }

    @Test func firstEnrollmentUsesValidLegacyPinsWhenNeeded() async throws {
        let fixture = try LegacyFixture()
        defer { fixture.remove() }
        let pins = try ActriumCore.acceptRealmPackagePublisher(body: fixture.body, domain: fixture.domain, previousPinsJson: nil).pinsJson
        try RealmFrontTrustStore.save(pins, .publisher, name: fixture.domain, directory: fixture.directory)
        let accepted = try await RealmProfileStore.acceptPublisher(
            body: fixture.body, domain: fixture.domain, preferences: fixture.preferences,
            database: GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:")), fallbackPins: { try fixture.read() }
        )
        #expect(accepted.storageIdentity != nil)
        #expect(fixture.reads == 1)
    }
}

@MainActor
private final class LegacyFixture {
    let domain = "publisher.example"
    let suite = "legacy-publisher-pins-\(UUID().uuidString)"
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let preferences: UserDefaults
    let body: String
    var reads = 0

    init() throws {
        preferences = try #require(UserDefaults(suiteName: suite))
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("Fixtures/publisher-rotation.json")
        let data = try #require(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        body = try #require(data["publisher"] as? String)
    }

    func writeInvalidFile() throws {
        try RealmFrontTrustStore.save("placeholder", .publisher, name: domain, directory: directory)
        let files = try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
        try Data([0xff]).write(to: #require(files.first))
    }

    func read() throws -> String? {
        reads += 1
        return try RealmFrontTrustStore.read(.publisher, name: domain, directory: directory)
    }

    func remove() {
        preferences.removePersistentDomain(forName: suite)
        try? FileManager.default.removeItem(at: directory)
    }
}
