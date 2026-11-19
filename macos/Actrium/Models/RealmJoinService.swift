import Foundation
import ActriumCore
import Actr
import ActrBindings

/// The outcome of a successful invite redemption.
struct RealmJoinResult {
    let memberId: String
    let role: String
    let keyId: String
}

/// Enroll this installation's device identity with a verified Realm.
enum RealmJoinService {
    struct VerifiedProfile {
        let info: RealmProfileInfo
        let isOpen: Bool

        init(body: Data, realmName: String, signingKeysJSON: String) throws {
            guard let text = String(data: body, encoding: .utf8) else {
                throw RealmJoinService.joinError("The Realm Profile is not valid text.")
            }
            info = try ActriumCore.acceptRealmProfile(
                body: text, realmName: realmName, signingKeysJson: signingKeysJSON
            )
            // Read the policy from the same bytes verified by the core.
            isOpen = try JSONDecoder().decode(AdmissionPolicy.self, from: Data(text.utf8)).policy.membership == "open"
        }
    }

    private struct AdmissionPolicy: Decodable {
        struct Policy: Decodable {
            let membership: String
        }

        let policy: Policy
    }

    static func joinOpen(_ profile: VerifiedProfile, label: String) async throws {
        guard profile.isOpen else {
            throw joinError("This Realm requires an invite.")
        }
        _ = try await join(profile.info, inviteText: "", label: label)
    }

    /// Resolve a standalone invite to its Realm address without including the code.
    static func address(forInvite inviteText: String) throws -> String {
        let realmName = try inviteRealm(inviteText)
        guard let at = realmName.firstIndex(of: "@") else {
            throw joinError("The invite names a malformed Realm.")
        }
        let realmId = String(realmName[..<at])
        let domain = String(realmName[realmName.index(after: at)...])
        guard !realmId.isEmpty, !domain.isEmpty else {
            throw joinError("The invite names a malformed Realm.")
        }

        return "actr://\(domain)/\(realmId)"
    }

    /// Redeem an explicit invite against the verified Profile for this navigation.
    static func joinWithInvite(
        _ profile: VerifiedProfile, inviteText: String, label: String
    ) async throws -> RealmJoinResult {
        guard try inviteRealm(inviteText) == profile.info.realmName else {
            throw joinError("This invite belongs to another Realm.")
        }

        let memberJSON = try await join(profile.info, inviteText: inviteText, label: label)
        guard let member = try JSONSerialization.jsonObject(with: Data(memberJSON.utf8))
                as? [String: Any],
            let memberId = member["member_id"] as? String,
            let role = member["role"] as? String,
            let keyId = member["key_id"] as? String else {
            throw joinError("The join response could not be read.")
        }
        return RealmJoinResult(memberId: memberId, role: role, keyId: keyId)
    }

    // MARK: - Helpers

    private static func inviteRealm(_ inviteText: String) throws -> String {
        guard let invite = try? ActriumCore.decodeActrInvite(inviteText: inviteText) else {
            throw joinError("The Realm invite code is invalid.")
        }
        return invite.realmName
    }

    private static func join(
        _ profile: RealmProfileInfo,
        inviteText: String,
        label: String
    ) async throws -> String {
        let membership = try RealmIdentityStore().membership(
            realm: profile.realmName,
            providerId: profile.providerId,
            issuer: profile.issuerUrl
        )
        return try await membership.join(invite: inviteText, label: label)
    }

    private static func joinError(_ message: String) -> NSError {
        NSError(
            domain: "ActrJoin", code: 1,
            userInfo: [NSLocalizedDescriptionKey: NSLocalizedString(
                message, comment: "Actr join error")])
    }
}
