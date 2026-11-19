import ActriumCore
import CryptoKit
import Foundation
import XCTest

@testable import Actrium

final class RealmAdmissionTests: XCTestCase {
    private let realmName = "echo@realm.example"

    @MainActor
    func testSignedOpenPolicyAllowsInviteFreeAdmission() throws {
        let (body, keys) = try profile(membership: "open")
        let verified = try RealmJoinService.VerifiedProfile(
            body: body, realmName: realmName, signingKeysJSON: keys, preferences: try makePreferences()
        )
        XCTAssertTrue(verified.isOpen)
        XCTAssertEqual(verified.info.realmName, realmName)
    }

    @MainActor
    func testInviteOnlyPolicyCannotUseOpenJoin() async throws {
        let (body, keys) = try profile(membership: "invite-only")
        let verified = try RealmJoinService.VerifiedProfile(
            body: body, realmName: realmName, signingKeysJSON: keys, preferences: try makePreferences()
        )
        XCTAssertFalse(verified.isOpen)
        do {
            try await RealmJoinService.joinOpen(verified, label: "Test")
            XCTFail("An invite-only Realm accepted an open join.")
        } catch {
            XCTAssertEqual((error as NSError).domain, "ActrJoin")
        }
    }

    @MainActor
    func testUnsignedPolicyChangeCannotOpenARealm() throws {
        let (body, keys) = try profile(membership: "invite-only")
        var document = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        document["policy"] = ["membership": "open"]
        let changed = try JSONSerialization.data(withJSONObject: document)
        let preferences = try makePreferences()
        XCTAssertThrowsError(try RealmJoinService.VerifiedProfile(
            body: changed, realmName: realmName, signingKeysJSON: keys, preferences: preferences
        ))
    }

    @MainActor
    func testRealmBaselineAdvancesBeforeFrontInstallation() throws {
        let preferences = try makePreferences()
        let signer = Curve25519.Signing.PrivateKey()
        let original = try profile(membership: "open", signer: signer)
        let first = try accept(original, preferences: preferences)
        let sameSequence = try profile(membership: "invite-only", signer: signer)
        XCTAssertThrowsError(try accept(sameSequence, preferences: preferences))
        XCTAssertTrue(RealmProfileStore.isCurrent(first, preferences: preferences))

        let update = try profile(membership: "invite-only", sequence: 2, signer: signer)
        let next = try accept(update, preferences: preferences)
        // A failed package load after acceptance leaves this newer baseline in place.
        XCTAssertEqual(next.sequence, 2)
        XCTAssertThrowsError(try accept(original, preferences: preferences))
        XCTAssertTrue(RealmProfileStore.isCurrent(next, preferences: preferences))
    }

    @MainActor
    func testOnlyAcceptedRetirementClearsRunningFronts() async throws {
        let preferences = try makePreferences()
        let signer = Curve25519.Signing.PrivateKey()
        let active = try profile(membership: "open", sequence: 2, signer: signer)
        _ = try accept(active, preferences: preferences)
        let windows = (0..<2).map { _ in
            BrowserViewModel(realmPreferences: preferences, saveBrowserSession: { _, _ in })
        }
        for window in windows {
            window.tabs[0].url = URL(string: "actr://realm.example/echo")
            window.tabs[0].appId = "echo-front"
            window.tabs[0].guestScript = "active guest"
        }

        let stale = try profile(membership: "open", status: "retired", signer: signer)
        XCTAssertThrowsError(try accept(stale, preferences: preferences))
        XCTAssertTrue(windows.allSatisfy { $0.currentTab?.guestScript == "active guest" })

        let retired = try profile(membership: "open", sequence: 3, status: "retired", signer: signer)
        XCTAssertThrowsError(try accept(retired, preferences: preferences)) { error in
            guard case CoreError.RealmRetired(let name) = error else {
                return XCTFail("Expected accepted retirement")
            }
            XCTAssertEqual(name, self.realmName)
        }
        XCTAssertTrue(windows.allSatisfy { $0.currentTab?.appId == nil && $0.currentTab?.guestScript == nil })
        let reopened = try profile(membership: "open", sequence: 4, signer: signer)
        XCTAssertThrowsError(try accept(reopened, preferences: preferences))
    }

    @MainActor
    func testPublisherRotationKeepsDigestPinnedFrontsAndRejectsRollback() throws {
        let preferences = try makePreferences()
        let root = Curve25519.Signing.PrivateKey()
        let firstKey = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
        let nextKey = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
        let previous = packageKey(firstKey, status: "active")
        let next = packageKey(nextKey, status: "active")
        let first = try publisherDescriptor(sequence: 1, root: root, keys: [previous])
        let pinned = LocalRealmFront(signingKeyId: try XCTUnwrap(previous["id"]), allowRebuild: false)
        let rebuildable = LocalRealmFront(signingKeyId: pinned.signingKeyId, allowRebuild: true)

        func acceptPublisher(_ body: String) throws -> PublisherProfileInfo {
            try RealmProfileStore.acceptPublisher(body: body, domain: "publisher.example", preferences: preferences)
        }

        XCTAssertEqual(try acceptPublisher(first).verificationKey(for: rebuildable), firstKey)
        let rotated = try publisherDescriptor(
            sequence: 2, root: root, keys: [packageKey(firstKey, status: "retired"), next]
        )
        let accepted = try acceptPublisher(rotated)
        XCTAssertEqual(accepted.profileSequence, 2)
        XCTAssertEqual(accepted.verificationKey(for: pinned), firstKey)
        XCTAssertNil(accepted.verificationKey(for: rebuildable))
        XCTAssertThrowsError(try acceptPublisher(first))

        let removed = try publisherDescriptor(sequence: 3, root: root, keys: [next])
        let latest = try acceptPublisher(removed)
        XCTAssertNil(latest.verificationKey(for: pinned))
        XCTAssertNil(latest.verificationKey(for: rebuildable))
    }

    @MainActor
    private func accept(_ document: (Data, String), preferences: UserDefaults) throws -> RealmProfileInfo {
        try RealmProfileStore.acceptRealm(
            body: String(decoding: document.0, as: UTF8.self), realmName: realmName,
            signingKeysJSON: document.1, preferences: preferences
        )
    }

    private func makePreferences() throws -> UserDefaults {
        let suite = "ActriumTests.RealmAdmission.\(UUID())"
        let preferences = try XCTUnwrap(UserDefaults(suiteName: suite))
        addTeardownBlock { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        return preferences
    }

    private func packageKey(_ publicKey: Data, status: String) -> [String: String] {
        let digest = SHA256.hash(data: publicKey).prefix(8).map { String(format: "%02x", $0) }.joined()
        return [
            "id": "actr-pub-" + digest, "status": status, "algorithm": "Ed25519",
            "public_key": publicKey.base64EncodedString()
        ]
    }

    private func publisherDescriptor(
        sequence: Int, root: Curve25519.Signing.PrivateKey, keys: [[String: String]]
    ) throws -> String {
        let now = Date()
        let formatter = ISO8601DateFormatter()
        let profile = try sign([
            "schema": "actr.publisher/v1",
            "domain": "publisher.example",
            "sequence": sequence,
            "profile_signing_keys": [[
                "id": "realm-key", "algorithm": "Ed25519", "status": "active",
                "public_key": root.publicKey.rawRepresentation.base64EncodedString()
            ]],
            "keys": keys,
            "profile_issued_at": formatter.string(from: now.addingTimeInterval(-60)),
            "profile_expires_at": formatter.string(from: now.addingTimeInterval(3600))
        ], with: root)
        let descriptor = try JSONSerialization.data(withJSONObject: [
            "version": 1, "authority": "actr://publisher.example",
            "services": [["rel": "contracts", "href": "https://publisher.example/contracts"]],
            "publisher": try JSONSerialization.jsonObject(with: profile)
        ])
        return String(decoding: descriptor, as: UTF8.self)
    }

    private func profile(
        membership: String, sequence: Int = 1, status: String = "active",
        signer: Curve25519.Signing.PrivateKey = Curve25519.Signing.PrivateKey()
    ) throws -> (Data, String) {
        let publicKey = signer.publicKey.rawRepresentation.base64EncodedString()
        let keys = try sign([
            "sequence": 1,
            "keys": [["id": "realm-key", "algorithm": "Ed25519", "public_key": publicKey, "status": "active"]]
        ], with: signer)
        let body = try sign([
            "schema": "actr.realm/v1",
            "sequence": sequence,
            "realm": ["name": realmName, "display_name": "Echo", "status": status],
            "primary_package": "service",
            "front_package": "front",
            "packages": [
                "service": archive("echo", digest: "a"),
                "front": archive("echo-front", digest: "b")
            ],
            "issuer": [
                "url": "https://issuer.example",
                "keys": [["id": "7", "algorithm": "Ed25519", "public_key": publicKey, "status": "active"]]
            ],
            "provider": [
                "id": "provider.example",
                "signaling": [[
                    "url": "wss://signal.provider.example/ws",
                    "priority": 10,
                    "control_plane_id": "provider.example/v1",
                    "ticket_audience": "signal"
                ]],
                "ice": [["urls": ["stun:stun.provider.example:3478"]]]
            ],
            "policy": ["membership": membership],
            "issued_at": "2026-07-29T00:00:00Z"
        ], with: signer)
        return (body, String(decoding: keys, as: UTF8.self))
    }

    private func archive(_ name: String, digest: String) -> [String: String] {
        [
            "href": "https://publisher.example/\(name).actr",
            "type": "publisher.example/\(name)@1.0.0",
            "sha256": String(repeating: digest, count: 64)
        ]
    }

    private func sign(_ payload: [String: Any], with key: Curve25519.Signing.PrivateKey) throws -> Data {
        let body = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys, .withoutEscapingSlashes])
        let signature = try key.signature(for: body)
        var document = payload
        document["signature"] = [
            "key_id": "realm-key", "algorithm": "Ed25519", "value": signature.base64EncodedString()
        ]
        return try JSONSerialization.data(withJSONObject: document)
    }
}
