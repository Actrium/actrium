import Actr
import ActrBindings
import CryptoKit
import Foundation
import Testing

private final class GovernanceSigner: RealmClientSigner, @unchecked Sendable {
    private let key = Curve25519.Signing.PrivateKey()
    func publicKey() throws -> Data { key.publicKey.rawRepresentation }
    func sign(payload: Data) throws -> Data { try key.signature(for: payload) }
}

struct RealmGovernanceTests {
    private func json(_ text: String) throws -> [String: Any] {
        try #require(JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
    }

    private func refused(_ code: String, _ call: () async throws -> String) async throws {
        do {
            _ = try await call()
            Issue.record("Expected membership refusal: \(code)")
        } catch RealmMembershipError.Rejected(let status, let actual) {
            #expect(actual == code)
            let expected: UInt16 = code == "invite_revoked" ? 410 :
                (["owner_must_transfer", "owner_last_client"].contains(code) ? 409 : 403)
            #expect(status == expected)
        }
    }

    @Test(.enabled(if: ProcessInfo.processInfo.environment["ACTR_GOVERNANCE_FIXTURE"] != nil,
        "Run tests/e2e/actr/member-governance/run.sh swift"))
    func realIssuerGovernanceAndRetryBoundaries() async throws {
        let path = try #require(ProcessInfo.processInfo.environment["ACTR_GOVERNANCE_FIXTURE"])
        let fixture = try JSONDecoder().decode([String: String].self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let realm = try #require(fixture["realm"])
        let provider = try #require(fixture["provider_id"])
        let issuer = try #require(fixture["issuer"])
        func member() throws -> Actr.RealmMembership {
            try Actr.RealmMembership(realm: realm, providerId: provider, issuer: issuer,
                signer: GovernanceSigner(), allowInsecureDevelopment: true)
        }
        let expires = ISO8601DateFormatter().string(from: Date().addingTimeInterval(3600))
        let owner = try member()
        let joined = try json(await owner.join(invite: #require(fixture["owner_invite"]), label: "Swift owner"))
        let ownerId = try #require(joined["member_id"] as? String)
        try await refused("owner_must_transfer") { try await owner.leave() }
        try await refused("owner_last_client") { try await owner.revokeClient(keyId: owner.keyId()) }
        let editorInvite = try json(await owner.inviteMember(role: .editor, expiresAt: expires))
        let viewerInvite = try json(await owner.inviteMember(role: .viewer, expiresAt: expires))
        let editor = try member()
        let viewer = try member()
        let editorJoin = try json(await editor.join(invite: #require(editorInvite["invite"] as? String), label: "Swift editor"))
        let viewerJoin = try json(await viewer.join(invite: #require(viewerInvite["invite"] as? String), label: "Swift viewer"))
        let editorId = try #require(editorJoin["member_id"] as? String)
        let viewerId = try #require(viewerJoin["member_id"] as? String)
        for client in [editor, viewer] {
            try await refused("forbidden") { try await client.inviteMember(role: .editor, expiresAt: expires) }
            try await refused("forbidden") { try await client.removeMember(memberId: editorId) }
            try await refused("forbidden") { try await client.setRole(memberId: editorId, role: .viewer) }
            try await refused("forbidden") { try await client.transferOwner(memberId: editorId, previousOwnerRole: .viewer) }
        }
        for _ in 0..<2 {
            #expect(try json(await owner.setRole(memberId: editorId, role: .viewer))["role"] as? String == "viewer")
        }
        let another = try json(await owner.inviteMember(role: .viewer, expiresAt: expires))
        #expect(another["invite_id"] as? String != viewerInvite["invite_id"] as? String)
        _ = try await owner.revokeInvite(inviteId: #require(another["invite_id"] as? String))
        let addDevice = try json(await editor.inviteClient(memberId: editorId, expiresAt: expires))
        let second = try member()
        _ = try await second.join(invite: #require(addDevice["invite"] as? String), label: "Swift second device")
        let unused = try json(await editor.inviteClient(memberId: editorId, expiresAt: expires))
        for _ in 0..<2 { _ = try await owner.removeMember(memberId: editorId) }
        try await refused("client_revoked") { try await editor.members() }
        try await refused("client_revoked") { try await second.members() }
        try await refused("invite_revoked") { try await member().join(invite: #require(unused["invite"] as? String), label: "late device") }
        let transferred = try json(await owner.transferOwner(memberId: viewerId, previousOwnerRole: .viewer))
        #expect(transferred["previous_owner"] as? String == ownerId)
        #expect(transferred["owner"] as? String == viewerId)
        try await refused("forbidden") { try await owner.transferOwner(memberId: viewerId, previousOwnerRole: .viewer) }
        let members = try #require(json(await viewer.members())["members"] as? [[String: Any]])
        #expect(members.filter { $0["role"] as? String == "owner" }.count == 1)
        _ = try await owner.leave()
        try await refused("client_revoked") { try await owner.leave() }
        _ = try await viewer.members()
    }
}
