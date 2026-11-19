import ActriumCore
import Foundation

/// Persists accepted trust before a Front is acquired or started.
@MainActor
enum RealmProfileStore {
    static let didRetire = Notification.Name("ActriumRealmDidRetire")

    struct AcceptedRealm {
        let info: RealmProfileInfo
        let signingKeyId: String
    }

    static func acceptAuthority(
        body: String, domain: String, realmName: String, preferences: UserDefaults
    ) throws -> AuthorityDiscoveryInfo {
        let key = authorityKey(domain)
        let previous = preferences.dictionary(forKey: key)
        let legacy = preferences.dictionary(forKey: realmKey(realmName))?["signingKeys"] as? String
        let accepted = try ActriumCore.acceptRealmAuthority(
            body: body, domain: domain, previousPinsJson: previous?["pins"] as? String,
            legacySigningKeysJson: legacy
        )
        preferences.set([
            "pins": accepted.pinsJson,
            "signingKeys": accepted.descriptor.signingKeysJson,
            "signingKeyIds": accepted.descriptor.signingKeys.map(\.id)
        ], forKey: key)
        return accepted.descriptor
    }

    static func acceptRealm(
        body: String, realmName: String, signingKeysJSON: String, preferences: UserDefaults
    ) throws -> AcceptedRealm {
        let key = realmKey(realmName)
        let documents = try preferences.dictionary(forKey: key).map { saved -> [String] in
            if let documents = saved["documents"] as? [String] { return documents }
            if let legacy = saved["body"] as? String { return [legacy] }
            throw CoreError.RealmProfileInvalid(message: "The accepted Realm Profile could not be read.")
        } ?? []
        // Authority discovery can advance in another Tab while this Profile is being fetched.
        let authority = authorityState(realmName, preferences: preferences)
        let currentKeys = authority?["signingKeys"] as? String ?? signingKeysJSON
        let acceptance = try ActriumCore.acceptRealmProfileSuccessor(
            body: body, realmName: realmName, signingKeysJson: currentKeys, previousDocuments: documents
        )
        var saved: [String: Any] = ["documents": acceptance.baselineDocuments, "signingKeys": currentKeys]
        if let profile = acceptance.profile {
            saved["sequence"] = String(profile.sequence)
        }
        preferences.set(saved, forKey: key)

        guard let active = acceptance.profile else {
            NotificationCenter.default.post(name: didRetire, object: preferences, userInfo: ["realm": realmName])
            throw CoreError.RealmRetired(message: realmName)
        }
        return AcceptedRealm(info: active, signingKeyId: acceptance.signingKeyId)
    }

    static func isCurrent(_ accepted: AcceptedRealm, preferences: UserDefaults) -> Bool {
        let profile = accepted.info
        let saved = preferences.dictionary(forKey: realmKey(profile.realmName))
        guard saved?["sequence"] as? String == String(profile.sequence) else { return false }
        if let ids = authorityState(profile.realmName, preferences: preferences)?["signingKeyIds"] as? [String] {
            return ids.contains(accepted.signingKeyId)
        }
        return true
    }

    static func acceptPublisher(
        body: String, domain: String, preferences: UserDefaults
    ) throws -> PublisherProfileInfo {
        let key = publisherKey(domain)
        let saved = preferences.dictionary(forKey: key)
        let pins = saved?["pins"] as? String ?? preferences.string(forKey: key)
        let accepted = try ActriumCore.acceptRealmPackagePublisher(
            body: body, domain: domain, previousPinsJson: pins
        )
        preferences.set(["body": body, "pins": accepted.pinsJson], forKey: key)
        return accepted.profile
    }

    /// Reevaluate a verified snapshot's key against the latest accepted Publisher Profile.
    static func requirePublisherKey(
        _ publicKey: Data, domain: String, localFront: LocalRealmFront? = nil, preferences: UserDefaults
    ) throws {
        guard let saved = preferences.dictionary(forKey: publisherKey(domain)),
              let body = saved["body"] as? String, let pins = saved["pins"] as? String else {
            throw RealmServiceError(message: "The accepted Publisher Profile is unavailable.")
        }
        let current = try ActriumCore.acceptRealmPackagePublisher(
            body: body, domain: domain, previousPinsJson: pins
        ).profile
        let allowed = localFront.map { current.verificationKey(for: $0) == publicKey }
            ?? current.fixedArchiveVerificationKeys.contains(publicKey)
        guard allowed else {
            throw RealmServiceError(message: "The Publisher key is no longer trusted for this package.")
        }
    }

    private static func authorityState(_ realmName: String, preferences: UserDefaults) -> [String: Any]? {
        guard let separator = realmName.firstIndex(of: "@") else { return nil }
        let domain = String(realmName[realmName.index(after: separator)...])
        return preferences.dictionary(forKey: authorityKey(domain))
    }

    private static func authorityKey(_ domain: String) -> String {
        "actr.acceptedRealmAuthority.\(domain)"
    }

    private static func publisherKey(_ domain: String) -> String {
        "actr.acceptedPublisherProfile.\(domain)"
    }

    private static func realmKey(_ realmName: String) -> String {
        "actr.acceptedRealmProfile.\(realmName)"
    }
}

extension PublisherProfileInfo {
    var fixedArchiveVerificationKeys: [Data] {
        packageSigningKeys.compactMap { key in
            guard key.status == "active" || key.status == "retired",
                  let bytes = Data(base64Encoded: key.publicKeyBase64), bytes.count == 32 else {
                return nil
            }
            return bytes
        }
    }

    func verificationKey(for localFront: LocalRealmFront) -> Data? {
        guard let key = packageSigningKeys.first(where: {
            $0.id == localFront.signingKeyId &&
                ($0.status == "active" || (!localFront.allowRebuild && $0.status == "retired"))
        }), let bytes = Data(base64Encoded: key.publicKeyBase64), bytes.count == 32 else {
            return nil
        }
        return bytes
    }
}
