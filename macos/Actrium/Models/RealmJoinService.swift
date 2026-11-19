import Foundation
import ActriumCore
import Actr
import ActrBindings

/// The outcome of a successful Realm admission.
struct RealmJoinResult {
    /// Canonical Realm name `<realm_id>@<domain>`.
    let realmName: String
    let memberId: String
    let role: String
    let keyId: String
    /// `actr://<domain>/<realm_id>` — feed it to the existing open-instance
    /// flow (URI parsing → authority → profile → package).
    let instanceURL: URL
}

/// Authenticate the Realm and enroll this installation's device identity.
///
/// Reuses the same core verification entry points as the address-bar
/// discovery flow (`acceptAuthorityDescriptor`, `acceptRealmProfile`) and
/// the same device identity storage as #417 (`RealmIdentityStore`); it
/// changes nothing about how those steps verify.
enum RealmJoinService {
    struct Admission {
        fileprivate let profile: RealmProfileInfo
        fileprivate let instanceURL: URL
        let isOpen: Bool
    }

    private struct AdmissionDocument: Decodable {
        struct Policy: Decodable {
            let membership: String
        }

        let policy: Policy?
    }

    /// Redeem `inviteText` (an `ri_…` invite) and join as a new device.
    ///
    /// - Parameters:
    ///   - inviteText: the `ri_…` invite as received out of band.
    ///   - label: a human-readable client label (length-limited by the Issuer).
    ///   - allowInsecureDevelopment: opt-in HTTP/plaintext for dev stacks only.
    /// - Throws: `CoreError` for decode/verification failures, and
    ///   `RealmMembershipError` for redemption failures (`Rejected` carries
    ///   the stable codes `invite_used` / `invite_expired` / `invite_revoked`
    ///   / `invite_realm_mismatch`; `Uncertain` means the same key may retry
    ///   the same invite).
    static func joinWithInvite(
        inviteText: String,
        label: String,
        allowInsecureDevelopment: Bool = false
    ) async throws -> RealmJoinResult {
        // 1. Decode the invite locally: learn the Realm name. The secret
        //    stays inside the invite text; `join` consumes it in Rust.
        let invite = try ActriumCore.decodeActrInvite(inviteText: inviteText)
        let admission = try await Self.admission(for: invite.realmName)
        return try await join(
            admission, inviteText: inviteText, label: label,
            allowInsecureDevelopment: allowInsecureDevelopment
        )
    }

    static func joinOpen(_ admission: Admission, label: String) async throws -> RealmJoinResult {
        guard admission.isOpen else {
            throw joinError("This Realm requires an invite.")
        }
        return try await join(admission, inviteText: "", label: label, allowInsecureDevelopment: false)
    }

    static func admission(for realmName: String) async throws -> Admission {
        guard let at = realmName.firstIndex(of: "@") else {
            throw joinError("The Realm name is malformed.")
        }
        let realmId = String(realmName[..<at])
        let domain = String(realmName[realmName.index(after: at)...])
        guard !realmId.isEmpty, !domain.isEmpty else {
            throw joinError("The Realm name is malformed.")
        }

        guard let discoveryURL = URL(string: "https://\(domain)/.well-known/actr") else {
            throw joinError("The Realm domain is invalid.")
        }
        let descriptorBody = try await fetchBoundedDocument(discoveryURL)
        guard let descriptorText = String(data: descriptorBody, encoding: .utf8) else {
            throw joinError("The authority information is not valid text.")
        }
        let descriptor = try ActriumCore.acceptAuthorityDescriptor(
            body: descriptorText,
            domain: domain
        )

        // Authenticate the Profile before interpreting its admission policy.
        var profilePath = descriptor.realmsHref
        if profilePath.hasSuffix("/") { profilePath.removeLast() }
        profilePath += "/\(realmId)"
        guard let profileURL = URL(string: profilePath) else {
            throw joinError("The Realm Profile location is invalid.")
        }
        let profileBody = try await fetchBoundedDocument(profileURL)
        guard let profileText = String(data: profileBody, encoding: .utf8) else {
            throw joinError("The Realm Profile is not valid text.")
        }
        let profile = try ActriumCore.acceptRealmProfile(
            body: profileText,
            realmName: realmName,
            signingKeysJson: descriptor.signingKeysJson
        )

        let document = try JSONDecoder().decode(AdmissionDocument.self, from: profileBody)
        guard let instanceURL = URL(string: "actr://\(domain)/\(realmId)") else {
            throw joinError("The instance address could not be built.")
        }
        return Admission(profile: profile, instanceURL: instanceURL, isOpen: document.policy?.membership == "open")
    }

    private static func join(
        _ admission: Admission,
        inviteText: String,
        label: String,
        allowInsecureDevelopment: Bool
    ) async throws -> RealmJoinResult {
        let profile = admission.profile
        let store = try RealmIdentityStore()
        let membership = try store.membership(
            realm: profile.realmName,
            providerId: profile.providerId,
            issuer: profile.issuerUrl,
            allowInsecureDevelopment: allowInsecureDevelopment
        )
        let memberJSON = try await membership.join(invite: inviteText, label: label)

        guard let member = try JSONSerialization.jsonObject(with: Data(memberJSON.utf8))
                as? [String: Any],
            let memberId = member["member_id"] as? String,
            let role = member["role"] as? String,
            let keyId = member["key_id"] as? String else {
            throw joinError("The join response could not be read.")
        }
        return RealmJoinResult(
            realmName: profile.realmName,
            memberId: memberId,
            role: role,
            keyId: keyId,
            instanceURL: admission.instanceURL
        )
    }

    // MARK: - Helpers

    private static func joinError(_ message: String) -> NSError {
        NSError(
            domain: "ActrJoin", code: 1,
            userInfo: [NSLocalizedDescriptionKey: NSLocalizedString(
                message, comment: "Actr join error")])
    }

    private static func requireSuccess(_ response: URLResponse) throws {
        guard let http = response as? HTTPURLResponse,
              (200 ..< 300).contains(http.statusCode) else {
            let status = (response as? HTTPURLResponse)?.statusCode ?? -1
            throw NSError(
                domain: "ActrJoin", code: 2,
                userInfo: [NSLocalizedDescriptionKey: String(
                    format: NSLocalizedString(
                        "The request failed (HTTP %ld).",
                        comment: "Actr join fetch failure"),
                    status)])
        }
    }

    private static func requireHTTPS(_ response: URLResponse) throws {
        guard response.url?.scheme?.lowercased() == "https" else {
            throw NSError(
                domain: "ActrJoin", code: 3,
                userInfo: [NSLocalizedDescriptionKey: NSLocalizedString(
                    "The Realm information was served over an insecure connection.",
                    comment: "Actr join insecure transport error")])
        }
    }

    /// Bounded fetch for the join flow's discovery documents: HTTPS, 2xx, and
    /// a size cap so a hostile endpoint cannot exhaust memory.
    private static func fetchBoundedDocument(_ url: URL) async throws -> Data {
        let (data, response) = try await URLSession.shared.data(from: url)
        try requireHTTPS(response)
        try requireSuccess(response)
        let maxBytes = 4 * 1024 * 1024
        guard data.count <= maxBytes else {
            throw NSError(
                domain: "ActrJoin", code: 4,
                userInfo: [NSLocalizedDescriptionKey: NSLocalizedString(
                    "The Realm information exceeds the size limit.",
                    comment: "Actr join document size error")])
        }
        return data
    }
}
