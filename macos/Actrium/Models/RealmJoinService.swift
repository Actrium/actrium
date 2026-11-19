import Foundation
import ActriumCore
import Actr
import ActrBindings

/// The outcome of a successful invite redemption.
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

/// Enroll this installation's device identity with a verified Realm.
enum RealmJoinService {
    typealias InviteExchange = @MainActor (RealmProfileInfo, String, String, Bool) async throws -> String

    struct VerifiedProfile {
        let accepted: RealmProfileStore.AcceptedRealm
        var info: RealmProfileInfo { accepted.info }
        let isOpen: Bool

        init(body: Data, realmName: String, signingKeysJSON: String, preferences: UserDefaults) throws {
            guard let text = String(data: body, encoding: .utf8) else {
                throw RealmJoinService.joinError("The Realm Profile is not valid text.")
            }
            let accepted = try RealmProfileStore.acceptRealm(
                body: text, realmName: realmName, signingKeysJSON: signingKeysJSON, preferences: preferences
            )
            try self.init(body: Data(text.utf8), accepted: accepted)
        }

        init(body: Data, accepted: RealmProfileStore.AcceptedRealm) throws {
            self.accepted = accepted
            // Read the policy from the same bytes verified by the core.
            isOpen = try JSONDecoder().decode(AdmissionPolicy.self, from: body).policy.membership == "open"
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
        _ = try await join(profile.info, inviteText: "", label: label, allowInsecureDevelopment: false)
    }

    static func resolveProfile(
        at url: URL,
        realmName: String,
        signingKeysJSON: String,
        preferences: UserDefaults,
        fetch: @MainActor (URL) async throws -> Data,
        checkCurrent: @MainActor () throws -> Void
    ) async throws -> (body: Data, admission: VerifiedProfile) {
        do {
            let body = try await fetch(url)
            try checkCurrent()
            let verified = try VerifiedProfile(
                body: body, realmName: realmName, signingKeysJSON: signingKeysJSON, preferences: preferences
            )
            return (body, verified)
        } catch CoreError.RealmRetired(let name) {
            throw CoreError.RealmRetired(message: name)
        } catch {
            if error is CancellationError || (error as? URLError)?.code == .cancelled { throw error }
            try checkCurrent()
            guard let fallback = try RealmProfileStore.fallbackRealm(
                realmName, preferences: preferences
            ) else { throw error }
            AppLogger.viewModel.warning("Using the last trusted Realm Profile for \(realmName) after signing key revocation.")
            return (fallback.body, try VerifiedProfile(body: fallback.body, accepted: fallback.accepted))
        }
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
        allowInsecureDevelopment: Bool = false,
        preferences: UserDefaults = .standard,
        fetchDocument: (@MainActor (URL) async throws -> Data)? = nil,
        exchange: InviteExchange? = nil,
        checkCurrent: @MainActor () throws -> Void = { try Task.checkCancellation() }
    ) async throws -> RealmJoinResult {
        try checkCurrent()
        let fetch = fetchDocument ?? Self.fetchBoundedDocument
        // 1. Decode the invite locally: learn the Realm name. The secret
        //    stays inside the invite text; `join` consumes it in Rust.
        let invite = try ActriumCore.decodeActrInvite(inviteText: inviteText)
        let realmName = invite.realmName

        guard let at = realmName.firstIndex(of: "@") else {
            throw joinError("The invite names a malformed Realm.")
        }
        let realmId = String(realmName[..<at])
        let domain = String(realmName[realmName.index(after: at)...])
        guard !realmId.isEmpty, !domain.isEmpty else {
            throw joinError("The invite names a malformed Realm.")
        }

        // 2. Realm discovery: the authority descriptor for the Realm domain.
        guard let discoveryURL = URL(string: "https://\(domain)/.well-known/actr") else {
            throw joinError("The Realm domain in the invite is invalid.")
        }
        let descriptorBody = try await fetch(discoveryURL)
        try checkCurrent()
        guard let descriptorText = String(data: descriptorBody, encoding: .utf8) else {
            throw joinError("The authority information is not valid text.")
        }
        let descriptor = try RealmProfileStore.acceptAuthority(
            body: descriptorText,
            domain: domain,
            realmName: realmName,
            preferences: preferences
        )

        // 3. Realm Profile: verify it with the keys accepted in step 2 and
        //    learn the membership endpoints (issuer, provider id).
        var profilePath = descriptor.realmsHref
        if profilePath.hasSuffix("/") { profilePath.removeLast() }
        profilePath += "/\(realmId)"
        guard let profileURL = URL(string: profilePath) else {
            throw joinError("The Realm Profile location is invalid.")
        }
        let resolved = try await resolveProfile(
            at: profileURL, realmName: realmName, signingKeysJSON: descriptor.signingKeysJson,
            preferences: preferences, fetch: fetch, checkCurrent: checkCurrent
        )
        try checkCurrent()
        guard RealmProfileStore.isCurrent(resolved.admission.accepted, preferences: preferences) else {
            throw joinError("The verified Realm Profile changed before redemption.")
        }
        let profile = resolved.admission.info

        // 4. Device identity (created and persisted on first use) and the
        //    invite exchange. The same signer later registers the Host.
        let redeem = exchange ?? Self.join(_:inviteText:label:allowInsecureDevelopment:)
        let memberJSON = try await redeem(profile, inviteText, label, allowInsecureDevelopment)
        try checkCurrent()

        // 5. Surface the redemption result and the instance address.
        guard let member = try JSONSerialization.jsonObject(with: Data(memberJSON.utf8))
                as? [String: Any],
            let memberId = member["member_id"] as? String,
            let role = member["role"] as? String,
            let keyId = member["key_id"] as? String else {
            throw joinError("The join response could not be read.")
        }
        guard let instanceURL = URL(string: "actr://\(domain)/\(realmId)") else {
            throw joinError("The instance address could not be built.")
        }
        return RealmJoinResult(
            realmName: profile.realmName,
            memberId: memberId,
            role: role,
            keyId: keyId,
            instanceURL: instanceURL
        )
    }

    // MARK: - Helpers

    private static func join(
        _ profile: RealmProfileInfo,
        inviteText: String,
        label: String,
        allowInsecureDevelopment: Bool
    ) async throws -> String {
        let membership = try RealmIdentityStore().membership(
            realm: profile.realmName,
            providerId: profile.providerId,
            issuer: profile.issuerUrl,
            allowInsecureDevelopment: allowInsecureDevelopment
        )
        return try await membership.join(invite: inviteText, label: label)
    }

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
