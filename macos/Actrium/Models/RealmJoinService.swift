import Foundation
import CryptoKit
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

    /// Recover admission by signing a membership request with this device's key.
    /// This also migrates clients that joined before confirmations were recorded.
    static func hasConfirmedMembership(_ profile: VerifiedProfile) async throws -> Bool {
        let membership = try membershipClient(for: profile.info)
        let keyId = membership.keyId()
        let confirmation = confirmationKey(for: profile.info, keyId: keyId)
        if UserDefaults.standard.bool(forKey: confirmation) { return true }

        let json: String
        do {
            json = try await membership.members()
        } catch ActrBindings.RealmMembershipError.Rejected(let status, let code)
            where status == 403 && code == "client_unknown" {
            return false
        }
        guard let reply = try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any],
              reply["realm"] as? String == profile.info.realmName,
              let members = reply["members"] as? [[String: Any]],
              members.contains(where: { member in
                  guard member["status"] as? String == "active",
                        let clients = member["clients"] as? [[String: Any]] else { return false }
                  return clients.contains { $0["key_id"] as? String == keyId && $0["revoked_at"] is NSNull }
              }) else {
            throw joinError("The Issuer did not confirm membership for this device.")
        }
        UserDefaults.standard.set(true, forKey: confirmation)
        return true
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

        return try await join(profile.info, inviteText: inviteText, label: label)
    }

    // MARK: - Helpers

    private static func inviteRealm(_ inviteText: String) throws -> String {
        guard let invite = try? ActriumCore.decodeActrInvite(inviteText: inviteText) else {
            throw joinError("The Realm invite code is invalid.")
        }
        return invite.realmName
    }

    private static func membershipClient(for profile: RealmProfileInfo) throws -> ActrBindings.RealmMembership {
        try RealmIdentityStore().membership(
            realm: profile.realmName, providerId: profile.providerId, issuer: profile.issuerUrl
        )
    }

    private static func confirmationKey(for profile: RealmProfileInfo, keyId: String) -> String {
        let scope = [profile.realmName, profile.providerId, profile.issuerUrl, keyId].joined(separator: "\u{0}")
        let digest = SHA256.hash(data: Data(scope.utf8)).map { String(format: "%02x", $0) }.joined()
        return "actr.realmMembership.\(digest)"
    }

    private static func join(
        _ profile: RealmProfileInfo,
        inviteText: String,
        label: String
    ) async throws -> RealmJoinResult {
        let membership = try membershipClient(for: profile)
        let keyId = membership.keyId()
        let json = try await membership.join(invite: inviteText, label: label)
        guard let member = try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any],
              member["realm"] as? String == profile.realmName,
              member["key_id"] as? String == keyId,
              let memberId = member["member_id"] as? String,
              let role = member["role"] as? String else {
            throw joinError("The join response could not be read.")
        }
        // Membership belongs to the captured signer. Its confirmation survives
        // the tab or Profile snapshot that initiated the request.
        UserDefaults.standard.set(true, forKey: confirmationKey(for: profile, keyId: keyId))
        return RealmJoinResult(memberId: memberId, role: role, keyId: keyId)
    }

    private static func joinError(_ message: String) -> NSError {
        NSError(
            domain: "ActrJoin", code: 1,
            userInfo: [NSLocalizedDescriptionKey: NSLocalizedString(
                message, comment: "Actr join error")])
    }
}
