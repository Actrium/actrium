import ActriumCore
import ActriumIPC
import CryptoKit
import Foundation
import Testing
@testable import Actrium

@MainActor
struct PublisherStorageIdentityTests {
    @Test func nativeDiscoveryAndRestoreKeepDataButRevokeTheOldGuestDocument() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let path = folder.appendingPathComponent("storage.sqlite").path
        let first = GuestStorageDatabase(database: GuestLocalStorage(path: path))
        let accepted = try await first.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: false), domain: "publisher.example")
        let identity = try #require(accepted.profile.storageIdentity)
        let broker = RendererLocalStorage(access: first)
        let tab = UUID()
        let original = try await broker.resolveOrigin(config: configuration(seed: 3, publisherIdentity: identity))
        let initial = try document(broker.prepare(tabId: tab, config: original))
        let write = try await call(broker, tab, initial, .setItem, value: "dark")
        #expect(write.error == nil)

        let rotatedBody = try PublisherStorageFixture.profile(rotated: true)
        _ = try await first.acceptPublisherProfile(body: rotatedBody, domain: "publisher.example")
        #expect(try await call(broker, tab, initial, .getItem).error == "Local storage access was denied.")
        let rotated = try await broker.resolveOrigin(config: configuration(seed: 4, publisherIdentity: identity))
        #expect(rotated["localStorageOrigin"] as? String == original["localStorageOrigin"] as? String)
        let next = try document(broker.prepare(tabId: tab, config: rotated))
        #expect(next != initial)
        #expect(try await call(broker, tab, initial, .getItem).error != nil)
        #expect(try await call(broker, tab, next, .getItem).value == "dark")

        let restored = GuestStorageDatabase(database: GuestLocalStorage(path: path))
        let restoredProfile = try await restored.acceptPublisherProfile(body: rotatedBody, domain: "publisher.example")
        #expect(restoredProfile.profile.storageIdentity == identity)
        let restoredBroker = RendererLocalStorage(access: restored)
        let restoredConfig = try await restoredBroker.resolveOrigin(config: configuration(seed: 4, publisherIdentity: identity))
        let restoredId = try document(restoredBroker.prepare(tabId: tab, config: restoredConfig))
        #expect(try await call(restoredBroker, tab, restoredId, .getItem).value == "dark")
    }

    @Test func nativeEntryRejectsUnchainedIdentityAndKeepsIndependentLocalLoadsSeparate() async throws {
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        let accepted = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: false), domain: "publisher.example")
        let identity = try #require(accepted.profile.storageIdentity)
        let broker = RendererLocalStorage(access: database)
        let original = try await broker.resolveOrigin(config: configuration(seed: 3, publisherIdentity: identity))
        let originalTab = UUID()
        let originalId = try document(broker.prepare(tabId: originalTab, config: original))
        _ = try await call(broker, originalTab, originalId, .setItem, value: "private")
        do {
            _ = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: true, chained: false), domain: "publisher.example")
            Issue.record("An unchained Publisher identity was accepted.")
        } catch GuestLocalStorageError.IdentityChanged { }
        let independent = try await broker.resolveOrigin(config: configuration(seed: 3))
        #expect(independent["localStorageOrigin"] as? String != original["localStorageOrigin"] as? String)
        let otherTab = UUID()
        let otherId = try document(broker.prepare(tabId: otherTab, config: independent))
        #expect(try await call(broker, otherTab, otherId, .getItem).value == nil)
        _ = try await call(broker, otherTab, otherId, .setItem, value: "local-only")
        #expect(try await call(broker, originalTab, originalId, .getItem).value == "private")
        let unchanged = try await broker.resolveOrigin(config: configuration(seed: 3, publisherIdentity: identity))
        #expect(unchanged["localStorageOrigin"] as? String == original["localStorageOrigin"] as? String)
    }

    private func configuration(seed: UInt8, publisherIdentity: String? = nil) throws -> [String: Any] {
        var config: [String: Any] = [
            "localStorageOrigin": "signed:\(try PublisherStorageFixture.key(seed).publicKey.rawRepresentation.base64EncodedString()):publisher.example/counter",
            "permissions": ["getItem", "setItem", "removeItem"].map { "host:local-storage.\($0)" },
            "capabilityManifestJSON": #"{"hostModules":{"local-storage":{"rpc":["getItem","setItem","removeItem"]}}}"#,
        ]
        config["localStoragePublisherIdentity"] = publisherIdentity
        return config
    }

    @Test func overlappingActiveKeysSelectTheActualArchiveSignerAndRejectChangedBytes() async throws {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("Fixtures/publisher-rotation.json")
        let fixture = try JSONDecoder().decode(PackageFixture.self, from: Data(contentsOf: url))
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        let acceptance = try await database.acceptPublisherProfile(body: fixture.publisher, domain: "publisher.example")
        let publisher = acceptance.profile
        #expect(publisher.packageSigningKeys.count == 2)
        let profile = try ActriumCore.acceptRealmProfile(
            body: fixture.realm_profile, realmName: fixture.realm_name, signingKeysJson: fixture.realm_keys
        )
        let keys = publisher.fixedArchiveVerificationKeys
        #expect(keys.first != fixture.signer)
        let signer = keys.first {
            (try? ActriumCore.verifyRealmFrontPackage(archive: fixture.archive, trustedPublicKey: $0, profile: profile)) != nil
        }
        #expect(signer == fixture.signer)
        #expect(keys.allSatisfy {
            (try? ActriumCore.verifyRealmFrontPackage(archive: fixture.archive + Data([0]), trustedPublicKey: $0, profile: profile)) == nil
        })
    }

    private func document(_ config: [String: Any]) throws -> UUID {
        #expect(config["localStorageOrigin"] == nil && config["localStoragePrincipal"] == nil && config["localStoragePublisherIdentity"] == nil)
        return try #require((config["localStorageDocumentId"] as? String).flatMap(UUID.init(uuidString:)))
    }

    private func call(
        _ broker: RendererLocalStorage, _ tab: UUID, _ document: UUID,
        _ operation: GuestLocalStorageCall.Operation, value: String? = nil
    ) async throws -> GuestLocalStorageReply {
        let input = GuestLocalStorageCall(id: UUID(), documentId: document, operation: operation, key: "theme", value: value)
        let json = String(decoding: try JSONEncoder().encode(input), as: UTF8.self)
        let output = try #require(try await broker.reply(to: json, tabId: tab))
        return try JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(output.utf8))
    }
}

private struct PackageFixture: Decodable {
    let archive: Data
    let realm_name: String
    let realm_profile: String
    let realm_keys: String
    let publisher: String
    let signer: Data
}

enum PublisherStorageFixture {
    static func key(_ seed: UInt8) throws -> Curve25519.Signing.PrivateKey {
        try Curve25519.Signing.PrivateKey(rawRepresentation: Data(repeating: seed, count: 32))
    }

    static func payload(_ value: [String: Any]) throws -> Data {
        try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .withoutEscapingSlashes])
    }

    static func entry(_ seed: UInt8, id: String, status: String = "active") throws -> [String: Any] {
        ["id": id, "algorithm": "Ed25519", "public_key": try key(seed).publicKey.rawRepresentation.base64EncodedString(), "status": status]
    }

    static func profile(rotated: Bool, chained: Bool = true) throws -> String {
        let root: UInt8 = rotated ? 2 : 1
        let package: UInt8 = rotated ? 4 : 3
        let bytes = try key(package).publicKey.rawRepresentation
        let id = "actr-pub-" + SHA256.hash(data: bytes).prefix(8).map { String(format: "%02x", $0) }.joined()
        var roots = [try entry(root, id: "root-\(root)")]
        var rotations: [[String: Any]] = []
        if rotated && chained {
            roots.append(try entry(1, id: "root-1", status: "retired"))
            let proof: [String: Any] = ["from": "root-1", "to": "root-2", "sequence": 2, "public_key": try key(2).publicKey.rawRepresentation.base64EncodedString()]
            rotations = [["from": "root-1", "to": "root-2", "sequence": 2, "signature": try key(1).signature(for: payload(proof)).base64EncodedString()]]
        }
        var profile: [String: Any] = [
            "schema": "actr.publisher/v1", "domain": "publisher.example", "sequence": rotated ? 2 : 1,
            "profile_issued_at": "2020-01-01T00:00:00Z", "profile_expires_at": "2100-01-01T00:00:00Z",
            "profile_signing_keys": roots, "keys": [try entry(package, id: id)], "rotations": rotations,
        ]
        profile["signature"] = ["key_id": "root-\(root)", "algorithm": "Ed25519", "value": try key(root).signature(for: payload(profile)).base64EncodedString()]
        return String(decoding: try payload(["version": 1, "authority": "actr://publisher.example", "services": [["rel": "contracts", "href": "https://publisher.example/contracts"]], "publisher": profile]), as: UTF8.self)
    }
}
