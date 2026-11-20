import Foundation
import CryptoKit
import ActriumCore
import Actr
import ActrBindings

/// The outcome of a successful invite redemption.
struct RealmJoinResult {
    let admission: RealmJoinService.VerifiedProfile
    /// Canonical Realm name `<realm_id>@<domain>`.
    var realmName: String { admission.info.realmName }
    let memberId: String
    let role: String
    let keyId: String
    /// `actr://<domain>/<realm_id>` — feed it to the existing open-instance
    /// flow (URI parsing → authority → profile → package).
    let instanceURL: URL
}

/// Coordinates invite priority across windows, which share the same Realm client keys.
@MainActor
final class PendingRealmInvites {
    static let shared = PendingRealmInvites()

    private struct Invite {
        let realmName: String
        let code: String
        var exchanging = false
    }

    private var invites: [UUID: Invite] = [:]
    private var waiters: [String: [CheckedContinuation<Void, Never>]] = [:]

    func register(_ load: UUID, code: String, for realmName: String) {
        invites[load] = Invite(realmName: realmName, code: code)
    }

    func contains(code: String) -> Bool {
        invites.values.contains { $0.code == code }
    }

    func exchangeStarted(_ load: UUID) {
        invites[load]?.exchanging = true
    }

    func cancelBeforeExchange(_ load: UUID) {
        guard invites[load]?.exchanging == false else { return }
        finish(load)
    }

    func finish(_ load: UUID) {
        guard let realmName = invites.removeValue(forKey: load)?.realmName,
              !invites.values.contains(where: { $0.realmName == realmName }) else { return }
        waiters.removeValue(forKey: realmName)?.forEach { $0.resume() }
    }

    func wait(for realmName: String) async {
        while invites.values.contains(where: { $0.realmName == realmName }) {
            await withCheckedContinuation { continuation in
                waiters[realmName, default: []].append(continuation)
            }
        }
    }
}

/// Enroll this installation's device identity with a verified Realm.
enum RealmJoinService {
    typealias InviteExchange = @MainActor (RealmProfileInfo, String, String, Bool) async throws -> String

    enum Invite {
        case explicit(String)
        case sharedLink(String)

        var code: String {
            switch self {
            case .explicit(let code), .sharedLink(let code): return code
            }
        }
    }

    typealias InviteAdmission = (profile: VerifiedProfile, result: RealmJoinResult?)

    struct VerifiedProfile {
        let body: Data
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
            self.body = body
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

    static func joinOpen(_ profile: VerifiedProfile, label: String, preferences: UserDefaults = .standard) async throws {
        guard profile.isOpen else {
            throw joinError("This Realm requires an invite.")
        }
        _ = try await join(profile.info, inviteText: "", label: label, allowInsecureDevelopment: false, preferences: preferences)
    }

    static func resolveProfile(
        at url: URL,
        realmName: String,
        signingKeysJSON: String,
        preferences: UserDefaults,
        fetch: @MainActor (URL) async throws -> Data,
        checkCurrent: @MainActor () throws -> Void,
        validateProfile: @MainActor (Data) throws -> Void = { _ in }
    ) async throws -> VerifiedProfile {
        do {
            let body = try await fetch(url)
            try checkCurrent()
            try validateProfile(body)
            return try VerifiedProfile(
                body: body, realmName: realmName, signingKeysJSON: signingKeysJSON, preferences: preferences
            )
        } catch CoreError.RealmRetired(let name) {
            throw CoreError.RealmRetired(message: name)
        } catch {
            if error is CancellationError || (error as? URLError)?.code == .cancelled { throw error }
            try checkCurrent()
            guard let fallback = try RealmProfileStore.fallbackRealm(
                realmName, preferences: preferences
            ) else { throw error }
            try validateProfile(fallback.body)
            AppLogger.viewModel.warning("Using the last trusted Realm Profile for \(realmName) after signing key revocation.")
            return try VerifiedProfile(body: fallback.body, accepted: fallback.accepted)
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
        return try await redeemVerifiedInvite(
            resolved, code: inviteText, label: label, allowInsecureDevelopment: allowInsecureDevelopment,
            preferences: preferences, exchange: exchange, checkCurrent: checkCurrent
        )
    }

    static func address(forInvite code: String) throws -> String {
        let realm = try inviteRealm(code)
        let parts = realm.split(separator: "@", maxSplits: 1)
        guard parts.count == 2 else { throw joinError("The invite names a malformed Realm.") }
        return "actr://\(parts[1])/\(parts[0])"
    }

    static func joinWithInvite(
        _ admission: VerifiedProfile, invite: Invite, label: String, preferences: UserDefaults,
        exchange: InviteExchange?, checkMembership: @MainActor (VerifiedProfile) async throws -> Bool,
        checkCurrent: @MainActor () throws -> Void,
        validateProfile: @MainActor (VerifiedProfile) throws -> Void,
        beforeExchange: @MainActor () -> Void
    ) async throws -> InviteAdmission {
        guard try inviteRealm(invite.code) == admission.info.realmName else {
            throw joinError("This invite belongs to another Realm.")
        }

        var current = admission
        while case .sharedLink = invite {
            try checkCurrent()
            current = try currentProfile(current, preferences: preferences)
            let alreadyMember = try await checkMembership(current)
            try checkCurrent()
            guard RealmProfileStore.isCurrent(current.accepted, preferences: preferences) else { continue }
            if alreadyMember {
                try validateProfile(current)
                return (current, nil)
            }
            break
        }
        let result = try await redeemVerifiedInvite(
            current, code: invite.code, label: label, preferences: preferences, exchange: exchange,
            checkCurrent: checkCurrent, validateProfile: validateProfile, beforeExchange: beforeExchange
        )
        return (result.admission, result)
    }

    private static func currentProfile(_ profile: VerifiedProfile, preferences: UserDefaults) throws -> VerifiedProfile {
        if RealmProfileStore.isCurrent(profile.accepted, preferences: preferences) { return profile }
        guard let updated = try RealmProfileStore.currentRealm(profile.info.realmName, preferences: preferences) else {
            throw joinError("The verified Realm Profile changed before redemption.")
        }
        return try VerifiedProfile(body: updated.body, accepted: updated.accepted)
    }

    private static func redeemVerifiedInvite(
        _ admission: VerifiedProfile, code: String, label: String, allowInsecureDevelopment: Bool = false,
        preferences: UserDefaults, exchange: InviteExchange?, checkCurrent: @MainActor () throws -> Void,
        validateProfile: @MainActor (VerifiedProfile) throws -> Void = { _ in },
        beforeExchange: @MainActor () -> Void = {}
    ) async throws -> RealmJoinResult {
        try checkCurrent()
        // Refresh under the same admission reservation before sending the code.
        let current = try currentProfile(admission, preferences: preferences)
        try validateProfile(current)
        guard RealmProfileStore.isCurrent(current.accepted, preferences: preferences) else {
            throw joinError("The verified Realm Profile changed before redemption.")
        }
        let address = try address(forInvite: code)
        beforeExchange()
        let json = try await exchangeInvite(
            current.info, code: code, label: label, allowInsecureDevelopment: allowInsecureDevelopment,
            preferences: preferences, exchange: exchange
        )
        // The device keeps confirmed membership even if the tab has closed.
        return try joinResult(json, admission: current, address: address)
    }

    static func hasConfirmedMembership(_ profile: VerifiedProfile, preferences: UserDefaults) async throws -> Bool {
        let membership = try membershipClient(for: profile.info, allowInsecureDevelopment: false)
        let keyId = membership.keyId()
        let confirmation = confirmationKey(for: profile.info, keyId: keyId)
        if preferences.bool(forKey: confirmation) { return true }
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
        preferences.set(true, forKey: confirmation)
        return true
    }

    private static func inviteRealm(_ code: String) throws -> String {
        guard let invite = try? ActriumCore.decodeActrInvite(inviteText: code) else {
            throw joinError("The Realm invite code is invalid.")
        }
        return invite.realmName
    }

    private static func exchangeInvite(
        _ profile: RealmProfileInfo, code: String, label: String, allowInsecureDevelopment: Bool,
        preferences: UserDefaults, exchange: InviteExchange?
    ) async throws -> String {
        if let exchange { return try await exchange(profile, code, label, allowInsecureDevelopment) }
        return try await join(
            profile, inviteText: code, label: label,
            allowInsecureDevelopment: allowInsecureDevelopment, preferences: preferences
        )
    }

    private static func joinResult(_ memberJSON: String, admission: VerifiedProfile, address: String) throws -> RealmJoinResult {
        guard let member = try JSONSerialization.jsonObject(with: Data(memberJSON.utf8))
                as? [String: Any],
            let memberId = member["member_id"] as? String,
            let role = member["role"] as? String,
            let keyId = member["key_id"] as? String else {
            throw joinError("The join response could not be read.")
        }
        guard let instanceURL = URL(string: address) else {
            throw joinError("The instance address could not be built.")
        }
        return RealmJoinResult(
            admission: admission,
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
        allowInsecureDevelopment: Bool,
        preferences: UserDefaults
    ) async throws -> String {
        let membership = try membershipClient(for: profile, allowInsecureDevelopment: allowInsecureDevelopment)
        let keyId = membership.keyId()
        let json = try await membership.join(invite: inviteText, label: label)
        guard let member = try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any],
              member["realm"] as? String == profile.realmName, member["key_id"] as? String == keyId else {
            throw joinError("The join response could not be read.")
        }
        // This confirmation belongs to the signer even if the navigation was cancelled.
        preferences.set(true, forKey: confirmationKey(for: profile, keyId: keyId))
        return json
    }

    private static func membershipClient(
        for profile: RealmProfileInfo, allowInsecureDevelopment: Bool
    ) throws -> ActrBindings.RealmMembership {
        try RealmIdentityStore().membership(
            realm: profile.realmName,
            providerId: profile.providerId,
            issuer: profile.issuerUrl,
            allowInsecureDevelopment: allowInsecureDevelopment
        )
    }

    private static func confirmationKey(for profile: RealmProfileInfo, keyId: String) -> String {
        let scope = [profile.realmName, profile.providerId, profile.issuerUrl, keyId].joined(separator: "\u{0}")
        let digest = SHA256.hash(data: Data(scope.utf8)).map { String(format: "%02x", $0) }.joined()
        return "actr.realmMembership.\(digest)"
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
