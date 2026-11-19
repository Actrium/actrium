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
            body: body, realmName: realmName, signingKeysJSON: keys
        )
        XCTAssertTrue(verified.isOpen)
        XCTAssertEqual(verified.info.realmName, realmName)
    }

    @MainActor
    func testInviteOnlyPolicyCannotUseOpenJoin() async throws {
        let (body, keys) = try profile(membership: "invite-only")
        let verified = try RealmJoinService.VerifiedProfile(
            body: body, realmName: realmName, signingKeysJSON: keys
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
        XCTAssertThrowsError(try RealmJoinService.VerifiedProfile(
            body: changed, realmName: realmName, signingKeysJSON: keys
        ))
    }

    private func profile(membership: String) throws -> (Data, String) {
        let signer = Curve25519.Signing.PrivateKey()
        let publicKey = signer.publicKey.rawRepresentation.base64EncodedString()
        let keys = try sign([
            "sequence": 1,
            "keys": [["id": "realm-key", "algorithm": "Ed25519", "public_key": publicKey, "status": "active"]]
        ], with: signer)
        let body = try sign([
            "schema": "actr.realm/v1",
            "sequence": 1,
            "realm": ["name": realmName, "display_name": "Echo", "status": "active"],
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
